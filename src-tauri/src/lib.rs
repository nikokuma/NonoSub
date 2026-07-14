mod chunking;
mod media;
mod openai;
mod pipeline;

use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex}};
use tauri::{ipc::Channel, Manager, State};

const KEYRING_SERVICE: &str = "com.nono.nonosub";
const KEYRING_ACCOUNT: &str = "openai-api-key";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum LearnerLevel {
    Beginner,
    Intermediate,
    Advanced,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SegmentStatus {
    Pending,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SubtitleSegment {
    id: String,
    start_ms: u64,
    end_ms: u64,
    source_text: String,
    natural_english: Option<String>,
    ambiguity_note: Option<String>,
    speaker_id: String,
    transcription_status: SegmentStatus,
    translation_status: SegmentStatus,
}

#[derive(Debug, Default)]
struct AppState {
    selected_media: Mutex<Option<PathBuf>>,
    prepared_session: Mutex<Option<PreparedSession>>,
    cancelled: Arc<AtomicBool>,
}

#[derive(Debug)]
struct PreparedSession {
    _directory: tempfile::TempDir,
    audio: Arc<media::DecodedAudio>,
    chunks: Vec<chunking::AudioChunk>,
}

#[derive(Debug, Serialize)]
struct ApiKeyStatus {
    present: bool,
}

#[derive(Debug, Serialize)]
struct PreparedMedia {
    path: String,
    file_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreparedAudio {
    duration_ms: u64,
    chunk_count: usize,
    sample_rate: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TutorMessage {
    role: String,
    text: String,
}

fn keyring_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|error| format!("Could not access the operating-system credential vault: {error}"))
}

#[tauri::command]
fn api_key_status() -> Result<ApiKeyStatus, String> {
    let present = match keyring_entry()?.get_password() {
        Ok(value) => !value.trim().is_empty(),
        Err(keyring::Error::NoEntry) => false,
        Err(error) => return Err(format!("Could not read the API key status: {error}")),
    };
    Ok(ApiKeyStatus { present })
}

#[tauri::command]
fn save_api_key(api_key: String) -> Result<ApiKeyStatus, String> {
    let trimmed = api_key.trim();
    if !trimmed.starts_with("sk-") || trimmed.len() < 20 {
        return Err("Enter a valid OpenAI API key beginning with sk-.".into());
    }
    keyring_entry()?
        .set_password(trimmed)
        .map_err(|error| format!("Could not save the API key: {error}"))?;
    Ok(ApiKeyStatus { present: true })
}

#[tauri::command]
async fn validate_model_access() -> Result<(), openai::ApiError> {
    let api_key = keyring_entry()
        .map_err(|message| openai::ApiError { kind: openai::ApiErrorKind::Authentication, message, retryable: false })?
        .get_password()
        .map_err(|_| openai::ApiError { kind: openai::ApiErrorKind::Authentication, message: "Save an OpenAI API key first.".into(), retryable: false })?;
    openai::OpenAiClient::new(api_key)?.validate_model_access().await
}

#[tauri::command]
fn remove_api_key() -> Result<ApiKeyStatus, String> {
    match keyring_entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(ApiKeyStatus { present: false }),
        Err(error) => Err(format!("Could not remove the API key: {error}")),
    }
}

#[tauri::command]
fn prepare_media(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<PreparedMedia, String> {
    let requested = PathBuf::from(path);
    let canonical = requested
        .canonicalize()
        .map_err(|_| "The selected video is no longer accessible.".to_string())?;
    if !canonical.is_file() {
        return Err("Select a video file, not a folder.".into());
    }
    let extension = canonical
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if extension != "mp4" && extension != "mov" {
        return Err("NonoSub currently supports MP4 and MOV video files.".into());
    }

    let scope = app.asset_protocol_scope();
    if let Some(previous) = state.selected_media.lock().map_err(|_| "Media state is unavailable.")?.take() {
        let _ = scope.forbid_file(previous);
    }
    scope
        .allow_file(&canonical)
        .map_err(|error| format!("Could not grant temporary access to this video: {error}"))?;
    *state.selected_media.lock().map_err(|_| "Media state is unavailable.")? = Some(canonical.clone());

    let file_name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Selected video")
        .to_owned();
    Ok(PreparedMedia {
        path: canonical.to_string_lossy().into_owned(),
        file_name,
    })
}

#[tauri::command]
async fn prepare_audio(state: State<'_, AppState>) -> Result<PreparedAudio, String> {
    state.cancelled.store(false, Ordering::Relaxed);
    let path = state
        .selected_media
        .lock()
        .map_err(|_| "Media state is unavailable.")?
        .clone()
        .ok_or_else(|| "Choose a video before starting analysis.".to_string())?;
    let (directory, audio, chunks) = tauri::async_runtime::spawn_blocking(move || {
        let directory = tempfile::Builder::new().prefix("nonosub-session-").tempdir()
            .map_err(|error| format!("Could not create secure temporary audio storage: {error}"))?;
        let audio = media::decode_to_mono_16k(&path)?;
        let chunks = chunking::create_chunks(&audio, directory.path())?;
        Ok::<_, String>((directory, audio, chunks))
    })
    .await
    .map_err(|error| format!("Audio preparation stopped unexpectedly: {error}"))??;
    let duration_ms = (audio.samples.len() as u64 * 1_000) / audio.sample_rate as u64;
    let sample_rate = audio.sample_rate;
    let chunk_count = chunks.len();
    *state.prepared_session.lock().map_err(|_| "Audio state is unavailable.")? = Some(PreparedSession {
        _directory: directory,
        audio: Arc::new(audio),
        chunks,
    });
    Ok(PreparedAudio { duration_ms, chunk_count, sample_rate })
}

#[tauri::command]
async fn start_analysis(
    state: State<'_, AppState>,
    on_event: Channel<pipeline::PipelineEvent>,
) -> Result<(), openai::ApiError> {
    state.cancelled.store(false, Ordering::Relaxed);
    let (audio, chunks) = {
        let guard = state.prepared_session.lock().map_err(|_| openai::ApiError {
            kind: openai::ApiErrorKind::Service,
            message: "Prepared audio state is unavailable.".into(),
            retryable: false,
        })?;
        let session = guard.as_ref().ok_or_else(|| openai::ApiError {
            kind: openai::ApiErrorKind::Service,
            message: "Prepare audio before starting analysis.".into(),
            retryable: false,
        })?;
        (Arc::clone(&session.audio), session.chunks.clone())
    };
    let api_key = keyring_entry()
        .map_err(|message| openai::ApiError { kind: openai::ApiErrorKind::Authentication, message, retryable: false })?
        .get_password()
        .map_err(|_| openai::ApiError { kind: openai::ApiErrorKind::Authentication, message: "Save an OpenAI API key before analysis.".into(), retryable: false })?;
    let client = openai::OpenAiClient::new(api_key)?;
    pipeline::run(client, audio, chunks, Arc::clone(&state.cancelled), on_event).await
}

#[tauri::command]
async fn ask_nono(
    question: String,
    selected: SubtitleSegment,
    learner_level: LearnerLevel,
    context: Vec<SubtitleSegment>,
    thread: Vec<TutorMessage>,
    on_delta: Channel<String>,
) -> Result<(), openai::ApiError> {
    if question.trim().is_empty() {
        return Err(openai::ApiError { kind: openai::ApiErrorKind::Service, message: "Ask Nono a question first.".into(), retryable: false });
    }
    let api_key = keyring_entry()
        .map_err(|message| openai::ApiError { kind: openai::ApiErrorKind::Authentication, message, retryable: false })?
        .get_password()
        .map_err(|_| openai::ApiError { kind: openai::ApiErrorKind::Authentication, message: "Save an OpenAI API key before asking Nono.".into(), retryable: false })?;
    let client = openai::OpenAiClient::new(api_key)?;
    let lesson_context = serde_json::json!({
        "learner_level": learner_level,
        "selected_line": selected,
        "nearby_dialogue": context,
        "local_question_thread": thread,
        "question": question,
    });
    client.tutor_stream(&lesson_context, |delta| {
        on_delta.send(delta).map_err(|error| openai::ApiError {
            kind: openai::ApiErrorKind::Service,
            message: format!("The tutor panel disconnected: {error}"),
            retryable: false,
        })
    }).await
}

#[tauri::command]
fn cancel_session(state: State<'_, AppState>) -> Result<(), String> {
    state.cancelled.store(true, Ordering::Relaxed);
    *state.selected_media.lock().map_err(|_| "Media state is unavailable.")? = None;
    *state.prepared_session.lock().map_err(|_| "Audio state is unavailable.")? = None;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            api_key_status,
            save_api_key,
            validate_model_access,
            remove_api_key,
            prepare_media,
            prepare_audio,
            start_analysis,
            ask_nono,
            cancel_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NonoSub");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_contract_serializes_with_webview_field_names() {
        let segment = SubtitleSegment {
            id: "segment-1".into(),
            start_ms: 1200,
            end_ms: 2400,
            source_text: "何ですか？".into(),
            natural_english: Some("What is it?".into()),
            ambiguity_note: None,
            speaker_id: "speaker-1".into(),
            transcription_status: SegmentStatus::Complete,
            translation_status: SegmentStatus::Complete,
        };
        let value = serde_json::to_value(segment).expect("segment should serialize");
        assert_eq!(value["startMs"], 1200);
        assert_eq!(value["sourceText"], "何ですか？");
    }

    #[test]
    fn learner_levels_are_stable_snake_case_values() {
        assert_eq!(serde_json::to_string(&LearnerLevel::Advanced).unwrap(), "\"advanced\"");
    }
}
