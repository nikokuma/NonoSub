mod chunking;
mod contracts;
#[cfg(target_os = "macos")]
mod live;
mod media;
mod openai;
mod pipeline;

use contracts::{
    LanguageSettings, LearnerLevel, LessonCard, PreparedMediaInfo, RecoverableError,
    SequencedSessionEvent, SessionEvent, SessionMode, SessionSnapshot, SpeakerProfile,
    SubtitleSegment, TutorMessage,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tauri::{
    menu::{MenuBuilder, SubmenuBuilder},
    tray::TrayIconBuilder,
    Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder,
};

const KEYRING_SERVICE: &str = "com.nono.nonosub";
const KEYRING_ACCOUNT: &str = "openai-api-key";
const API_KEY_MARKER: &str = "api-key-configured";

#[derive(Debug)]
struct AppState {
    selected_media: Mutex<Option<PathBuf>>,
    playback_media: Mutex<Option<PathBuf>>,
    playback_directory: Mutex<Option<tempfile::TempDir>>,
    prepared_session: Mutex<Option<PreparedSession>>,
    cancelled: Arc<AtomicBool>,
    session_counter: AtomicU64,
    canonical: Mutex<SessionSnapshot>,
    lesson_cache: Mutex<HashMap<String, LessonCard>>,
    #[cfg(target_os = "macos")]
    live: live::LiveState,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            selected_media: Mutex::new(None),
            playback_media: Mutex::new(None),
            playback_directory: Mutex::new(None),
            prepared_session: Mutex::new(None),
            cancelled: Arc::new(AtomicBool::new(false)),
            session_counter: AtomicU64::new(0),
            canonical: Mutex::new(SessionSnapshot::default()),
            lesson_cache: Mutex::new(HashMap::new()),
            #[cfg(target_os = "macos")]
            live: live::LiveState::default(),
        }
    }
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
struct ModelReadiness {
    file: bool,
    live: bool,
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

fn keyring_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|error| format!("Could not access the operating-system credential vault: {error}"))
}

fn development_api_key() -> Option<String> {
    #[cfg(debug_assertions)]
    {
        std::env::var("OPENAI_API_KEY")
            .ok()
            .map(|key| key.trim().to_owned())
            .filter(|key| !key.is_empty())
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

fn api_key_marker_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|directory| directory.join(API_KEY_MARKER))
        .map_err(|error| format!("Could not locate NonoSub's local settings: {error}"))
}

fn api_key_marker_exists(app: &tauri::AppHandle) -> bool {
    development_api_key().is_some() || api_key_marker_path(app).is_ok_and(|path| path.is_file())
}

fn write_api_key_marker(app: &tauri::AppHandle) -> Result<(), String> {
    let path = api_key_marker_path(app)?;
    let directory = path
        .parent()
        .ok_or_else(|| "Could not locate NonoSub's local settings directory.".to_string())?;
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("Could not create NonoSub's local settings: {error}"))?;
    std::fs::write(path, b"configured\n")
        .map_err(|error| format!("Could not update the API key status: {error}"))
}

fn remove_api_key_marker(app: &tauri::AppHandle) -> Result<(), String> {
    let path = api_key_marker_path(app)?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not clear the API key status: {error}")),
    }
}

fn api_key() -> Result<String, openai::ApiError> {
    if let Some(key) = development_api_key() {
        return Ok(key);
    }
    keyring_entry()
        .map_err(|message| openai::ApiError {
            kind: openai::ApiErrorKind::Authentication,
            message,
            retryable: false,
        })?
        .get_password()
        .map_err(|_| openai::ApiError {
            kind: openai::ApiErrorKind::Authentication,
            message: "Save an OpenAI API key first.".into(),
            retryable: false,
        })
}

#[tauri::command]
fn api_key_status(app: tauri::AppHandle) -> ApiKeyStatus {
    ApiKeyStatus {
        present: api_key_marker_exists(&app),
    }
}

