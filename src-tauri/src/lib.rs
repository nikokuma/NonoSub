#![recursion_limit = "256"]

mod chunking;
mod contracts;
#[cfg(target_os = "macos")]
mod live;
mod media;
mod media_keys;
mod openai;
mod pipeline;

use contracts::{
    CaptionProcessingMode, LanguageSettings, LearnerLevel, LessonCard, LiveSyncMode, LiveSyncState,
    PreparedMediaInfo, RecoverableError, RetranslatedSegment, SegmentStatus,
    SequencedSessionEvent, SessionEvent, SessionMode, SessionSnapshot, SpeakerProfile,
    SubtitleSegment, TutorMessage,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, MutexGuard,
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
    selected_media: Mutex<Option<SelectedMedia>>,
    playback_media: Mutex<Option<PathBuf>>,
    playback_directory: Mutex<Option<tempfile::TempDir>>,
    prepared_session: Mutex<Option<PreparedSession>>,
    runs: RunCoordinator,
    retranslations: RetranslationCoordinator,
    canonical: Mutex<SessionSnapshot>,
    preferences: Mutex<Option<CanonicalPreferences>>,
    lesson_cache: Mutex<HashMap<String, LessonCard>>,
    launcher_mode: Mutex<String>,
    context_lesson_segment: Mutex<Option<String>>,
    context_lesson_source: Mutex<Option<String>>,
    lesson_open_context: Mutex<Option<LessonOpenContext>>,
    subtitles_visible: AtomicBool,
    external_media_paused_for_lesson: AtomicBool,
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
            runs: RunCoordinator::default(),
            retranslations: RetranslationCoordinator::default(),
            canonical: Mutex::new(SessionSnapshot::default()),
            preferences: Mutex::new(None),
            lesson_cache: Mutex::new(HashMap::new()),
            launcher_mode: Mutex::new("file".into()),
            context_lesson_segment: Mutex::new(None),
            context_lesson_source: Mutex::new(None),
            lesson_open_context: Mutex::new(None),
            subtitles_visible: AtomicBool::new(true),
            external_media_paused_for_lesson: AtomicBool::new(false),
            #[cfg(target_os = "macos")]
            live: live::LiveState::default(),
        }
    }
}

#[derive(Debug, Clone)]
struct SelectedMedia {
    generation: u64,
    path: PathBuf,
}

#[derive(Debug)]
struct PreparedSession {
    generation: u64,
    _directory: tempfile::TempDir,
    audio: Arc<media::DecodedAudio>,
    chunks: Vec<chunking::AudioChunk>,
}

#[derive(Debug, Clone)]
struct RunLease {
    generation: u64,
    cancelled: Arc<AtomicBool>,
}

impl RunLease {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Default)]
struct RunCoordinator {
    counter: AtomicU64,
    active: Mutex<Option<RunLease>>,
}