#[tauri::command]
fn save_api_key(app: tauri::AppHandle, api_key: String) -> Result<ApiKeyStatus, String> {
    let trimmed = api_key.trim();
    if !trimmed.starts_with("sk-") || trimmed.len() < 20 {
        return Err("Enter a valid OpenAI API key beginning with sk-.".into());
    }
    keyring_entry()?
        .set_password(trimmed)
        .map_err(|error| format!("Could not save the API key: {error}"))?;
    write_api_key_marker(&app)?;
    Ok(ApiKeyStatus { present: true })
}

#[tauri::command]
async fn validate_model_access() -> Result<ModelReadiness, openai::ApiError> {
    let client = openai::OpenAiClient::new(api_key()?)?;
    client.validate_model_access().await?;
    let live = client
        .model_accessible(openai::REALTIME_TRANSLATION_MODEL)
        .await;
    Ok(ModelReadiness { file: true, live })
}

#[tauri::command]
fn remove_api_key(app: tauri::AppHandle) -> Result<ApiKeyStatus, String> {
    match keyring_entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {
            remove_api_key_marker(&app)?;
            Ok(ApiKeyStatus { present: false })
        }
        Err(error) => Err(format!("Could not remove the API key: {error}")),
    }
}

#[tauri::command]
fn get_session_snapshot(state: State<'_, AppState>) -> Result<SessionSnapshot, String> {
    state
        .canonical
        .lock()
        .map(|snapshot| snapshot.clone())
        .map_err(|_| "Session state is unavailable.".into())
}

#[tauri::command]
async fn prepare_media(
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

    #[cfg(target_os = "macos")]
    let (playback_path, playback_directory) = if media::needs_macos_playback_proxy(&canonical)? {
        let source = canonical.clone();
        let converted =
            tauri::async_runtime::spawn_blocking(move || create_macos_playback_proxy(&source))
                .await
                .map_err(|error| {
                    format!("Video compatibility preparation stopped unexpectedly: {error}")
                })??;
        (converted.1.clone(), Some(converted.0))
    } else {
        (canonical.clone(), None)
    };
    #[cfg(not(target_os = "macos"))]
    let (playback_path, playback_directory) = (canonical.clone(), None);

    let scope = app.asset_protocol_scope();
    scope
        .allow_file(&playback_path)
        .map_err(|error| format!("Could not grant temporary access to this video: {error}"))?;
    if let Some(previous) = state
        .playback_media
        .lock()
        .map_err(|_| "Media state is unavailable.")?
        .take()
    {
        if previous != playback_path {
            let _ = scope.forbid_file(previous);
        }
    }
    *state
        .selected_media
        .lock()
        .map_err(|_| "Media state is unavailable.")? = Some(canonical.clone());
    *state
        .playback_media
        .lock()
        .map_err(|_| "Media state is unavailable.")? = Some(playback_path.clone());
    *state
        .playback_directory
        .lock()
        .map_err(|_| "Media state is unavailable.")? = playback_directory;
    *state
        .prepared_session
        .lock()
        .map_err(|_| "Audio state is unavailable.")? = None;

    let file_name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Selected video")
        .to_owned();
    state
        .canonical
        .lock()
        .map_err(|_| "Session state is unavailable.")?
        .media = Some(PreparedMediaInfo {
        path: playback_path.to_string_lossy().into_owned(),
        file_name: file_name.clone(),
    });
    Ok(PreparedMedia {
        path: playback_path.to_string_lossy().into_owned(),
        file_name,
    })
}

#[cfg(target_os = "macos")]
fn create_macos_playback_proxy(
    source: &std::path::Path,
) -> Result<(tempfile::TempDir, PathBuf), String> {
    let directory = tempfile::Builder::new()
        .prefix("nonosub-playback-")
        .tempdir()
        .map_err(|error| format!("Could not create secure temporary video storage: {error}"))?;
    let output_path = directory.path().join("playback.m4v");
    let output = std::process::Command::new("/usr/bin/avconvert")
        .arg("--source")
        .arg(source)
        .arg("--preset")
        .arg("Preset1280x720")
        .arg("--output")
        .arg(&output_path)
        .arg("--replace")
        .arg("--disableMetadataFilter")
        .output()
        .map_err(|error| {
            format!("Could not start macOS video compatibility conversion: {error}")
        })?;
    if !output.status.success() || !output_path.is_file() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if detail.is_empty() {
            "macOS could not prepare this HEVC video for embedded playback.".into()
        } else {
            format!("macOS could not prepare this HEVC video for embedded playback: {detail}")
        });
    }
    Ok((directory, output_path))
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
        let directory = tempfile::Builder::new()
            .prefix("nonosub-session-")
            .tempdir()
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
    *state
        .prepared_session
        .lock()
        .map_err(|_| "Audio state is unavailable.")? = Some(PreparedSession {
        _directory: directory,
        audio: Arc::new(audio),
        chunks,
    });
    Ok(PreparedAudio {
        duration_ms,
        chunk_count,
        sample_rate,
    })
}

#[tauri::command]
async fn start_analysis(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    languages: LanguageSettings,
) -> Result<(), openai::ApiError> {
    state.cancelled.store(false, Ordering::Relaxed);
    let (audio, chunks) = {
        let guard = state
            .prepared_session
            .lock()
            .map_err(|_| service_error("Prepared audio state is unavailable."))?;
        let session = guard
            .as_ref()
            .ok_or_else(|| service_error("Prepare audio before starting analysis."))?;
        (Arc::clone(&session.audio), session.chunks.clone())
    };
    begin_session(&app, SessionMode::File, languages.clone())?;
    let client = openai::OpenAiClient::new(api_key()?)?;
    let sink_app = app.clone();
    let sink: pipeline::EventSink = Arc::new(move |event| record_event(&sink_app, event));
    pipeline::run(
        client,
        audio,
        chunks,
        languages,
        Arc::clone(&state.cancelled),
        sink,
    )
    .await
}

#[tauri::command]
async fn request_lesson(
    state: State<'_, AppState>,
    question: String,
    selected: SubtitleSegment,
    learner_level: LearnerLevel,
    context: Vec<SubtitleSegment>,
    thread: Vec<TutorMessage>,
) -> Result<LessonCard, openai::ApiError> {
    if question.trim().is_empty() {
        return Err(service_error("Ask Nono a question first."));
    }
    let languages = state
        .canonical
        .lock()
        .map_err(|_| service_error("Session state is unavailable."))?
        .languages
        .clone();
    let cache_key = format!(
        "{}::{learner_level:?}::{}",
        selected.id,
        question.trim().to_ascii_lowercase()
    );
    if let Some(card) = state
        .lesson_cache
        .lock()
        .map_err(|_| service_error("Lesson cache is unavailable."))?
        .get(&cache_key)
        .cloned()
    {
        return Ok(card);
    }
    let client = openai::OpenAiClient::new(api_key()?)?;
    let lesson_context = serde_json::json!({
        "learner_level": learner_level,
        "languages": languages,
        "selected_line": selected,
        "nearby_dialogue": context,
        "local_question_thread": thread,
        "question": question,
    });
    let first = client.lesson(&lesson_context).await;
    let card = match first {
        Err(error) if error.retryable => client.lesson(&lesson_context).await?,
        result => result?,
    };
    state
        .lesson_cache
        .lock()
        .map_err(|_| service_error("Lesson cache is unavailable."))?
        .insert(cache_key, card.clone());
    Ok(card)
}

#[tauri::command]
fn select_lesson_segment(
    app: tauri::AppHandle,
    segment_id: Option<String>,
) -> Result<(), openai::ApiError> {
    record_event(&app, SessionEvent::LessonSelected { segment_id })?;
    show_surface(&app, "lesson").map_err(|message| service_error(&message))?;
    app.emit("lesson-opened", ())
        .map_err(|error| service_error(&format!("Could not notify the player: {error}")))
}