impl RunCoordinator {
    fn replace(&self) -> Result<RunLease, openai::ApiError> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| service_error("Session generation state is unavailable."))?;
        if let Some(previous) = active.take() {
            previous.cancelled.store(true, Ordering::Relaxed);
        }
        let run = RunLease {
            generation: self.counter.fetch_add(1, Ordering::Relaxed) + 1,
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        *active = Some(run.clone());
        Ok(run)
    }

    fn lease(&self, generation: u64) -> Result<RunLease, openai::ApiError> {
        let active = self
            .active
            .lock()
            .map_err(|_| service_error("Session generation state is unavailable."))?;
        active
            .as_ref()
            .filter(|run| run.generation == generation && !run.is_cancelled())
            .cloned()
            .ok_or_else(cancelled_error)
    }

    fn cancel(&self) -> Result<(), openai::ApiError> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| service_error("Session generation state is unavailable."))?;
        if let Some(run) = active.take() {
            run.cancelled.store(true, Ordering::Relaxed);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetranslationStatus {
    Running,
    Failed,
}

#[derive(Debug, Clone)]
struct RetranslationLease {
    session_generation: u64,
    request_generation: u64,
    languages: LanguageSettings,
    cancelled: Arc<AtomicBool>,
}

impl RetranslationLease {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
struct ActiveRetranslation {
    lease: RetranslationLease,
    status: RetranslationStatus,
}

#[derive(Debug, Default)]
struct RetranslationCoordinator {
    counter: AtomicU64,
    active: Mutex<Option<ActiveRetranslation>>,
}

impl RetranslationCoordinator {
    fn begin(
        &self,
        session_generation: u64,
        languages: LanguageSettings,
    ) -> Result<Option<RetranslationLease>, openai::ApiError> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| service_error("Retranslation generation state is unavailable."))?;
        if active.as_ref().is_some_and(|request| {
            request.lease.session_generation == session_generation
                && request.lease.languages == languages
                && !request.lease.is_cancelled()
        }) {
            return Ok(None);
        }
        if let Some(previous) = active.take() {
            previous.lease.cancelled.store(true, Ordering::Relaxed);
        }
        let lease = RetranslationLease {
            session_generation,
            request_generation: self.counter.fetch_add(1, Ordering::Relaxed) + 1,
            languages,
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        *active = Some(ActiveRetranslation {
            lease: lease.clone(),
            status: RetranslationStatus::Running,
        });
        Ok(Some(lease))
    }

    fn ensure_current(&self, lease: &RetranslationLease) -> Result<(), openai::ApiError> {
        let active = self
            .active
            .lock()
            .map_err(|_| service_error("Retranslation generation state is unavailable."))?;
        if lease.is_cancelled()
            || active.as_ref().is_none_or(|request| {
                request.status != RetranslationStatus::Running
                    || request.lease.session_generation != lease.session_generation
                    || request.lease.request_generation != lease.request_generation
            })
        {
            return Err(cancelled_error());
        }
        Ok(())
    }

    fn cancel(&self) -> Result<(), openai::ApiError> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| service_error("Retranslation generation state is unavailable."))?;
        if let Some(request) = active.take() {
            request.lease.cancelled.store(true, Ordering::Relaxed);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileSourceRevision {
    segment_id: String,
    start_ms: u64,
    end_ms: u64,
    source_text: String,
    speaker_id: Option<String>,
}

type ScopedRetranslationEnvelope<'a> = (
    MutexGuard<'a, Option<RunLease>>,
    MutexGuard<'a, Option<ActiveRetranslation>>,
    SequencedSessionEvent,
);

#[derive(Debug, Clone)]
struct CanonicalPreferences {
    revision: u64,
    preferences: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreferenceEnvelope {
    revision: u64,
    preferences: serde_json::Value,
    rebased: bool,
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
    generation: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreparedAudio {
    duration_ms: u64,
    chunk_count: usize,
    sample_rate: u32,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ExternalMediaControlResult {
    NotRequested,
    Paused,
    PermissionRequired,
    Failed,
    Unsupported,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LessonOpenContext {
    source_surface: String,
    segment_id: String,
    cursor_x: f64,
    cursor_y: f64,
    external_media_control: ExternalMediaControlResult,
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

fn merge_preference_patch(target: &mut serde_json::Value, patch: &serde_json::Value) {
    match (target, patch) {
        (serde_json::Value::Object(target), serde_json::Value::Object(patch)) => {
            for (key, value) in patch {
                if let Some(existing) = target.get_mut(key) {
                    merge_preference_patch(existing, value);
                } else {
                    target.insert(key.clone(), value.clone());
                }
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}

fn initialize_preference_state(
    state: &mut Option<CanonicalPreferences>,
    preferences: serde_json::Value,
) -> Result<PreferenceEnvelope, String> {
    if !preferences.is_object() {
        return Err("Preferences must be a JSON object.".into());
    }
    let current = state.get_or_insert(CanonicalPreferences {
        revision: 0,
        preferences,
    });
    Ok(PreferenceEnvelope {
        revision: current.revision,
        preferences: current.preferences.clone(),
        rebased: false,
    })
}

fn apply_preference_patch(
    state: &mut CanonicalPreferences,
    base_revision: u64,
    patch: serde_json::Value,
) -> Result<PreferenceEnvelope, String> {
    if !patch.is_object() {
        return Err("Preference patches must be JSON objects.".into());
    }
    let rebased = base_revision != state.revision;
    let mut merged = state.preferences.clone();
    merge_preference_patch(&mut merged, &patch);
    if !merged.is_object() {
        return Err("The merged preferences must remain a JSON object.".into());
    }
    if merged != state.preferences {
        state.revision = state.revision.saturating_add(1);
        state.preferences = merged;
    }
    Ok(PreferenceEnvelope {
        revision: state.revision,
        preferences: state.preferences.clone(),
        rebased,
    })
}

#[tauri::command]
fn initialize_preferences(
    state: State<'_, AppState>,
    preferences: serde_json::Value,
) -> Result<PreferenceEnvelope, String> {
    let mut canonical = state
        .preferences
        .lock()
        .map_err(|_| "Preference state is unavailable.".to_string())?;
    initialize_preference_state(&mut canonical, preferences)
}

#[tauri::command]
fn patch_preferences(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    base_revision: u64,
    patch: serde_json::Value,
) -> Result<PreferenceEnvelope, String> {
    let patches_languages = patch
        .as_object()
        .is_some_and(|object| object.contains_key("languages"));
    let mut canonical = state
        .preferences
        .lock()
        .map_err(|_| "Preference state is unavailable.".to_string())?;
    let current = canonical
        .as_mut()
        .ok_or_else(|| "Preferences have not been initialized.".to_string())?;
    let previous_revision = current.revision;
    let previous_languages = current.preferences.get("languages").cloned();
    let mut candidate = current.clone();
    let envelope = apply_preference_patch(&mut candidate, base_revision, patch)?;
    let languages_changed =
        patches_languages && previous_languages.as_ref() != envelope.preferences.get("languages");
    if languages_changed {
        let languages = serde_json::from_value::<LanguageSettings>(
            envelope
                .preferences
                .get("languages")
                .cloned()
                .ok_or_else(|| "Canonical language preferences are missing.".to_string())?,
        )
        .map_err(|_| "Canonical language preferences are invalid.".to_string())?;
        apply_language_settings(&app, state.inner(), languages)?;
    }
    *current = candidate;
    if envelope.revision != previous_revision {
        app.emit("preferences-updated", envelope.clone())
            .map_err(|error| format!("Could not update preference windows: {error}"))?;
    }
    Ok(envelope)
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

    state
        .retranslations
        .cancel()
        .map_err(|error| error.message)?;
    let run = state.runs.replace().map_err(|error| error.message)?;
    #[cfg(target_os = "macos")]
    live::stop(&state.live);

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

    let active_run = state
        .runs
        .active
        .lock()
        .map_err(|_| "Session generation state is unavailable.".to_string())?;
    if active_run
        .as_ref()
        .is_none_or(|active| active.generation != run.generation || active.is_cancelled())
    {
        return Err(cancelled_error().message);
    }
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
        .map_err(|_| "Media state is unavailable.")? = Some(SelectedMedia {
            generation: run.generation,
            path: canonical.clone(),
        });
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
        generation: run.generation,
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
async fn prepare_audio(
    state: State<'_, AppState>,
    generation: u64,
) -> Result<PreparedAudio, String> {
    let run = state
        .runs
        .lease(generation)
        .map_err(|error| error.message)?;
    let path = state
        .selected_media
        .lock()
        .map_err(|_| "Media state is unavailable.")?
        .as_ref()
        .filter(|media| media.generation == generation)
        .map(|media| media.path.clone())
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
    let active_run = state
        .runs
        .active
        .lock()
        .map_err(|_| "Session generation state is unavailable.".to_string())?;
    if active_run
        .as_ref()
        .is_none_or(|active| active.generation != run.generation || active.is_cancelled())
    {
        return Err(cancelled_error().message);
    }
    *state
        .prepared_session
        .lock()
        .map_err(|_| "Audio state is unavailable.")? = Some(PreparedSession {
        generation,
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
    generation: u64,
    languages: LanguageSettings,
    processing_mode: CaptionProcessingMode,
) -> Result<(), openai::ApiError> {
    let run = state.runs.lease(generation)?;
    let (audio, chunks) = {
        let guard = state
            .prepared_session
            .lock()
            .map_err(|_| service_error("Prepared audio state is unavailable."))?;
        let session = guard
            .as_ref()
            .filter(|session| session.generation == generation)
            .ok_or_else(|| service_error("Prepare audio before starting analysis."))?;
        (Arc::clone(&session.audio), session.chunks.clone())
    };
    begin_session_for_generation(
        &app,
        generation,
        SessionMode::File,
        languages.clone(),
        processing_mode.clone(),
    )?;
    let client = openai::OpenAiClient::new(api_key()?)?;
    let sink_app = app.clone();
    let sink: pipeline::EventSink =
        Arc::new(move |event| record_event_for_generation(&sink_app, generation, event));
    pipeline::run(
        client,
        audio,
        chunks,
        languages,
        processing_mode,
        Arc::clone(&run.cancelled),
        sink,
    )
    .await
}

#[tauri::command]
async fn retry_translation(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    segment_id: String,
) -> Result<(), openai::ApiError> {
    let generation = {
        let active = state
            .runs
            .active
            .lock()
            .map_err(|_| service_error("Session generation state is unavailable."))?;
        active
            .as_ref()
            .filter(|run| !run.is_cancelled())
            .map(|run| run.generation)
            .ok_or_else(cancelled_error)?
    };
    let (languages, context, batch, mut pending_segment) = {
        let snapshot = state
            .canonical
            .lock()
            .map_err(|_| service_error("Session state is unavailable."))?;
        if snapshot.mode != Some(SessionMode::File)
            || snapshot.processing_mode != CaptionProcessingMode::Translated
        {
            return Err(service_error(
                "Translation retry is available for translated file sessions.",
            ));
        }
        let index = snapshot
            .segments
            .iter()
            .position(|segment| segment.id == segment_id)
            .ok_or_else(|| service_error("That subtitle is no longer in the active session."))?;
        let selected = snapshot.segments[index].clone();
        if selected.translation_status != SegmentStatus::Failed {
            return Err(service_error(
                "That subtitle does not need a translation retry.",
            ));
        }
        let to_input = |segment: &SubtitleSegment| openai::TranslationInput {
            segment_id: segment.id.clone(),
            speaker: segment
                .speaker_id
                .clone()
                .unwrap_or_else(|| "speaker-unknown".into()),
            source_text: segment.source_text.clone(),
        };
        let context_start = index.saturating_sub(80);
        let context = snapshot.segments[context_start..index]
            .iter()
            .map(to_input)
            .collect::<Vec<_>>();
        let batch = vec![to_input(&selected)];
        let mut pending = selected;
        pending.translation_text = None;
        pending.ambiguity_note = None;
        pending.translation_status = SegmentStatus::Pending;
        (snapshot.languages.clone(), context, batch, pending)
    };

    let client = openai::OpenAiClient::new(api_key()?)?;
    record_event_for_generation(
        &app,
        generation,
        SessionEvent::TranscriptFinalized {
            segment: pending_segment.clone(),
        },
    )?;
    match pipeline::translate_batch_with_retry(&client, &context, &batch, &languages).await {
        Ok(mut translations) => {
            let translation = translations
                .pop()
                .ok_or_else(|| service_error("Translation retry returned no subtitle."))?;
            record_event_for_generation(
                &app,
                generation,
                SessionEvent::TranslationFinalized {
                    segment_id: translation.segment_id,
                    translation_text: translation.translation,
                    ambiguity_note: translation.ambiguity_note,
                },
            )
        }
        Err(error) => {
            pending_segment.translation_status = SegmentStatus::Failed;
            record_event_for_generation(
                &app,
                generation,
                SessionEvent::TranscriptFinalized {
                    segment: pending_segment,
                },
            )?;
            record_event_for_generation(
                &app,
                generation,
                SessionEvent::RecoverableError {
                    error: RecoverableError {
                        code: "translation_failed".into(),
                        message: error.message.clone(),
                        segment_id: Some(segment_id),
                    },
                },
            )?;
            Err(error)
        }
    }
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
        "lesson-v2::{}::{learner_level:?}::{}",
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
#[allow(clippy::too_many_arguments)] // Tauri exposes these as named webview command parameters.
fn open_lesson_composer(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    window: tauri::WebviewWindow,
    segment_id: String,
    source_surface: String,
    cursor_x: f64,
    cursor_y: f64,
    experimental_external_pause: bool,
) -> Result<(), String> {
    let expected_label = match source_surface.as_str() {
        "viewer" => "viewer",
        "overlay" => "overlay",
        "workbench" => "main",
        _ => return Err("Unknown lesson source surface.".into()),
    };
    if window.label() != expected_label {
        return Err("Lesson source window did not match the requested surface.".into());
    }
    let finalized = state
        .canonical
        .lock()
        .map_err(|_| "Session state is unavailable.".to_string())?
        .segments
        .iter()
        .any(|segment| segment.id == segment_id && !segment.is_provisional);
    if !finalized {
        return Err("Wait for this subtitle to finish before asking Nono.".into());
    }

    record_event(
        &app,
        SessionEvent::LessonSelected {
            segment_id: Some(segment_id.clone()),
        },
    )
    .map_err(|error| error.message)?;

    let external_media_control = if source_surface == "overlay" && experimental_external_pause {
        if !cfg!(target_os = "macos") {
            ExternalMediaControlResult::Unsupported
        } else if !media_keys::permission_status() {
            ExternalMediaControlResult::PermissionRequired
        } else if state
            .external_media_paused_for_lesson
            .load(Ordering::Relaxed)
        {
            ExternalMediaControlResult::Paused
        } else {
            match media_keys::post_play_pause() {
                Ok(()) => {
                    state
                        .external_media_paused_for_lesson
                        .store(true, Ordering::Relaxed);
                    ExternalMediaControlResult::Paused
                }
                Err(message) if message == "permission_required" => {
                    ExternalMediaControlResult::PermissionRequired
                }
                Err(_) => ExternalMediaControlResult::Failed,
            }
        }
    } else {
        ExternalMediaControlResult::NotRequested
    };

    show_surface(&app, "lesson")?;
    place_composer_near_cursor(&app, &window, cursor_x, cursor_y);
    let payload = LessonOpenContext {
        source_surface,
        segment_id,
        cursor_x,
        cursor_y,
        external_media_control,
    };
    *state
        .lesson_open_context
        .lock()
        .map_err(|_| "Lesson state is unavailable.".to_string())? = Some(payload.clone());
    app.emit("lesson-composer-opened", payload)
        .map_err(|error| format!("Could not open Ask Nono: {error}"))
}

#[tauri::command]
fn get_lesson_open_context(
    state: State<'_, AppState>,
) -> Result<Option<LessonOpenContext>, String> {
    state
        .lesson_open_context
        .lock()
        .map(|context| context.clone())
        .map_err(|_| "Lesson state is unavailable.".into())
}

#[tauri::command]
fn media_key_permission_status() -> bool {
    media_keys::permission_status()
}

#[tauri::command]
fn request_media_key_permission() -> bool {
    media_keys::request_permission()
}

#[tauri::command]
fn post_media_play_pause() -> ExternalMediaControlResult {
    match media_keys::post_play_pause() {
        Ok(()) => ExternalMediaControlResult::Paused,
        Err(message) if message == "permission_required" => {
            ExternalMediaControlResult::PermissionRequired
        }
        Err(message) if message == "unsupported" => ExternalMediaControlResult::Unsupported,
        Err(_) => ExternalMediaControlResult::Failed,
    }
}

#[tauri::command]
fn close_lesson_surface(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    close_lesson(&app, state.inner());
    Ok(())
}

fn file_source_revision(segments: &[SubtitleSegment]) -> Vec<FileSourceRevision> {
    segments
        .iter()
        .map(|segment| FileSourceRevision {
            segment_id: segment.id.clone(),
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            source_text: segment.source_text.clone(),
            speaker_id: segment.speaker_id.clone(),
        })
        .collect()
}

fn begin_file_retranslation(
    state: &AppState,
    session_generation: u64,
    languages: LanguageSettings,
) -> Result<Option<RetranslationLease>, openai::ApiError> {
    let run_guard = state
        .runs
        .active
        .lock()
        .map_err(|_| service_error("Session generation state is unavailable."))?;
    if run_guard.as_ref().is_none_or(|run| {
        run.generation != session_generation || run.is_cancelled()
    }) {
        return Err(cancelled_error());
    }
    state
        .retranslations
        .begin(session_generation, languages)
}

async fn completed_file_snapshot(
    app: &tauri::AppHandle,
    lease: &RetranslationLease,
) -> Result<SessionSnapshot, openai::ApiError> {
    loop {
        let state = app.state::<AppState>();
        state.runs.lease(lease.session_generation)?;
        state.retranslations.ensure_current(lease)?;
        let snapshot = state
            .canonical
            .lock()
            .map_err(|_| service_error("Session state is unavailable."))?
            .clone();
        if snapshot.session_id != format!("session-{}", lease.session_generation) {
            return Err(cancelled_error());
        }
        if let Some(message) = snapshot.fatal_error.as_ref() {
            return Err(service_error(&format!(
                "The file session could not finish before retranslation: {message}"
            )));
        }
        if snapshot.phase == "complete" {
            if snapshot.mode != Some(SessionMode::File)
                || snapshot.processing_mode != CaptionProcessingMode::Translated
            {
                return Err(cancelled_error());
            }
            return Ok(snapshot);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

async fn run_file_retranslation(
    app: tauri::AppHandle,
    client: openai::OpenAiClient,
    lease: RetranslationLease,
) -> Result<(), openai::ApiError> {
    let snapshot = completed_file_snapshot(&app, &lease).await?;
    let source_revision = file_source_revision(&snapshot.segments);
    let inputs = snapshot
        .segments
        .iter()
        .map(|segment| openai::TranslationInput {
            segment_id: segment.id.clone(),
            speaker: segment
                .speaker_id
                .as_ref()
                .and_then(|id| snapshot.speakers.get(id))
                .map(|speaker| speaker.display_name.clone())
                .unwrap_or_else(|| "Speaker".into()),
            source_text: segment.source_text.clone(),
        })
        .collect::<Vec<_>>();
    let mut replacements = Vec::with_capacity(inputs.len());
    for (batch_index, batch) in inputs.chunks(6).enumerate() {
        app.state::<AppState>()
            .retranslations
            .ensure_current(&lease)?;
        app.state::<AppState>()
            .runs
            .lease(lease.session_generation)?;
        let start = batch_index * 6;
        let preceding_start = start.saturating_sub(80);
        let outputs = pipeline::translate_batch_with_retry(
            &client,
            &inputs[preceding_start..start],
            batch,
            &lease.languages,
        )
        .await?;
        app.state::<AppState>()
            .retranslations
            .ensure_current(&lease)?;
        replacements.extend(outputs.into_iter().map(|output| RetranslatedSegment {
            segment_id: output.segment_id,
            translation_text: output.translation,
            ambiguity_note: output.ambiguity_note,
        }));
    }
    record_file_retranslation_success(&app, &lease, &source_revision, replacements)
}

fn scoped_file_retranslation_success<'a>(
    state: &'a AppState,
    lease: &RetranslationLease,
    source_revision: &[FileSourceRevision],
    translations: Vec<RetranslatedSegment>,
) -> Result<ScopedRetranslationEnvelope<'a>, openai::ApiError> {
    let run_guard = state
        .runs
        .active
        .lock()
        .map_err(|_| service_error("Session generation state is unavailable."))?;
    if run_guard.as_ref().is_none_or(|run| {
        run.generation != lease.session_generation || run.is_cancelled()
    }) {
        return Err(cancelled_error());
    }
    let mut request_guard = state
        .retranslations
        .active
        .lock()
        .map_err(|_| service_error("Retranslation generation state is unavailable."))?;
    if lease.is_cancelled()
        || request_guard.as_ref().is_none_or(|request| {
            request.status != RetranslationStatus::Running
                || request.lease.session_generation != lease.session_generation
                || request.lease.request_generation != lease.request_generation
        })
    {
        return Err(cancelled_error());
    }
    let mut snapshot = state
        .canonical
        .lock()
        .map_err(|_| service_error("Session state is unavailable."))?;
    if snapshot.session_id != format!("session-{}", lease.session_generation)
        || snapshot.mode != Some(SessionMode::File)
        || snapshot.processing_mode != CaptionProcessingMode::Translated
        || snapshot.phase != "complete"
        || file_source_revision(&snapshot.segments) != source_revision
    {
        return Err(service_error(
            "The file transcript changed before retranslation could be applied.",
        ));
    }
    let mut replacement_ids = HashMap::new();
    for translation in &translations {
        if translation.translation_text.trim().is_empty()
            || replacement_ids
                .insert(translation.segment_id.as_str(), ())
                .is_some()
        {
            return Err(service_error(
                "The replacement translation set was incomplete or invalid.",
            ));
        }
    }
    if translations.len() != snapshot.segments.len()
        || snapshot
            .segments
            .iter()
            .any(|segment| !replacement_ids.contains_key(segment.id.as_str()))
    {
        return Err(service_error(
            "The replacement translation set did not match the current transcript.",
        ));
    }
    let event = SessionEvent::FileRetranslationApplied {
        languages: lease.languages.clone(),
        translations,
    };
    snapshot.sequence += 1;
    apply_event(&mut snapshot, &event);
    let envelope = SequencedSessionEvent {
        session_id: snapshot.session_id.clone(),
        sequence: snapshot.sequence,
        event,
    };
    *request_guard = None;
    drop(snapshot);
    Ok((run_guard, request_guard, envelope))
}

fn record_file_retranslation_success(
    app: &tauri::AppHandle,
    lease: &RetranslationLease,
    source_revision: &[FileSourceRevision],
    translations: Vec<RetranslatedSegment>,
) -> Result<(), openai::ApiError> {
    let (_run_guard, _request_guard, envelope) = scoped_file_retranslation_success(
        app.state::<AppState>().inner(),
        lease,
        source_revision,
        translations,
    )?;
    app.emit("session-event", envelope)
        .map_err(|error| service_error(&format!("Could not update subtitle windows: {error}")))
}

fn scoped_file_retranslation_failure<'a>(
    state: &'a AppState,
    lease: &RetranslationLease,
    message: String,
) -> Result<ScopedRetranslationEnvelope<'a>, openai::ApiError> {
    let run_guard = state
        .runs
        .active
        .lock()
        .map_err(|_| service_error("Session generation state is unavailable."))?;
    if run_guard.as_ref().is_none_or(|run| {
        run.generation != lease.session_generation || run.is_cancelled()
    }) {
        return Err(cancelled_error());
    }
    let mut request_guard = state
        .retranslations
        .active
        .lock()
        .map_err(|_| service_error("Retranslation generation state is unavailable."))?;
    let request = request_guard.as_mut().ok_or_else(cancelled_error)?;
    if lease.is_cancelled()
        || request.status != RetranslationStatus::Running
        || request.lease.session_generation != lease.session_generation
        || request.lease.request_generation != lease.request_generation
    {
        return Err(cancelled_error());
    }
    request.status = RetranslationStatus::Failed;
    let event = SessionEvent::RecoverableError {
        error: RecoverableError {
            code: "retranslation_failed".into(),
            message: format!(
                "Could not switch subtitle languages. The previous subtitles remain available. {message}"
            ),
            segment_id: None,
        },
    };
    let mut snapshot = state
        .canonical
        .lock()
        .map_err(|_| service_error("Session state is unavailable."))?;
    snapshot.sequence += 1;
    apply_event(&mut snapshot, &event);
    let envelope = SequencedSessionEvent {
        session_id: snapshot.session_id.clone(),
        sequence: snapshot.sequence,
        event,
    };
    drop(snapshot);
    Ok((run_guard, request_guard, envelope))
}

fn record_file_retranslation_failure(
    app: &tauri::AppHandle,
    lease: &RetranslationLease,
    message: String,
) -> Result<(), openai::ApiError> {
    let (_run_guard, _request_guard, envelope) = scoped_file_retranslation_failure(
        app.state::<AppState>().inner(),
        lease,
        message,
    )?;
    app.emit("session-event", envelope)
        .map_err(|error| service_error(&format!("Could not update subtitle windows: {error}")))
}

fn apply_language_settings(
    app: &tauri::AppHandle,
    state: &AppState,
    languages: LanguageSettings,
) -> Result<(), String> {
    let (session_id, previous, mode, processing_mode) = {
        let snapshot = state
            .canonical
            .lock()
            .map_err(|_| "Session state is unavailable.")?;
        (
            snapshot.session_id.clone(),
            snapshot.languages.clone(),
            snapshot.mode.clone(),
            snapshot.processing_mode.clone(),
        )
    };
    let generation = state
        .runs
        .active
        .lock()
        .map_err(|_| "Session generation state is unavailable.")?
        .as_ref()
        .filter(|run| !run.is_cancelled())
        .map(|run| run.generation);
    if let Some(generation) = generation.filter(|generation| {
            processing_mode == CaptionProcessingMode::Translated
            && mode == Some(SessionMode::File)
            && previous.target != languages.target
            && session_id == format!("session-{generation}")
    }) {
        {
            let mut snapshot = state
                .canonical
                .lock()
                .map_err(|_| "Session state is unavailable.")?;
            snapshot.languages.source = languages.source.clone();
            snapshot.languages.explanation = languages.explanation.clone();
        }
        if let Some(lease) = begin_file_retranslation(state, generation, languages)
            .map_err(|error| error.message)?
        {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let result = async {
                    let key = api_key()?;
                    let client = openai::OpenAiClient::new(key)?;
                    run_file_retranslation(app.clone(), client, lease.clone()).await
                }
                .await;
                if let Err(error) = result {
                    if error.kind != openai::ApiErrorKind::Cancelled {
                        let _ = record_file_retranslation_failure(
                            &app,
                            &lease,
                            error.message,
                        );
                    }
                }
            });
        }
    } else if processing_mode == CaptionProcessingMode::Translated
        && mode == Some(SessionMode::Live)
        && previous.target != languages.target
    {
        state
            .retranslations
            .cancel()
            .map_err(|error| error.message)?;
        state
            .canonical
            .lock()
            .map_err(|_| "Session state is unavailable.")?
            .languages = languages;
        #[cfg(target_os = "macos")]
        live::stop(&state.live);
        let _ = record_event(
            app,
            SessionEvent::RecoverableError {
                error: RecoverableError {
                    code: "live_language_changed".into(),
                    message: "Target language changed. Start Live Captions again to apply it."
                        .into(),
                    segment_id: None,
                },
            },
        );
    } else {
        state
            .retranslations
            .cancel()
            .map_err(|error| error.message)?;
        state
            .canonical
            .lock()
            .map_err(|_| "Session state is unavailable.")?
            .languages = languages;
    }
    Ok(())
}

#[tauri::command]
fn update_speaker(app: tauri::AppHandle, speaker: SpeakerProfile) -> Result<(), openai::ApiError> {
    record_event(&app, SessionEvent::SpeakerDiscovered { speaker })
}

#[tauri::command]
fn open_surface(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    surface: String,
) -> Result<(), String> {
    if matches!(surface.as_str(), "viewer" | "overlay") {
        state.subtitles_visible.store(true, Ordering::Relaxed);
        let _ = app.emit("tray-action", "show_subtitles");
    }
    show_surface(&app, &surface)
}

#[tauri::command]
fn get_launcher_mode(state: State<'_, AppState>) -> Result<String, String> {
    state
        .launcher_mode
        .lock()
        .map(|mode| mode.clone())
        .map_err(|_| "Launcher state is unavailable.".into())
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
    state
        .retranslations
        .cancel()
        .map_err(|error| error.message)?;
    state.runs.cancel().map_err(|error| error.message)?;
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
    sync_mode: LiveSyncMode,
    processing_mode: CaptionProcessingMode,
) -> Result<(), openai::ApiError> {
    state.retranslations.cancel()?;
    let run = state.runs.replace()?;
    begin_session_for_generation(
        &app,
        run.generation,
        SessionMode::Live,
        languages.clone(),
        processing_mode.clone(),
    )?;
    let key = api_key()?;
    match live::start(
        app.clone(),
        &state.live,
        key,
        languages,
        sync_mode,
        processing_mode,
        run.generation,
    )
    .await
    {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = record_event_for_generation(
                &app,
                run.generation,
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
            Err(error)
        }
    }
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
async fn start_live_capture(
    _languages: LanguageSettings,
    _sync_mode: LiveSyncMode,
    _processing_mode: CaptionProcessingMode,
) -> Result<(), openai::ApiError> {
    Err(service_error(
        "Live system-audio captions are available on macOS 14 or later.",
    ))
}

#[tauri::command]
fn stop_live_capture(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state
        .retranslations
        .cancel()
        .map_err(|error| error.message)?;
    state.runs.cancel().map_err(|error| error.message)?;
    #[cfg(target_os = "macos")]
    live::stop(&state.live);
    record_event(&app, SessionEvent::Complete).map_err(|error| error.message)?;
    Ok(())
}

fn begin_session_for_generation(
    app: &tauri::AppHandle,
    generation: u64,
    mode: SessionMode,
    languages: LanguageSettings,
    processing_mode: CaptionProcessingMode,
) -> Result<(), openai::ApiError> {
    let state = app.state::<AppState>();
    let event = SessionEvent::SessionReset {
        mode: mode.clone(),
        languages: languages.clone(),
        processing_mode: processing_mode.clone(),
    };
    let active = state
        .runs
        .active
        .lock()
        .map_err(|_| service_error("Session generation state is unavailable."))?;
    if active
        .as_ref()
        .is_none_or(|run| run.generation != generation || run.is_cancelled())
    {
        return Err(cancelled_error());
    }
    state
        .lesson_cache
        .lock()
        .map_err(|_| service_error("Lesson cache is unavailable."))?
        .clear();
    let mut snapshot = state
        .canonical
        .lock()
        .map_err(|_| service_error("Session state is unavailable."))?;
    let media = snapshot.media.clone();
    *snapshot = SessionSnapshot {
        session_id: format!("session-{generation}"),
        mode: Some(mode.clone()),
        languages: languages.clone(),
        media: if mode == SessionMode::File {
            media
        } else {
            None
        },
        ..SessionSnapshot::default()
    };
    snapshot.sequence += 1;
    apply_event(&mut snapshot, &event);
    let envelope = SequencedSessionEvent {
        session_id: snapshot.session_id.clone(),
        sequence: snapshot.sequence,
        event,
    };
    drop(snapshot);
    app.emit("session-event", envelope)
        .map_err(|error| service_error(&format!("Could not update subtitle windows: {error}")))
}

fn scoped_event_envelope<'a>(
    state: &'a AppState,
    generation: u64,
    event: SessionEvent,
) -> Result<(MutexGuard<'a, Option<RunLease>>, SequencedSessionEvent), openai::ApiError> {
    let active = state
        .runs
        .active
        .lock()
        .map_err(|_| service_error("Session generation state is unavailable."))?;
    if active
        .as_ref()
        .is_none_or(|run| run.generation != generation || run.is_cancelled())
    {
        return Err(cancelled_error());
    }
    let mut snapshot = state
        .canonical
        .lock()
        .map_err(|_| service_error("Session state is unavailable."))?;
    snapshot.sequence += 1;
    apply_event(&mut snapshot, &event);
    let envelope = SequencedSessionEvent {
        session_id: snapshot.session_id.clone(),
        sequence: snapshot.sequence,
        event,
    };
    drop(snapshot);
    Ok((active, envelope))
}

pub(crate) fn record_event_for_generation(
    app: &tauri::AppHandle,
    generation: u64,
    event: SessionEvent,
) -> Result<(), openai::ApiError> {
    let (_generation_guard, envelope) =
        scoped_event_envelope(app.state::<AppState>().inner(), generation, event)?;
    app.emit("session-event", envelope)
        .map_err(|error| service_error(&format!("Could not update subtitle windows: {error}")))
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
        SessionEvent::SessionReset {
            mode,
            languages,
            processing_mode,
        } => {
            snapshot.mode = Some(mode.clone());
            snapshot.processing_mode = processing_mode.clone();
            snapshot.languages = languages.clone();
            snapshot.phase = "preparing".into();
            snapshot.live_sync = (mode == &SessionMode::Live).then(LiveSyncState::default);
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
        SessionEvent::FileRetranslationApplied {
            languages,
            translations,
        } => {
            let replacements = translations
                .iter()
                .map(|translation| (translation.segment_id.as_str(), translation))
                .collect::<HashMap<_, _>>();
            snapshot.languages = languages.clone();
            for segment in &mut snapshot.segments {
                if let Some(replacement) = replacements.get(segment.id.as_str()) {
                    segment.translation_text = Some(replacement.translation_text.clone());
                    segment.ambiguity_note = replacement.ambiguity_note.clone();
                    segment.translation_status = contracts::SegmentStatus::Complete;
                }
            }
        }
        SessionEvent::SpeakerDiscovered { speaker } => {
            snapshot
                .speakers
                .insert(speaker.id.clone(), speaker.clone());
        }
        SessionEvent::CoverageChanged { ready_through_ms } => {
            snapshot.ready_through_ms = *ready_through_ms
        }
        SessionEvent::LiveSyncChanged { sync } => snapshot.live_sync = Some(sync.clone()),
        SessionEvent::LessonSelected { segment_id } => {
            snapshot.selected_segment_id = segment_id.clone()
        }
        SessionEvent::RecoverableError { error } => snapshot.errors.push(error.clone()),
        SessionEvent::FatalError { message } => snapshot.fatal_error = Some(message.clone()),
        SessionEvent::Complete => snapshot.phase = "complete".into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct WindowSpec {
    label: &'static str,
    width: f64,
    height: f64,
    min_width: f64,
    min_height: f64,
    decorations: bool,
    transparent: bool,
    shadow: bool,
    always_on_top: bool,
    resizable: bool,
}

fn window_spec(surface: &str) -> Result<WindowSpec, String> {
    match surface {
        "workbench" => Ok(WindowSpec {
            label: "main",
            width: 1320.0,
            height: 840.0,
            min_width: 960.0,
            min_height: 680.0,
            decorations: true,
            transparent: false,
            shadow: true,
            always_on_top: false,
            resizable: true,
        }),
        "viewer" => Ok(WindowSpec {
            label: "viewer",
            width: 1180.0,
            height: 720.0,
            min_width: 720.0,
            min_height: 440.0,
            decorations: false,
            transparent: false,
            shadow: true,
            always_on_top: false,
            resizable: true,
        }),
        "overlay" => Ok(WindowSpec {
            label: "overlay",
            width: 900.0,
            height: 220.0,
            min_width: 520.0,
            min_height: 130.0,
            decorations: false,
            transparent: true,
            shadow: false,
            always_on_top: true,
            resizable: true,
        }),
        "lesson" => Ok(WindowSpec {
            label: "lesson",
            width: 720.0,
            height: 210.0,
            min_width: 600.0,
            min_height: 180.0,
            decorations: false,
            transparent: true,
            shadow: false,
            always_on_top: true,
            resizable: false,
        }),
        "launcher" => Ok(WindowSpec {
            label: "launcher",
            width: 420.0,
            height: 190.0,
            min_width: 420.0,
            min_height: 190.0,
            decorations: false,
            transparent: true,
            shadow: false,
            always_on_top: true,
            resizable: false,
        }),
        _ => Err("Unknown NonoSub surface.".into()),
    }
}

fn show_surface(app: &tauri::AppHandle, surface: &str) -> Result<(), String> {
    let spec = window_spec(surface)?;
    let label = spec.label;
    if let Some(window) = app.get_webview_window(label) {
        window.show().map_err(|error| error.to_string())?;
        let _ = window.set_focus();
        update_activation_policy(app);
        return Ok(());
    }
    let url = WebviewUrl::App(surface_path(surface).into());
    let builder = WebviewWindowBuilder::new(app, label, url)
        .title("NonoSub")
        .inner_size(spec.width, spec.height)
        .min_inner_size(spec.min_width, spec.min_height)
        .decorations(spec.decorations)
        .transparent(spec.transparent)
        .shadow(spec.shadow)
        .always_on_top(spec.always_on_top)
        .resizable(spec.resizable);
    let window = builder.build().map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    update_activation_policy(app);
    Ok(())
}

fn surface_path(surface: &str) -> String {
    format!("?surface={surface}")
}

fn surface_label(surface: &str) -> Result<&'static str, String> {
    window_spec(surface).map(|spec| spec.label)
}

fn place_composer_near_cursor(
    app: &tauri::AppHandle,
    source: &tauri::WebviewWindow,
    cursor_x: f64,
    cursor_y: f64,
) {
    let Ok(Some(monitor)) = source.current_monitor() else {
        return;
    };
    let Some(lesson) = app.get_webview_window("lesson") else {
        return;
    };
    let scale = monitor.scale_factor();
    let logical_monitor_width = monitor.size().width as f64 / scale;
    let logical_monitor_height = monitor.size().height as f64 / scale;
    let logical_width = 720.0_f64.min(logical_monitor_width * 0.9);
    let logical_height = 210.0_f64.min(logical_monitor_height * 0.9);
    let _ = lesson.set_size(tauri::LogicalSize::new(logical_width, logical_height));

    let Ok(source_position) = source.outer_position() else {
        return;
    };
    let cursor_global_x = source_position.x as f64 + cursor_x * scale;
    let cursor_global_y = source_position.y as f64 + cursor_y * scale;
    let width = (logical_width * scale).round() as u32;
    let height = (logical_height * scale).round() as u32;
    let (x, y) = composer_position(
        (
            monitor.position().x,
            monitor.position().y,
            monitor.size().width,
            monitor.size().height,
        ),
        (width, height),
        (cursor_global_x.round() as i32, cursor_global_y.round() as i32),
        18,
    );
    let _ = lesson.set_position(tauri::PhysicalPosition::new(x, y));
}

fn composer_position(
    monitor: (i32, i32, u32, u32),
    window: (u32, u32),
    cursor: (i32, i32),
    margin: i32,
) -> (i32, i32) {
    let (monitor_x, monitor_y, monitor_width, monitor_height) = monitor;
    let (window_width, window_height) = window;
    let (cursor_x, cursor_y) = cursor;
    let min_x = monitor_x + margin;
    let max_x = monitor_x + monitor_width.saturating_sub(window_width) as i32 - margin;
    let min_y = monitor_y + margin;
    let max_y = monitor_y + monitor_height.saturating_sub(window_height) as i32 - margin;
    let centered_x = cursor_x - window_width as i32 / 2;
    let above_y = cursor_y - window_height as i32 - margin;
    let preferred_y = if above_y >= min_y {
        above_y
    } else {
        cursor_y + margin
    };
    (
        centered_x.clamp(min_x, max_x.max(min_x)),
        preferred_y.clamp(min_y, max_y.max(min_y)),
    )
}

fn close_lesson(app: &tauri::AppHandle, state: &AppState) {
    if let Some(window) = app.get_webview_window("lesson") {
        let _ = window.hide();
    }
    if state
        .external_media_paused_for_lesson
        .swap(false, Ordering::Relaxed)
    {
        let _ = media_keys::post_play_pause();
    }
    if let Ok(mut context) = state.lesson_open_context.lock() {
        *context = None;
    }
    let _ = app.emit("lesson-closed", ());
    update_activation_policy(app);
}

fn update_activation_policy(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let visible = ["main", "viewer"]
            .into_iter()
            .filter(|label| {
                app.get_webview_window(label)
                    .and_then(|window| window.is_visible().ok())
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        let regular = requires_regular_activation(visible.iter().copied());
        let _ = app.set_activation_policy(if regular {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        });
    }
}

fn requires_regular_activation<'a>(labels: impl IntoIterator<Item = &'a str>) -> bool {
    labels
        .into_iter()
        .any(|label| matches!(label, "main" | "viewer"))
}

fn subtitle_preset_menu<R: tauri::Runtime>(
    app: &impl Manager<R>,
) -> tauri::Result<tauri::menu::Submenu<R>> {
    SubmenuBuilder::new(app, "Subtitle preset")
        .text("preset_clean", "Clean")
        .text("preset_classic-outline", "Classic Outline")
        .text("preset_yellow-drop", "Yellow Drop")
        .text("preset_fallout", "Fallout")
        .text("preset_momento", "Momento")
        .text("preset_wired", "Wired")
        .build()
}

fn subtitle_timing_menu<R: tauri::Runtime>(
    app: &impl Manager<R>,
) -> tauri::Result<tauri::menu::Submenu<R>> {
    SubmenuBuilder::new(app, "Subtitle timing")
        .text("subtitle_earlier", "100 ms Earlier")
        .text("subtitle_later", "100 ms Later")
        .text("subtitle_reset", "Reset")
        .build()
}

fn subtitle_display_menu<R: tauri::Runtime>(
    app: &impl Manager<R>,
) -> tauri::Result<tauri::menu::Submenu<R>> {
    SubmenuBuilder::new(app, "Subtitle display")
        .text("display_source", "Original only")
        .text("display_translation", "Translation only")
        .text("display_both", "Original + translation")
        .build()
}

fn live_timing_menu<R: tauri::Runtime>(
    app: &impl Manager<R>,
) -> tauri::Result<tauri::menu::Submenu<R>> {
    SubmenuBuilder::new(app, "Live timing")
        .text("live_mode_coordinated", "Coordinated")
        .text("live_mode_fast_source", "Fast Source")
        .build()
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let levels = SubmenuBuilder::new(app, "Learner level")
        .text("level_beginner", "Beginner")
        .text("level_intermediate", "Intermediate")
        .text("level_advanced", "Advanced")
        .build()?;
    let presets = subtitle_preset_menu(app)?;
    let timing = subtitle_timing_menu(app)?;
    let display = subtitle_display_menu(app)?;
    let live_timing = live_timing_menu(app)?;
    let experimental = SubmenuBuilder::new(app, "Experimental")
        .text("external_pause_on", "External Media Pause: On")
        .text("external_pause_off", "External Media Pause: Off")
        .build()?;
    let menu = MenuBuilder::new(app)
        .text("open_video", "Open Video…")
        .text("start_live", "Start Live Captions…")
        .text("stop_session", "Stop Current Session")
        .separator()
        .text("toggle_subtitles", "Show / Hide Subtitles")
        .text("show_lesson", "Show Nono Lesson")
        .text("hide_lesson", "Hide Nono Lesson")
        .text("arrange_overlay", "Arrange Subtitle Overlay")
        .text("play_pause", "Play / Pause")
        .item(&timing)
        .item(&display)
        .item(&live_timing)
        .item(&experimental)
        .item(&presets)
        .item(&levels)
        .text("languages", "Languages…")
        .separator()
        .text("show_workbench", "Settings & Transcript")
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
    tray.build(app)?;
    Ok(())
}

fn show_launcher(app: &tauri::AppHandle, mode: &str) {
    let state = app.state::<AppState>();
    if let Ok(mut current) = state.launcher_mode.lock() {
        *current = mode.into();
    }
    let _ = show_surface(app, "launcher");
    let _ = app.emit("launcher-action", mode);
}

fn dispatch_action(app: &tauri::AppHandle, id: &str) {
    match id {
        "open_video" => show_launcher(app, "file"),
        "start_live" => show_launcher(app, "live"),
        "stop_session" => {
            let state = app.state::<AppState>();
            let mode = state
                .canonical
                .lock()
                .ok()
                .and_then(|snapshot| snapshot.mode.clone());
            let _ = state.retranslations.cancel();
            let _ = state.runs.cancel();
            #[cfg(target_os = "macos")]
            live::stop(&state.live);
            let _ = record_event(app, SessionEvent::Complete);
            if mode == Some(SessionMode::Live) {
                if let Some(window) = app.get_webview_window("overlay") {
                    let _ = window.hide();
                }
            }
            update_activation_policy(app);
        }
        "toggle_subtitles" => {
            let state = app.state::<AppState>();
            let visible = !state.subtitles_visible.fetch_xor(true, Ordering::Relaxed);
            let mode = state
                .canonical
                .lock()
                .ok()
                .and_then(|snapshot| snapshot.mode.clone());
            if mode == Some(SessionMode::Live) {
                if let Some(window) = app.get_webview_window("overlay") {
                    if visible {
                        let _ = window.show();
                    } else {
                        let _ = window.hide();
                    }
                } else if visible {
                    let _ = show_surface(app, "overlay");
                }
            }
            let _ = app.emit("tray-action", id);
            update_activation_policy(app);
        }
        "arrange_overlay" => {
            app.state::<AppState>()
                .subtitles_visible
                .store(true, Ordering::Relaxed);
            let _ = show_surface(app, "overlay");
            let _ = app.emit("tray-action", "show_subtitles");
            let _ = app.emit("tray-action", id);
        }
        "show_lesson" => {
            let state = app.state::<AppState>();
            let context_segment = state
                .context_lesson_segment
                .lock()
                .ok()
                .and_then(|mut segment| segment.take());
            if let Some(segment_id) = context_segment {
                let _ = record_event(
                    app,
                    SessionEvent::LessonSelected {
                        segment_id: Some(segment_id),
                    },
                );
            }
            let has_selection = state
                .canonical
                .lock()
                .is_ok_and(|snapshot| snapshot.selected_segment_id.is_some());
            if has_selection {
                let _ = show_surface(app, "lesson");
                let source_label = state
                    .context_lesson_source
                    .lock()
                    .ok()
                    .and_then(|mut source| source.take());
                let source = source_label
                    .and_then(|label| app.get_webview_window(&label))
                    .or_else(|| visible_lesson_source(app));
                if let Some(source) = source {
                    let source_surface = if source.label() == "viewer" { "viewer" } else if source.label() == "overlay" { "overlay" } else { "workbench" };
                    let scale = source.scale_factor().unwrap_or(1.0);
                    let size = source.inner_size().unwrap_or(tauri::PhysicalSize::new(720, 420));
                    let cursor_x = size.width as f64 / scale / 2.0;
                    let cursor_y = size.height as f64 / scale / 2.0;
                    place_composer_near_cursor(app, &source, cursor_x, cursor_y);
                    let segment_id = state.canonical.lock().ok().and_then(|snapshot| snapshot.selected_segment_id.clone()).unwrap_or_default();
                    let payload = LessonOpenContext {
                        source_surface: source_surface.into(),
                        segment_id,
                        cursor_x,
                        cursor_y,
                        external_media_control: ExternalMediaControlResult::NotRequested,
                    };
                    if let Ok(mut context) = state.lesson_open_context.lock() {
                        *context = Some(payload.clone());
                    }
                    let _ = app.emit("lesson-composer-opened", payload);
                }
            }
        }
        "hide_lesson" => {
            let state = app.state::<AppState>();
            close_lesson(app, state.inner());
        }
        "languages" | "show_workbench" => {
            let _ = show_surface(app, "workbench");
            let _ = app.emit("tray-action", id);
        }
        "quit" => app.exit(0),
        "play_pause"
        | "subtitle_earlier"
        | "subtitle_later"
        | "subtitle_reset"
        | "toggle_speaker_names"
        | "display_source"
        | "display_translation"
        | "display_both"
        | "live_mode_coordinated"
        | "live_mode_fast_source"
        | "external_pause_on"
        | "external_pause_off" => {
            let _ = app.emit("tray-action", id);
        }
        value if value.starts_with("preset_") || value.starts_with("level_") => {
            let _ = app.emit("tray-action", value);
        }
        _ => {}
    }
}

fn visible_lesson_source(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
    ["viewer", "overlay"].into_iter().find_map(|label| {
        app.get_webview_window(label)
            .filter(|window| window.is_visible().unwrap_or(false))
    })
}

fn service_error(message: &str) -> openai::ApiError {
    openai::ApiError {
        kind: openai::ApiErrorKind::Service,
        message: message.into(),
        retryable: false,
    }
}

fn cancelled_error() -> openai::ApiError {
    openai::ApiError {
        kind: openai::ApiErrorKind::Cancelled,
        message: "This session run was replaced or cancelled.".into(),
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
            if has_key {
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
        .on_menu_event(|app, event| dispatch_action(app, event.id().as_ref()))
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                if window.label() == "lesson" {
                    let state = window.app_handle().state::<AppState>();
                    close_lesson(window.app_handle(), state.inner());
                }
                update_activation_policy(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            api_key_status,
            save_api_key,
            validate_model_access,
            remove_api_key,
            get_session_snapshot,
            initialize_preferences,
            patch_preferences,
            prepare_media,
            prepare_audio,
            start_analysis,
            retry_translation,
            request_lesson,
            open_lesson_composer,
            get_lesson_open_context,
            media_key_permission_status,
            request_media_key_permission,
            post_media_play_pause,
            close_lesson_surface,
            update_speaker,
            open_surface,
            get_launcher_mode,
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

    fn file_segment(id: &str, source: &str, translation: &str, start_ms: u64) -> SubtitleSegment {
        SubtitleSegment {
            id: id.into(),
            origin: SessionMode::File,
            start_ms,
            end_ms: start_ms + 1_000,
            source_text: source.into(),
            translation_text: Some(translation.into()),
            ambiguity_note: None,
            speaker_id: Some("speaker-1".into()),
            is_provisional: false,
            transcription_status: SegmentStatus::Complete,
            translation_status: SegmentStatus::Complete,
        }
    }

    fn completed_file_state() -> AppState {
        let state = AppState::default();
        let run = state.runs.replace().unwrap();
        *state.canonical.lock().unwrap() = SessionSnapshot {
            session_id: format!("session-{}", run.generation),
            mode: Some(SessionMode::File),
            processing_mode: CaptionProcessingMode::Translated,
            languages: LanguageSettings {
                source: "ja".into(),
                target: "en".into(),
                explanation: "en".into(),
            },
            phase: "complete".into(),
            segments: vec![
                file_segment("segment-1", "何ですか？", "What is it?", 0),
                file_segment("segment-2", "今日はちょっと", "Today is a little…", 1_000),
            ],
            ..SessionSnapshot::default()
        };
        state
    }

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
    fn preference_broker_rebases_stale_leaf_patches_without_losing_unrelated_changes() {
        let mut slot = None;
        let initial = serde_json::json!({
            "style": { "preset": "momento", "position": { "x": 0.5, "y": 0.8 } },
            "languages": { "source": "auto", "target": "en", "explanation": "en" },
            "lessonPlacements": {}
        });
        let initialized = initialize_preference_state(&mut slot, initial).unwrap();
        assert_eq!(initialized.revision, 0);

        let current = slot.as_mut().unwrap();
        let style = apply_preference_patch(
            current,
            0,
            serde_json::json!({ "style": { "preset": "wired" } }),
        )
        .unwrap();
        assert_eq!(style.revision, 1);
        assert!(!style.rebased);

        let language = apply_preference_patch(
            current,
            0,
            serde_json::json!({ "languages": { "target": "ja", "explanation": "ja" } }),
        )
        .unwrap();
        assert_eq!(language.revision, 2);
        assert!(language.rebased);
        assert_eq!(language.preferences["style"]["preset"], "wired");
        assert_eq!(language.preferences["languages"]["source"], "auto");
        assert_eq!(language.preferences["languages"]["target"], "ja");
    }

    #[test]
    fn preference_broker_merges_independent_monitor_placements_and_orders_conflicts() {
        let mut state = CanonicalPreferences {
            revision: 4,
            preferences: serde_json::json!({
                "style": { "preset": "clean" },
                "lessonPlacements": {}
            }),
        };
        apply_preference_patch(
            &mut state,
            4,
            serde_json::json!({ "lessonPlacements": { "display-a": { "x": 0.1, "y": 0.2 } } }),
        )
        .unwrap();
        apply_preference_patch(
            &mut state,
            4,
            serde_json::json!({ "lessonPlacements": { "display-b": { "x": 0.7, "y": 0.3 } } }),
        )
        .unwrap();
        apply_preference_patch(
            &mut state,
            5,
            serde_json::json!({ "style": { "preset": "fallout" } }),
        )
        .unwrap();
        let last = apply_preference_patch(
            &mut state,
            5,
            serde_json::json!({ "style": { "preset": "wired" } }),
        )
        .unwrap();

        assert_eq!(last.preferences["lessonPlacements"].as_object().unwrap().len(), 2);
        assert_eq!(last.preferences["style"]["preset"], "wired");
        assert_eq!(last.revision, 8);
        assert!(last.rebased);
    }

    #[test]
    fn preference_broker_keeps_the_first_valid_seed_and_rejects_non_object_patches() {
        let mut slot = None;
        initialize_preference_state(&mut slot, serde_json::json!({ "level": "beginner" }))
            .unwrap();
        let second = initialize_preference_state(
            &mut slot,
            serde_json::json!({ "level": "advanced" }),
        )
        .unwrap();
        assert_eq!(second.preferences["level"], "beginner");
        assert!(apply_preference_patch(slot.as_mut().unwrap(), 0, serde_json::json!("bad"))
            .is_err());
    }

    #[test]
    fn secondary_surfaces_use_the_root_app_route() {
        assert_eq!(surface_path("overlay"), "?surface=overlay");
        assert_eq!(surface_path("launcher"), "?surface=launcher");
        assert!(!surface_path("lesson").contains("index.html"));
    }

    #[test]
    fn compact_surface_specs_are_transparent_and_accessory_safe() {
        for surface in ["overlay", "lesson", "launcher"] {
            let spec = window_spec(surface).unwrap();
            assert!(spec.transparent);
            assert!(!spec.shadow);
            assert!(!requires_regular_activation([spec.label]));
        }
        assert_eq!(window_spec("launcher").unwrap().width, 420.0);
        assert_eq!(window_spec("lesson").unwrap().height, 210.0);
    }

    #[test]
    fn only_settings_and_viewer_require_a_dock_icon() {
        assert!(requires_regular_activation(["main"]));
        assert!(requires_regular_activation(["overlay", "viewer"]));
        assert!(!requires_regular_activation([
            "overlay", "lesson", "launcher"
        ]));
    }

    #[test]
    fn composer_placement_prefers_above_and_clamps_to_monitor() {
        assert_eq!(
            composer_position((100, 50, 1600, 900), (720, 210), (900, 500), 18),
            (540, 272)
        );
        assert_eq!(
            composer_position((100, 50, 800, 500), (720, 210), (110, 55), 18),
            (118, 73)
        );
    }

    #[test]
    fn learner_levels_are_stable_snake_case_values() {
        assert_eq!(
            serde_json::to_string(&LearnerLevel::Advanced).unwrap(),
            "\"advanced\""
        );
    }

    #[test]
    fn replacing_a_run_permanently_cancels_only_the_previous_token() {
        let runs = RunCoordinator::default();
        let first = runs.replace().unwrap();
        assert!(!first.is_cancelled());

        let second = runs.replace().unwrap();
        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());
        assert!(runs.lease(first.generation).is_err());
        assert_eq!(runs.lease(second.generation).unwrap().generation, second.generation);

        runs.cancel().unwrap();
        assert!(first.is_cancelled());
        assert!(second.is_cancelled());
        assert!(runs.lease(second.generation).is_err());
    }

    #[test]
    fn retranslation_requests_suppress_duplicates_and_permanently_cancel_superseded_work() {
        let coordinator = RetranslationCoordinator::default();
        let spanish = LanguageSettings {
            source: "ja".into(),
            target: "es".into(),
            explanation: "es".into(),
        };
        let first = coordinator.begin(7, spanish.clone()).unwrap().unwrap();
        assert!(coordinator.begin(7, spanish.clone()).unwrap().is_none());
        coordinator.active.lock().unwrap().as_mut().unwrap().status =
            RetranslationStatus::Failed;
        assert!(coordinator.begin(7, spanish).unwrap().is_none());

        let french = LanguageSettings {
            source: "ja".into(),
            target: "fr".into(),
            explanation: "fr".into(),
        };
        let second = coordinator.begin(7, french).unwrap().unwrap();
        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());
        assert!(coordinator.ensure_current(&first).is_err());
        assert!(coordinator.ensure_current(&second).is_ok());
    }

    #[test]
    fn complete_file_retranslation_changes_every_line_and_language_atomically() {
        let state = completed_file_state();
        let run_generation = state.runs.active.lock().unwrap().as_ref().unwrap().generation;
        let requested = LanguageSettings {
            source: "ja".into(),
            target: "es".into(),
            explanation: "es".into(),
        };
        let lease = state
            .retranslations
            .begin(run_generation, requested.clone())
            .unwrap()
            .unwrap();
        let revision = file_source_revision(&state.canonical.lock().unwrap().segments);
        let replacements = vec![
            RetranslatedSegment {
                segment_id: "segment-1".into(),
                translation_text: "¿Qué es?".into(),
                ambiguity_note: None,
            },
            RetranslatedSegment {
                segment_id: "segment-2".into(),
                translation_text: "Hoy me viene un poco mal…".into(),
                ambiguity_note: Some("Indirect refusal".into()),
            },
        ];

        let (run_guard, request_guard, envelope) = scoped_file_retranslation_success(
            &state,
            &lease,
            &revision,
            replacements,
        )
        .unwrap();
        assert!(matches!(
            envelope.event,
            SessionEvent::FileRetranslationApplied { .. }
        ));
        drop(request_guard);
        drop(run_guard);
        let snapshot = state.canonical.lock().unwrap();
        assert_eq!(snapshot.languages, requested);
        assert_eq!(
            snapshot
                .segments
                .iter()
                .map(|segment| segment.translation_text.as_deref().unwrap())
                .collect::<Vec<_>>(),
            vec!["¿Qué es?", "Hoy me viene un poco mal…"]
        );
        assert_eq!(snapshot.sequence, 1);
        assert!(state.retranslations.active.lock().unwrap().is_none());
    }

    #[test]
    fn stale_retranslation_generation_cannot_change_the_visible_target() {
        let state = completed_file_state();
        let generation = state.runs.active.lock().unwrap().as_ref().unwrap().generation;
        let first = state
            .retranslations
            .begin(
                generation,
                LanguageSettings {
                    source: "ja".into(),
                    target: "es".into(),
                    explanation: "es".into(),
                },
            )
            .unwrap()
            .unwrap();
        let revision = file_source_revision(&state.canonical.lock().unwrap().segments);
        let _second = state
            .retranslations
            .begin(
                generation,
                LanguageSettings {
                    source: "ja".into(),
                    target: "fr".into(),
                    explanation: "fr".into(),
                },
            )
            .unwrap()
            .unwrap();
        let before = state.canonical.lock().unwrap().clone();

        let error = scoped_file_retranslation_success(
            &state,
            &first,
            &revision,
            vec![
                RetranslatedSegment {
                    segment_id: "segment-1".into(),
                    translation_text: "stale one".into(),
                    ambiguity_note: None,
                },
                RetranslatedSegment {
                    segment_id: "segment-2".into(),
                    translation_text: "stale two".into(),
                    ambiguity_note: None,
                },
            ],
        )
        .unwrap_err();
        assert_eq!(error.kind, openai::ApiErrorKind::Cancelled);
        let after = state.canonical.lock().unwrap();
        assert_eq!(after.sequence, before.sequence);
        assert_eq!(after.languages, before.languages);
        assert_eq!(after.segments, before.segments);
    }

    #[test]
    fn incomplete_atomic_replacement_is_rejected_without_partial_mutation() {
        let state = completed_file_state();
        let generation = state.runs.active.lock().unwrap().as_ref().unwrap().generation;
        let lease = state
            .retranslations
            .begin(
                generation,
                LanguageSettings {
                    source: "ja".into(),
                    target: "es".into(),
                    explanation: "es".into(),
                },
            )
            .unwrap()
            .unwrap();
        let revision = file_source_revision(&state.canonical.lock().unwrap().segments);
        let before = state.canonical.lock().unwrap().clone();

        let error = scoped_file_retranslation_success(
            &state,
            &lease,
            &revision,
            vec![RetranslatedSegment {
                segment_id: "segment-1".into(),
                translation_text: "¿Qué es?".into(),
                ambiguity_note: None,
            }],
        )
        .unwrap_err();
        assert_eq!(error.kind, openai::ApiErrorKind::Service);
        let after = state.canonical.lock().unwrap();
        assert_eq!(after.sequence, before.sequence);
        assert_eq!(after.languages, before.languages);
        assert_eq!(after.segments, before.segments);
    }

    #[test]
    fn failed_retranslation_preserves_the_previous_language_and_complete_subtitles() {
        let state = completed_file_state();
        let generation = state.runs.active.lock().unwrap().as_ref().unwrap().generation;
        let requested = LanguageSettings {
            source: "ja".into(),
            target: "es".into(),
            explanation: "es".into(),
        };
        let lease = state
            .retranslations
            .begin(generation, requested.clone())
            .unwrap()
            .unwrap();
        let before = state.canonical.lock().unwrap().clone();

        let (run_guard, request_guard, envelope) = scoped_file_retranslation_failure(
            &state,
            &lease,
            "network unavailable".into(),
        )
        .unwrap();
        assert!(matches!(
            envelope.event,
            SessionEvent::RecoverableError { .. }
        ));
        drop(request_guard);
        drop(run_guard);
        let after = state.canonical.lock().unwrap();
        assert_eq!(after.languages, before.languages);
        assert_eq!(after.segments, before.segments);
        assert_eq!(after.errors.len(), before.errors.len() + 1);
        drop(after);
        assert!(state
            .retranslations
            .begin(generation, requested)
            .unwrap()
            .is_none());
    }

    #[test]
    fn stale_generation_events_cannot_mutate_or_advance_the_current_snapshot() {
        let state = AppState::default();
        let first = state.runs.replace().unwrap();
        state.canonical.lock().unwrap().session_id = format!("session-{}", first.generation);
        let (first_guard, _) = scoped_event_envelope(
            &state,
            first.generation,
            SessionEvent::PhaseChanged {
                phase: "transcribing".into(),
            },
        )
        .unwrap();
        drop(first_guard);

        let second = state.runs.replace().unwrap();
        *state.canonical.lock().unwrap() = SessionSnapshot {
            session_id: format!("session-{}", second.generation),
            phase: "preparing".into(),
            ..SessionSnapshot::default()
        };
        let before = state.canonical.lock().unwrap().clone();

        let stale = scoped_event_envelope(
            &state,
            first.generation,
            SessionEvent::FatalError {
                message: "late failure from the old run".into(),
            },
        )
        .unwrap_err();
        assert_eq!(stale.kind, openai::ApiErrorKind::Cancelled);
        let after_stale = state.canonical.lock().unwrap().clone();
        assert_eq!(after_stale.session_id, before.session_id);
        assert_eq!(after_stale.sequence, before.sequence);
        assert_eq!(after_stale.phase, before.phase);
        assert_eq!(after_stale.fatal_error, before.fatal_error);

        let (generation_guard, accepted) = scoped_event_envelope(
            &state,
            second.generation,
            SessionEvent::PhaseChanged {
                phase: "transcribing".into(),
            },
        )
        .unwrap();
        assert!(state.runs.active.try_lock().is_err());
        drop(generation_guard);
        assert_eq!(accepted.session_id, format!("session-{}", second.generation));
        assert_eq!(accepted.sequence, 1);
        assert_eq!(state.canonical.lock().unwrap().phase, "transcribing");
    }
}