#[tauri::command]
fn update_languages(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    languages: LanguageSettings,
) -> Result<(), String> {
    let (previous, mode, segments, speakers) = {
        let mut snapshot = state
            .canonical
            .lock()
            .map_err(|_| "Session state is unavailable.")?;
        let previous = snapshot.languages.clone();
        snapshot.languages = languages.clone();
        (
            previous,
            snapshot.mode.clone(),
            snapshot.segments.clone(),
            snapshot.speakers.clone(),
        )
    };
    if mode == Some(SessionMode::File)
        && previous.target != languages.target
        && !segments.is_empty()
    {
        let key = api_key().map_err(|error| error.message)?;
        tauri::async_runtime::spawn(async move {
            let _ = record_event(
                &app,
                SessionEvent::PhaseChanged {
                    phase: "translating".into(),
                },
            );
            let client = match openai::OpenAiClient::new(key) {
                Ok(client) => client,
                Err(error) => {
                    let _ = record_event(
                        &app,
                        SessionEvent::RecoverableError {
                            error: RecoverableError {
                                code: "retranslation_setup".into(),
                                message: error.message,
                                segment_id: None,
                            },
                        },
                    );
                    return;
                }
            };
            let inputs: Vec<openai::TranslationInput> = segments
                .iter()
                .map(|segment| openai::TranslationInput {
                    segment_id: segment.id.clone(),
                    speaker: segment
                        .speaker_id
                        .as_ref()
                        .and_then(|id| speakers.get(id))
                        .map(|speaker| speaker.display_name.clone())
                        .unwrap_or_else(|| "Speaker".into()),
                    source_text: segment.source_text.clone(),
                })
                .collect();
            for (batch_index, batch) in inputs.chunks(6).enumerate() {
                let start = batch_index * 6;
                let preceding_start = start.saturating_sub(80);
                let preceding = &inputs[preceding_start..start];
                let first = client.translate(preceding, batch, &languages).await;
                let outputs = match first {
                    Err(error) if error.retryable => {
                        client.translate(preceding, batch, &languages).await
                    }
                    result => result,
                };
                match outputs {
                    Ok(outputs) => {
                        for output in outputs {
                            let _ = record_event(
                                &app,
                                SessionEvent::TranslationFinalized {
                                    segment_id: output.segment_id,
                                    translation_text: output.translation,
                                    ambiguity_note: output.ambiguity_note,
                                },
                            );
                        }
                    }
                    Err(error) => {
                        let _ = record_event(
                            &app,
                            SessionEvent::RecoverableError {
                                error: RecoverableError {
                                    code: "retranslation_failed".into(),
                                    message: error.message,
                                    segment_id: batch
                                        .first()
                                        .map(|segment| segment.segment_id.clone()),
                                },
                            },
                        );
                    }
                }
            }
            let translated_through_ms = segments.last().map_or(0, |segment| segment.end_ms);
            let _ = record_event(
                &app,
                SessionEvent::CoverageChanged {
                    translated_through_ms,
                },
            );
            let _ = record_event(
                &app,
                SessionEvent::PhaseChanged {
                    phase: "ready".into(),
                },
            );
        });
    } else if mode == Some(SessionMode::Live) && previous.target != languages.target {
        #[cfg(target_os = "macos")]
        live::stop(&state.live);
        let _ = record_event(
            &app,
            SessionEvent::RecoverableError {
                error: RecoverableError {
                    code: "live_language_changed".into(),
                    message: "Target language changed. Start Live Captions again to apply it."
                        .into(),
                    segment_id: None,
                },
            },
        );
    }
    Ok(())
}

#[tauri::command]
fn update_speaker(app: tauri::AppHandle, speaker: SpeakerProfile) -> Result<(), openai::ApiError> {
    record_event(&app, SessionEvent::SpeakerDiscovered { speaker })
}

#[tauri::command]
fn open_surface(app: tauri::AppHandle, surface: String) -> Result<(), String> {
    show_surface(&app, &surface)
}

#[tauri::command]
fn hide_surface(app: tauri::AppHandle, surface: String) -> Result<(), String> {
    let label = surface_label(&surface)?;
    if let Some(window) = app.get_webview_window(label) {
        window.hide().map_err(|error| error.to_string())?;
    }
    update_activation_policy(&app);
    Ok(())
}

#[tauri::command]
fn cancel_session(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.cancelled.store(true, Ordering::Relaxed);
    #[cfg(target_os = "macos")]
    live::stop(&state.live);
    *state
        .selected_media
        .lock()
        .map_err(|_| "Media state is unavailable.")? = None;
    if let Some(path) = state
        .playback_media
        .lock()
        .map_err(|_| "Media state is unavailable.")?
        .take()
    {
        let _ = app.asset_protocol_scope().forbid_file(path);
    }
    *state
        .playback_directory
        .lock()
        .map_err(|_| "Media state is unavailable.")? = None;
    *state
        .prepared_session
        .lock()
        .map_err(|_| "Audio state is unavailable.")? = None;
    state
        .lesson_cache
        .lock()
        .map_err(|_| "Lesson cache is unavailable.")?
        .clear();
    record_event(&app, SessionEvent::Complete).map_err(|error| error.message)
}

#[cfg(target_os = "macos")]
#[tauri::command]
async fn start_live_capture(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    languages: LanguageSettings,
) -> Result<(), openai::ApiError> {
    begin_session(&app, SessionMode::Live, languages.clone())?;
    let key = api_key()?;
    match live::start(app.clone(), &state.live, key, languages).await {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = record_event(
                &app,
                SessionEvent::RecoverableError {
                    error: RecoverableError {
                        code: "live_start_failed".into(),
                        message: error.message.clone(),
                        segment_id: None,
                    },
                },
            );
            if let Some(window) = app.get_webview_window("overlay") {
                let _ = window.hide();
            }
            let _ = show_surface(&app, "workbench");
            Err(error)
        }
    }
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
async fn start_live_capture(_languages: LanguageSettings) -> Result<(), openai::ApiError> {
    Err(service_error(
        "Live system-audio captions are available on macOS 14 or later.",
    ))
}

#[tauri::command]
fn stop_live_capture(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    live::stop(&state.live);
    record_event(&app, SessionEvent::Complete).map_err(|error| error.message)?;
    Ok(())
}

fn begin_session(
    app: &tauri::AppHandle,
    mode: SessionMode,
    languages: LanguageSettings,
) -> Result<(), openai::ApiError> {
    let state = app.state::<AppState>();
    state.cancelled.store(true, Ordering::Relaxed);
    #[cfg(target_os = "macos")]
    live::stop(&state.live);
    state.cancelled.store(false, Ordering::Relaxed);
    state
        .lesson_cache
        .lock()
        .map_err(|_| service_error("Lesson cache is unavailable."))?
        .clear();
    let id = state.session_counter.fetch_add(1, Ordering::Relaxed) + 1;
    let media = state
        .canonical
        .lock()
        .map_err(|_| service_error("Session state is unavailable."))?
        .media
        .clone();
    *state
        .canonical
        .lock()
        .map_err(|_| service_error("Session state is unavailable."))? = SessionSnapshot {
        session_id: format!("session-{id}"),
        mode: Some(mode.clone()),
        languages: languages.clone(),
        media: if mode == SessionMode::File {
            media
        } else {
            None
        },
        ..SessionSnapshot::default()
    };
    record_event(app, SessionEvent::SessionReset { mode, languages })
}

pub(crate) fn record_event(
    app: &tauri::AppHandle,
    event: SessionEvent,
) -> Result<(), openai::ApiError> {
    let state = app.state::<AppState>();
    let envelope = {
        let mut snapshot = state
            .canonical
            .lock()
            .map_err(|_| service_error("Session state is unavailable."))?;
        snapshot.sequence += 1;
        apply_event(&mut snapshot, &event);
        SequencedSessionEvent {
            session_id: snapshot.session_id.clone(),
            sequence: snapshot.sequence,
            event,
        }
    };
    app.emit("session-event", envelope)
        .map_err(|error| service_error(&format!("Could not update subtitle windows: {error}")))
}

fn apply_event(snapshot: &mut SessionSnapshot, event: &SessionEvent) {
    match event {
        SessionEvent::SessionReset { mode, languages } => {
            snapshot.mode = Some(mode.clone());
            snapshot.languages = languages.clone();
            snapshot.phase = "preparing".into();
        }
        SessionEvent::PhaseChanged { phase } => snapshot.phase = phase.clone(),
        SessionEvent::CaptionUpserted { segment }
        | SessionEvent::TranscriptFinalized { segment } => {
            snapshot
                .segments
                .retain(|existing| existing.id != segment.id);
            snapshot.segments.push(segment.clone());
            snapshot.segments.sort_by_key(|segment| segment.start_ms);
        }
        SessionEvent::TranslationFinalized {
            segment_id,
            translation_text,
            ambiguity_note,
        } => {
            if let Some(segment) = snapshot
                .segments
                .iter_mut()
                .find(|segment| &segment.id == segment_id)
            {
                segment.translation_text = Some(translation_text.clone());
                segment.ambiguity_note = ambiguity_note.clone();
                segment.translation_status = contracts::SegmentStatus::Complete;
            }
        }
        SessionEvent::SpeakerDiscovered { speaker } => {
            snapshot
                .speakers
                .insert(speaker.id.clone(), speaker.clone());
        }
        SessionEvent::CoverageChanged {
            translated_through_ms,
        } => snapshot.translated_through_ms = *translated_through_ms,
        SessionEvent::LessonSelected { segment_id } => {
            snapshot.selected_segment_id = segment_id.clone()
        }
        SessionEvent::RecoverableError { error } => snapshot.errors.push(error.clone()),
        SessionEvent::FatalError { message } => snapshot.fatal_error = Some(message.clone()),
        SessionEvent::Complete => snapshot.phase = "complete".into(),
    }
}

fn show_surface(app: &tauri::AppHandle, surface: &str) -> Result<(), String> {
    let label = surface_label(surface)?;
    if let Some(window) = app.get_webview_window(label) {
        window.show().map_err(|error| error.to_string())?;
        let _ = window.set_focus();
        update_activation_policy(app);
        return Ok(());
    }
    let url = WebviewUrl::App(surface_path(surface).into());
    let mut builder = WebviewWindowBuilder::new(app, label, url).title("NonoSub");
    builder = match surface {
        "viewer" => builder
            .inner_size(1180.0, 720.0)
            .min_inner_size(720.0, 440.0)
            .decorations(false)
            .resizable(true),
        "overlay" => builder
            .inner_size(900.0, 220.0)
            .min_inner_size(520.0, 130.0)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .resizable(true),
        "lesson" => builder
            .inner_size(780.0, 620.0)
            .min_inner_size(620.0, 480.0)
            .decorations(false)
            .always_on_top(true)
            .resizable(true),
        "workbench" => builder
            .inner_size(1320.0, 840.0)
            .min_inner_size(960.0, 680.0)
            .resizable(true),
        _ => return Err("Unknown NonoSub surface.".into()),
    };
    builder.build().map_err(|error| error.to_string())?;
    update_activation_policy(app);
    Ok(())
}

fn surface_path(surface: &str) -> String {
    format!("?surface={surface}")
}

fn surface_label(surface: &str) -> Result<&'static str, String> {
    match surface {
        "workbench" => Ok("main"),
        "viewer" => Ok("viewer"),
        "overlay" => Ok("overlay"),
        "lesson" => Ok("lesson"),
        _ => Err("Unknown NonoSub surface.".into()),
    }
}

fn update_activation_policy(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let regular = ["main", "viewer"].iter().any(|label| {
            app.get_webview_window(label)
                .and_then(|window| window.is_visible().ok())
                .unwrap_or(false)
        });
        let _ = app.set_activation_policy(if regular {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        });
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let levels = SubmenuBuilder::new(app, "Learner level")
        .text("level_beginner", "Beginner")
        .text("level_intermediate", "Intermediate")
        .text("level_advanced", "Advanced")
        .build()?;
    let presets = SubmenuBuilder::new(app, "Subtitle preset")
        .text("preset_clean", "Clean")
        .text("preset_cinema", "Cinema")
        .text("preset_contrast", "High Contrast")
        .text("preset_nono-pop", "Nono Pop")
        .text("preset_manga", "Manga")
        .text("preset_retro", "Retro Pixel")
        .build()?;
    let menu = MenuBuilder::new(app)
        .text("open_video", "Open Video…")
        .text("start_live", "Start Live Captions…")
        .text("stop_session", "Stop Current Session")
        .separator()
        .text("toggle_subtitles", "Show / Hide Subtitles")
        .text("arrange_overlay", "Arrange Subtitle Overlay")
        .text("play_pause", "Play / Pause")
        .item(&presets)
        .item(&levels)
        .text("languages", "Languages…")
        .separator()
        .text("show_workbench", "Show Workbench")
        .text("quit", "Quit NonoSub")
        .build()?;
    let icon = app.default_window_icon().cloned();
    let mut tray = TrayIconBuilder::with_id("nonosub")
        .menu(&menu)
        .tooltip("NonoSub")
        .icon_as_template(true);
    if let Some(icon) = icon {
        tray = tray.icon(icon);
    }
    tray.on_menu_event(|app, event| {
        let id = event.id().as_ref();
        match id {
            "open_video" => {
                let _ = show_surface(app, "workbench");
                let _ = app.emit("tray-action", "open_video");
            }
            "start_live" => {
                let _ = show_surface(app, "workbench");
                let _ = app.emit("tray-action", "start_live");
            }
            "stop_session" => {
                let state = app.state::<AppState>();
                state.cancelled.store(true, Ordering::Relaxed);
                #[cfg(target_os = "macos")]
                live::stop(&state.live);
                let _ = record_event(app, SessionEvent::Complete);
            }
            "toggle_subtitles" | "arrange_overlay" | "play_pause" | "languages" => {
                let _ = app.emit("tray-action", id);
                if id == "languages" {
                    let _ = show_surface(app, "workbench");
                }
            }
            "show_workbench" => {
                let _ = show_surface(app, "workbench");
            }
            "quit" => app.exit(0),
            value if value.starts_with("preset_") || value.starts_with("level_") => {
                let _ = app.emit("tray-action", value);
            }
            _ => {}
        }
    })
    .build(app)?;
    Ok(())
}

fn service_error(message: &str) -> openai::ApiError {
    openai::ApiError {
        kind: openai::ApiErrorKind::Service,
        message: message.into(),
        retryable: false,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            setup_tray(app)?;
            let has_key = api_key_marker_exists(app.handle());
            let show_debug_workbench =
                cfg!(debug_assertions) && std::env::var_os("NONOSUB_SHOW_WORKBENCH").is_some();
            if has_key && !show_debug_workbench {
                let _ = app.get_webview_window("main").map(|window| window.hide());
                #[cfg(target_os = "macos")]
                let _ = app
                    .handle()
                    .set_activation_policy(tauri::ActivationPolicy::Accessory);
            } else if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                update_activation_policy(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            api_key_status,
            save_api_key,
            validate_model_access,
            remove_api_key,
            get_session_snapshot,
            prepare_media,
            prepare_audio,
            start_analysis,
            request_lesson,
            select_lesson_segment,
            update_languages,
            update_speaker,
            open_surface,
            hide_surface,
            cancel_session,
            start_live_capture,
            stop_live_capture,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NonoSub");
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::{SegmentStatus, SessionMode};

    #[test]
    fn canonical_contract_serializes_with_webview_field_names() {
        let segment = SubtitleSegment {
            id: "segment-1".into(),
            origin: SessionMode::File,
            start_ms: 1200,
            end_ms: 2400,
            source_text: "何ですか？".into(),
            translation_text: Some("What is it?".into()),
            ambiguity_note: None,
            speaker_id: Some("speaker-1".into()),
            is_provisional: false,
            transcription_status: SegmentStatus::Complete,
            translation_status: SegmentStatus::Complete,
        };
        let value = serde_json::to_value(segment).expect("segment should serialize");
        assert_eq!(value["startMs"], 1200);
        assert_eq!(value["translationText"], "What is it?");
    }

    #[test]
    fn secondary_surfaces_use_the_root_app_route() {
        assert_eq!(surface_path("overlay"), "?surface=overlay");
        assert!(!surface_path("lesson").contains("index.html"));
    }

    #[test]
    fn learner_levels_are_stable_snake_case_values() {
        assert_eq!(
            serde_json::to_string(&LearnerLevel::Advanced).unwrap(),
            "\"advanced\""
        );
    }
}
