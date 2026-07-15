use crate::{
    contracts::{
        LanguageSettings, LiveSyncMode, LiveSyncState, LiveSyncStatus, RecoverableError,
        SegmentStatus, SessionEvent, SessionMode, SubtitleSegment,
    },
    openai::{ApiError, ApiErrorKind},
    record_event,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use screencapturekit::{
    async_api::{AsyncSCContentSharingPicker, AsyncSCStream},
    cm::CMSampleBufferExt,
    content_sharing_picker::{
        SCContentSharingPickerConfiguration, SCContentSharingPickerMode, SCPickerOutcome,
    },
    stream::{configuration::SCStreamConfiguration, output_type::SCStreamOutputType},
};
use serde_json::{json, Value};
use std::{
    collections::{HashSet, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream, WebSocketStream,
};

const REALTIME_URL: &str =
    "wss://api.openai.com/v1/realtime/translations?model=gpt-realtime-translate";
const SEND_SAMPLES: usize = 2_400;
type RealtimeSocket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
type RealtimeWriter = SplitSink<RealtimeSocket, Message>;
type RealtimeReader = SplitStream<RealtimeSocket>;

#[derive(Debug, Default)]
pub struct LiveState {
    cancelled: Arc<AtomicBool>,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TimedText {
    event_id: Option<String>,
    text: String,
    elapsed_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RealtimeEvent {
    SourceDelta(TimedText),
    TranslationDelta(TimedText),
    SourceDone(TimedText),
    TranslationDone(TimedText),
    Closed,
    Error(String),
    Ignored,
}

#[derive(Debug)]
struct CaptionDraft {
    id: String,
    start_ms: u64,
    end_ms: u64,
    source: String,
    translation: String,
    last_source_at_ms: u64,
    last_translation_at_ms: u64,
    source_closed_at_ms: Option<u64>,
    source_closed: bool,
    translation_closed: bool,
}

impl CaptionDraft {
    fn new(id: String, aligned_ms: u64, capture_clock_ms: u64) -> Self {
        Self {
            id,
            start_ms: aligned_ms,
            end_ms: aligned_ms.saturating_add(200),
            source: String::new(),
            translation: String::new(),
            last_source_at_ms: capture_clock_ms,
            last_translation_at_ms: capture_clock_ms,
            source_closed_at_ms: None,
            source_closed: false,
            translation_closed: false,
        }
    }

    fn segment(&self) -> SubtitleSegment {
        SubtitleSegment {
            id: self.id.clone(),
            origin: SessionMode::Live,
            start_ms: self.start_ms,
            end_ms: self.end_ms.max(self.start_ms.saturating_add(250)),
            source_text: self.source.trim().to_owned(),
            translation_text: (!self.translation.trim().is_empty())
                .then(|| self.translation.trim().to_owned()),
            ambiguity_note: None,
            speaker_id: Some("live-audio".into()),
            is_provisional: !self.source_closed,
            transcription_status: if self.source_closed {
                SegmentStatus::Complete
            } else {
                SegmentStatus::Pending
            },
            translation_status: if self.translation_closed && !self.translation.trim().is_empty() {
                SegmentStatus::Complete
            } else {
                SegmentStatus::Pending
            },
        }
    }
}

#[derive(Debug)]
struct LagEstimator {
    samples: VecDeque<(u64, u64)>,
    target_delay_ms: u64,
    observed_lag_ms: u64,
    last_decrease_at_ms: u64,
    status: LiveSyncStatus,
}

impl Default for LagEstimator {
    fn default() -> Self {
        Self {
            samples: VecDeque::new(),
            target_delay_ms: 2_500,
            observed_lag_ms: 0,
            last_decrease_at_ms: 0,
            status: LiveSyncStatus::Steady,
        }
    }
}

impl LagEstimator {
    fn observe(&mut self, capture_clock_ms: u64, aligned_ms: u64) {
        let lag = capture_clock_ms.saturating_sub(aligned_ms);
        self.samples.push_back((capture_clock_ms, lag));
        while self
            .samples
            .front()
            .is_some_and(|(at, _)| capture_clock_ms.saturating_sub(*at) > 30_000)
        {
            self.samples.pop_front();
        }
        let mut values: Vec<u64> = self.samples.iter().map(|(_, value)| *value).collect();
        values.sort_unstable();
        let index = (values.len() * 90).div_ceil(100).saturating_sub(1);
        self.observed_lag_ms = values.get(index).copied().unwrap_or_default();
        let desired_unclamped = self.observed_lag_ms.saturating_add(600);
        let desired = desired_unclamped.clamp(1_500, 6_000);
        if desired > self.target_delay_ms {
            self.target_delay_ms = desired;
            self.last_decrease_at_ms = capture_clock_ms;
        } else if self.target_delay_ms > desired
            && capture_clock_ms.saturating_sub(self.last_decrease_at_ms) >= 10_000
        {
            self.target_delay_ms = self.target_delay_ms.saturating_sub(200).max(desired);
            self.last_decrease_at_ms = capture_clock_ms;
        }
        self.status = if desired_unclamped > 6_000 {
            LiveSyncStatus::Degraded
        } else if lag.saturating_add(300) >= self.target_delay_ms {
            LiveSyncStatus::CatchingUp
        } else {
            LiveSyncStatus::Steady
        };
    }
}

#[derive(Debug)]
struct CaptionCoordinator {
    mode: LiveSyncMode,
    epoch_offset_ms: u64,
    segment_index: u64,
    drafts: VecDeque<CaptionDraft>,
    seen_event_ids: HashSet<String>,
    lag: LagEstimator,
    display_cursor_ms: u64,
    visible_segment_id: Option<String>,
    last_emitted_sync: Option<LiveSyncState>,
}

impl CaptionCoordinator {
    fn new(mode: LiveSyncMode) -> Self {
        Self {
            mode,
            epoch_offset_ms: 0,
            segment_index: 1,
            drafts: VecDeque::new(),
            seen_event_ids: HashSet::new(),
            lag: LagEstimator::default(),
            display_cursor_ms: 0,
            visible_segment_id: None,
            last_emitted_sync: None,
        }
    }

    fn next_draft(&mut self, aligned_ms: u64, capture_clock_ms: u64) -> usize {
        let id = format!("live-{}", self.segment_index);
        self.segment_index += 1;
        self.drafts
            .push_back(CaptionDraft::new(id, aligned_ms, capture_clock_ms));
        self.drafts.len() - 1
    }

    fn accept(&mut self, event_id: &Option<String>) -> bool {
        match event_id {
            Some(id) if !id.is_empty() => self.seen_event_ids.insert(id.clone()),
            _ => true,
        }
    }

    fn aligned_ms(&self, elapsed_ms: Option<u64>, capture_clock_ms: u64) -> u64 {
        elapsed_ms.map_or(capture_clock_ms, |elapsed| {
            self.epoch_offset_ms.saturating_add(elapsed)
        })
    }

    fn on_source(&mut self, delta: TimedText, capture_clock_ms: u64) -> Vec<SessionEvent> {
        if !self.accept(&delta.event_id) || delta.text.is_empty() {
            return Vec::new();
        }
        let aligned_ms = self.aligned_ms(delta.elapsed_ms, capture_clock_ms);
        let index = self
            .drafts
            .iter()
            .rposition(|draft| !draft.source_closed)
            .unwrap_or_else(|| self.next_draft(aligned_ms, capture_clock_ms));
        let draft = &mut self.drafts[index];
        draft.start_ms = draft.start_ms.min(aligned_ms);
        draft.end_ms = draft.end_ms.max(aligned_ms.saturating_add(200));
        draft.source.push_str(&delta.text);
        draft.last_source_at_ms = capture_clock_ms;
        let mut events = vec![SessionEvent::CaptionUpserted {
            segment: draft.segment(),
        }];
        events.extend(self.advance(capture_clock_ms));
        events
    }

    fn target_index(&mut self, aligned_ms: u64, capture_clock_ms: u64) -> usize {
        if self.drafts.is_empty() {
            return self.next_draft(aligned_ms, capture_clock_ms);
        }
        self.drafts
            .iter()
            .enumerate()
            .filter(|(_, draft)| !draft.source.trim().is_empty())
            .min_by_key(|(_, draft)| {
                if aligned_ms < draft.start_ms {
                    draft.start_ms - aligned_ms
                } else {
                    aligned_ms.saturating_sub(draft.end_ms)
                }
            })
            .map(|(index, _)| index)
            .unwrap_or_else(|| self.drafts.len() - 1)
    }

    fn on_translation(&mut self, delta: TimedText, capture_clock_ms: u64) -> Vec<SessionEvent> {
        if !self.accept(&delta.event_id) || delta.text.is_empty() {
            return Vec::new();
        }
        let aligned_ms = self.aligned_ms(delta.elapsed_ms, capture_clock_ms);
        if delta.elapsed_ms.is_some() {
            self.lag.observe(capture_clock_ms, aligned_ms);
        }
        let index = self.target_index(aligned_ms, capture_clock_ms);
        let draft = &mut self.drafts[index];
        draft.start_ms = draft.start_ms.min(aligned_ms);
        draft.end_ms = draft.end_ms.max(aligned_ms.saturating_add(200));
        draft.translation.push_str(&delta.text);
        draft.translation_closed = false;
        draft.last_translation_at_ms = capture_clock_ms;
        let mut events = vec![SessionEvent::CaptionUpserted {
            segment: draft.segment(),
        }];
        events.extend(self.advance(capture_clock_ms));
        events
    }

    fn on_done(
        &mut self,
        source: bool,
        done: TimedText,
        capture_clock_ms: u64,
    ) -> Vec<SessionEvent> {
        if !self.accept(&done.event_id) {
            return Vec::new();
        }
        let aligned_ms = self.aligned_ms(done.elapsed_ms, capture_clock_ms);
        let index = if source {
            self.drafts
                .iter()
                .rposition(|draft| !draft.source_closed)
                .unwrap_or_else(|| self.next_draft(aligned_ms, capture_clock_ms))
        } else {
            self.target_index(aligned_ms, capture_clock_ms)
        };
        let draft = &mut self.drafts[index];
        if !done.text.trim().is_empty() {
            if source {
                draft.source = done.text;
            } else {
                draft.translation = done.text;
            }
        }
        draft.end_ms = draft.end_ms.max(aligned_ms.saturating_add(200));
        if source {
            draft.source_closed = true;
            draft.source_closed_at_ms = Some(capture_clock_ms);
        } else {
            draft.translation_closed = true;
        }
        let mut events = vec![if source {
            SessionEvent::TranscriptFinalized {
                segment: draft.segment(),
            }
        } else {
            SessionEvent::CaptionUpserted {
                segment: draft.segment(),
            }
        }];
        events.extend(self.advance(capture_clock_ms));
        events
    }

    fn tick(&mut self, capture_clock_ms: u64) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        for draft in &mut self.drafts {
            let span = draft.end_ms.saturating_sub(draft.start_ms);
            let source_quiet = capture_clock_ms.saturating_sub(draft.last_source_at_ms);
            if !draft.source_closed
                && !draft.source.trim().is_empty()
                && (span >= 8_000
                    || source_quiet >= 1_200
                    || (terminal(&draft.source) && source_quiet >= 350))
            {
                draft.source_closed = true;
                draft.source_closed_at_ms = Some(capture_clock_ms);
                events.push(SessionEvent::TranscriptFinalized {
                    segment: draft.segment(),
                });
            }
            let translation_quiet = capture_clock_ms.saturating_sub(draft.last_translation_at_ms);
            if !draft.translation_closed
                && !draft.translation.trim().is_empty()
                && (span >= 8_000
                    || translation_quiet >= 1_200
                    || (terminal(&draft.translation) && translation_quiet >= 350))
            {
                draft.translation_closed = true;
                events.push(SessionEvent::CaptionUpserted {
                    segment: draft.segment(),
                });
            }
        }
        events.extend(self.advance(capture_clock_ms));
        events
    }

    fn advance(&mut self, capture_clock_ms: u64) -> Vec<SessionEvent> {
        self.display_cursor_ms = self
            .display_cursor_ms
            .max(capture_clock_ms.saturating_sub(self.lag.target_delay_ms));
        let next_visible = match self.mode {
            LiveSyncMode::FastSource => self
                .drafts
                .iter()
                .rev()
                .find(|draft| !draft.source.trim().is_empty())
                .map(|draft| draft.id.clone()),
            LiveSyncMode::Coordinated => self
                .drafts
                .iter()
                .rev()
                .find(|draft| {
                    let fallback = draft
                        .source_closed_at_ms
                        .is_some_and(|closed| capture_clock_ms.saturating_sub(closed) >= 6_000);
                    draft.source_closed
                        && draft.start_ms <= self.display_cursor_ms
                        && ((!draft.translation.trim().is_empty() && draft.translation_closed)
                            || fallback)
                })
                .map(|draft| draft.id.clone()),
        };
        if next_visible.is_some() {
            self.visible_segment_id = next_visible;
        }
        let fallback_visible = self
            .visible_segment_id
            .as_ref()
            .and_then(|id| self.drafts.iter().find(|draft| &draft.id == id))
            .is_some_and(|draft| draft.translation.trim().is_empty());
        let sync = LiveSyncState {
            target_delay_ms: self.lag.target_delay_ms,
            observed_lag_ms: self.lag.observed_lag_ms,
            status: if fallback_visible {
                LiveSyncStatus::Degraded
            } else {
                self.lag.status.clone()
            },
            visible_segment_id: self.visible_segment_id.clone(),
        };
        if self.last_emitted_sync.as_ref() == Some(&sync) {
            Vec::new()
        } else {
            self.last_emitted_sync = Some(sync.clone());
            vec![SessionEvent::LiveSyncChanged { sync }]
        }
    }

    fn begin_epoch(&mut self, capture_clock_ms: u64) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        for draft in &mut self.drafts {
            if !draft.source.trim().is_empty() && !draft.source_closed {
                draft.source_closed = true;
                draft.source_closed_at_ms = Some(capture_clock_ms);
                events.push(SessionEvent::TranscriptFinalized {
                    segment: draft.segment(),
                });
            }
            if !draft.translation.trim().is_empty() && !draft.translation_closed {
                draft.translation_closed = true;
                events.push(SessionEvent::CaptionUpserted {
                    segment: draft.segment(),
                });
            }
        }
        self.epoch_offset_ms = capture_clock_ms;
        self.seen_event_ids.clear();
        events.extend(self.advance(capture_clock_ms));
        events
    }

    fn finish(&mut self, capture_clock_ms: u64) -> Vec<SessionEvent> {
        self.begin_epoch(capture_clock_ms)
    }
}

fn terminal(text: &str) -> bool {
    text.trim_end()
        .chars()
        .last()
        .is_some_and(|character| matches!(character, '.' | '?' | '!' | '。' | '？' | '！' | '…'))
}

fn emit_events(app: &tauri::AppHandle, events: Vec<SessionEvent>) {
    for event in events {
        let _ = record_event(app, event);
    }
}

pub async fn start(
    app: tauri::AppHandle,
    state: &LiveState,
    api_key: String,
    languages: LanguageSettings,
    sync_mode: LiveSyncMode,
) -> Result<(), ApiError> {
    abort_previous(state);
    state.cancelled.store(false, Ordering::Relaxed);

    let mut picker_config = SCContentSharingPickerConfiguration::new();
    picker_config.set_allows_changing_selected_content(false);
    picker_config.set_allowed_picker_modes(&[
        SCContentSharingPickerMode::SingleDisplay,
        SCContentSharingPickerMode::SingleApplication,
        SCContentSharingPickerMode::SingleWindow,
    ]);
    let picked = AsyncSCContentSharingPicker::show(&picker_config).await;
    let filter = match picked {
        SCPickerOutcome::Picked(result) => result.filter(),
        SCPickerOutcome::Cancelled => {
            return Err(ApiError {
                kind: ApiErrorKind::Service,
                message: "Live caption selection was cancelled.".into(),
                retryable: true,
            })
        }
        SCPickerOutcome::Error(error) => {
            return Err(capture_error(&format!(
                "Apple's capture picker failed: {error}"
            )))
        }
    };

    let config = SCStreamConfiguration::new()
        .with_captures_audio(true)
        .with_sample_rate(48_000)
        .with_channel_count(1)
        .with_excludes_current_process_audio(true);
    let stream = AsyncSCStream::new(&filter, &config, 8, SCStreamOutputType::Audio);

    let (mut writer, mut reader) = connect_translation(&api_key, &languages.target).await?;

    stream.start_capture().await.map_err(|error| {
        capture_error(&format!("System audio capture could not start: {error}"))
    })?;
    let _ = record_event(
        &app,
        SessionEvent::PhaseChanged {
            phase: "buffering".into(),
        },
    );

    let cancelled = state.cancelled.clone();
    let task = tauri::async_runtime::spawn(async move {
        let mut pcm = Vec::<i16>::with_capacity(SEND_SAMPLES * 2);
        let mut coordinator = CaptionCoordinator::new(sync_mode);
        let mut sent_samples = 0_u64;
        let mut ready = false;
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        let mut closing = false;
        let mut close_started = None;
        let mut closed = false;
        let mut reconnect_used = false;

        'capture: loop {
            if cancelled.load(Ordering::Relaxed) && !closing {
                closing = true;
                close_started = Some(Instant::now());
                let _ = writer
                    .send(Message::Text(
                        json!({ "type": "session.close" }).to_string().into(),
                    ))
                    .await;
            }
            if closed
                || close_started.is_some_and(|started| started.elapsed() > Duration::from_secs(2))
            {
                break;
            }

            tokio::select! {
                sample = stream.next(), if !closing => {
                    match sample {
                        Some(sample) => {
                            if let Some(list) = sample.audio_buffer_list() {
                                for buffer in list.iter() { append_f32_48k_as_pcm16_24k(buffer.data(), &mut pcm); }
                            } else {
                                emit_recoverable(&app, "capture_buffer", "A system-audio buffer could not be read.");
                            }
                            // The CoreMedia buffer is dropped before the socket await; it is not Send.
                            drop(sample);
                            {
                                while pcm.len() >= SEND_SAMPLES {
                                    let samples: Vec<i16> = pcm.drain(..SEND_SAMPLES).collect();
                                    let mut bytes = Vec::with_capacity(samples.len() * 2);
                                    for sample in samples { bytes.extend_from_slice(&sample.to_le_bytes()); }
                                    let event = json!({ "type": "session.input_audio_buffer.append", "audio": BASE64.encode(bytes) });
                                    if writer.send(Message::Text(event.to_string().into())).await.is_err() {
                                        if !reconnect_used {
                                            reconnect_used = true;
                                            let _ = record_event(&app, SessionEvent::PhaseChanged { phase: "reconnecting".into() });
                                            if let Ok((next_writer, next_reader)) = connect_translation(&api_key, &languages.target).await {
                                                writer = next_writer;
                                                reader = next_reader;
                                                emit_events(&app, coordinator.begin_epoch(capture_clock_ms(sent_samples)));
                                                let _ = record_event(&app, SessionEvent::PhaseChanged { phase: "ready".into() });
                                                continue 'capture;
                                            }
                                        }
                                        emit_recoverable(&app, "live_disconnected", "Live translation disconnected and the automatic reconnect did not succeed.");
                                        closing = true;
                                        break;
                                    }
                                    sent_samples = sent_samples.saturating_add(SEND_SAMPLES as u64);
                                }
                            }
                        }
                        None => break,
                    }
                }
                message = reader.next() => {
                    let Some(message) = message else {
                        if !reconnect_used {
                            reconnect_used = true;
                            let _ = record_event(&app, SessionEvent::PhaseChanged { phase: "reconnecting".into() });
                            if let Ok((next_writer, next_reader)) = connect_translation(&api_key, &languages.target).await {
                                writer = next_writer;
                                reader = next_reader;
                                emit_events(&app, coordinator.begin_epoch(capture_clock_ms(sent_samples)));
                                let _ = record_event(&app, SessionEvent::PhaseChanged { phase: "ready".into() });
                                continue 'capture;
                            }
                        }
                        emit_recoverable(&app, "live_disconnected", "Live translation disconnected and the automatic reconnect did not succeed.");
                        break;
                    };
                    match message {
                        Ok(Message::Text(text)) => match parse_realtime_event(&text) {
                            RealtimeEvent::SourceDelta(delta) => {
                                if !ready {
                                    ready = true;
                                    let _ = record_event(&app, SessionEvent::PhaseChanged { phase: "ready".into() });
                                }
                                emit_events(&app, coordinator.on_source(delta, capture_clock_ms(sent_samples)));
                            }
                            RealtimeEvent::TranslationDelta(delta) => {
                                if !ready {
                                    ready = true;
                                    let _ = record_event(&app, SessionEvent::PhaseChanged { phase: "ready".into() });
                                }
                                emit_events(&app, coordinator.on_translation(delta, capture_clock_ms(sent_samples)));
                            }
                            RealtimeEvent::SourceDone(done) => {
                                emit_events(&app, coordinator.on_done(true, done, capture_clock_ms(sent_samples)));
                            }
                            RealtimeEvent::TranslationDone(done) => {
                                emit_events(&app, coordinator.on_done(false, done, capture_clock_ms(sent_samples)));
                            }
                            RealtimeEvent::Closed => closed = true,
                            RealtimeEvent::Error(message) => emit_recoverable(&app, "realtime_error", &message),
                            RealtimeEvent::Ignored => {}
                        },
                        Ok(Message::Close(_)) => break,
                        Ok(_) => {}
                        Err(error) => {
                            if !reconnect_used {
                                reconnect_used = true;
                                let _ = record_event(&app, SessionEvent::PhaseChanged { phase: "reconnecting".into() });
                                if let Ok((next_writer, next_reader)) = connect_translation(&api_key, &languages.target).await {
                                    writer = next_writer;
                                    reader = next_reader;
                                    emit_events(&app, coordinator.begin_epoch(capture_clock_ms(sent_samples)));
                                    let _ = record_event(&app, SessionEvent::PhaseChanged { phase: "ready".into() });
                                    continue 'capture;
                                }
                            }
                            emit_recoverable(&app, "live_disconnected", &format!("Live translation connection ended: {error}"));
                            break;
                        }
                    }
                }
                _ = tick.tick() => {
                    emit_events(&app, coordinator.tick(capture_clock_ms(sent_samples)));
                }
            }
        }

        emit_events(&app, coordinator.finish(capture_clock_ms(sent_samples)));
        let _ = stream.stop_capture().await;
        let _ = record_event(&app, SessionEvent::Complete);
    });
    *state
        .task
        .lock()
        .map_err(|_| capture_error("Live task state is unavailable."))? = Some(task);
    Ok(())
}

fn capture_clock_ms(sent_samples: u64) -> u64 {
    sent_samples.saturating_mul(1_000) / 24_000
}

pub fn stop(state: &LiveState) {
    state.cancelled.store(true, Ordering::Relaxed);
}

fn abort_previous(state: &LiveState) {
    state.cancelled.store(true, Ordering::Relaxed);
    if let Ok(mut task) = state.task.lock() {
        if let Some(task) = task.take() {
            task.abort();
        }
    }
}

async fn connect_translation(
    api_key: &str,
    target: &str,
) -> Result<(RealtimeWriter, RealtimeReader), ApiError> {
    let mut request = REALTIME_URL.into_client_request().map_err(|error| {
        network_error(&format!("Could not prepare realtime connection: {error}"))
    })?;
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {api_key}")
            .parse()
            .map_err(|_| network_error("Could not authorize the realtime connection."))?,
    );
    let (socket, _) = connect_async(request).await.map_err(|error| {
        network_error(&format!("Could not connect realtime translation: {error}"))
    })?;
    let (mut writer, reader) = socket.split();
    writer
        .send(Message::Text(
            realtime_session_update(target).to_string().into(),
        ))
        .await
        .map_err(|error| {
            network_error(&format!(
                "Could not configure realtime translation: {error}"
            ))
        })?;
    Ok((writer, reader))
}

fn realtime_session_update(target: &str) -> Value {
    json!({
        "type": "session.update",
        "session": {
            "audio": {
                "input": { "transcription": { "model": "gpt-realtime-whisper" } },
                "output": { "language": target }
            }
        }
    })
}

fn append_f32_48k_as_pcm16_24k(bytes: &[u8], output: &mut Vec<i16>) {
    let mut frames = bytes.chunks_exact(4);
    while let Some(first) = frames.next() {
        let Some(second) = frames.next() else {
            break;
        };
        let a = f32::from_ne_bytes(first.try_into().expect("four-byte float"));
        let b = f32::from_ne_bytes(second.try_into().expect("four-byte float"));
        let averaged = ((a + b) * 0.5).clamp(-1.0, 1.0);
        output.push((averaged * i16::MAX as f32).round() as i16);
    }
}

fn parse_realtime_event(text: &str) -> RealtimeEvent {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return RealtimeEvent::Ignored;
    };
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let timed_text = |field: &str| TimedText {
        event_id: value
            .get("event_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        text: value
            .get(field)
            .or_else(|| {
                (field != "transcript")
                    .then(|| value.get("transcript"))
                    .flatten()
            })
            .or_else(|| value.get("text"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        elapsed_ms: value.get("elapsed_ms").and_then(Value::as_u64),
    };
    match event_type {
        "session.input_transcript.delta" => RealtimeEvent::SourceDelta(timed_text("delta")),
        "session.output_transcript.delta" => RealtimeEvent::TranslationDelta(timed_text("delta")),
        "session.input_transcript.done" | "session.input_transcript.completed" => {
            RealtimeEvent::SourceDone(timed_text("transcript"))
        }
        "session.output_transcript.done" | "session.output_transcript.completed" => {
            RealtimeEvent::TranslationDone(timed_text("transcript"))
        }
        "session.closed" => RealtimeEvent::Closed,
        "error" => RealtimeEvent::Error(
            value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("Realtime translation reported an error.")
                .to_owned(),
        ),
        _ => RealtimeEvent::Ignored,
    }
}

fn emit_recoverable(app: &tauri::AppHandle, code: &str, message: &str) {
    let _ = record_event(
        app,
        SessionEvent::RecoverableError {
            error: RecoverableError {
                code: code.into(),
                message: message.into(),
                segment_id: None,
            },
        },
    );
}

fn capture_error(message: &str) -> ApiError {
    ApiError {
        kind: ApiErrorKind::Service,
        message: message.into(),
        retryable: true,
    }
}

fn network_error(message: &str) -> ApiError {
    ApiError {
        kind: ApiErrorKind::Network,
        message: message.into(),
        retryable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_48k_float_audio_to_continuous_24k_pcm16() {
        let input: Vec<u8> = [0.5_f32, 0.5, -2.0, -2.0]
            .into_iter()
            .flat_map(f32::to_ne_bytes)
            .collect();
        let mut output = Vec::new();
        append_f32_48k_as_pcm16_24k(&input, &mut output);
        assert_eq!(output.len(), 2);
        assert!((output[0] - 16_384).abs() <= 1);
        assert_eq!(output[1], -32_767);
    }

    #[test]
    fn parses_realtime_translation_deltas_and_errors() {
        assert_eq!(
            parse_realtime_event(
                r#"{"event_id":"source-1","type":"session.input_transcript.delta","delta":"今日は","elapsed_ms":1200}"#
            ),
            RealtimeEvent::SourceDelta(TimedText {
                event_id: Some("source-1".into()),
                text: "今日は".into(),
                elapsed_ms: Some(1_200)
            })
        );
        assert_eq!(
            parse_realtime_event(
                r#"{"event_id":"target-1","type":"session.output_transcript.delta","delta":"Today","elapsed_ms":1200}"#
            ),
            RealtimeEvent::TranslationDelta(TimedText {
                event_id: Some("target-1".into()),
                text: "Today".into(),
                elapsed_ms: Some(1_200)
            })
        );
        assert_eq!(
            parse_realtime_event(r#"{"type":"error","error":{"message":"bad audio"}}"#),
            RealtimeEvent::Error("bad audio".into())
        );
    }

    #[test]
    fn keeps_append_only_deltas_with_shared_alignment_times() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinator.on_source(
            TimedText {
                event_id: Some("s1".into()),
                text: "何".into(),
                elapsed_ms: Some(200),
            },
            1_000,
        );
        coordinator.on_source(
            TimedText {
                event_id: Some("s2".into()),
                text: "ですか？".into(),
                elapsed_ms: Some(200),
            },
            1_100,
        );
        coordinator.on_source(
            TimedText {
                event_id: Some("s2".into()),
                text: "duplicate".into(),
                elapsed_ms: Some(200),
            },
            1_100,
        );
        assert_eq!(coordinator.drafts[0].source, "何ですか？");
    }

    #[test]
    fn grows_delay_immediately_and_reduces_it_slowly() {
        let mut estimator = LagEstimator::default();
        estimator.observe(5_000, 1_000);
        assert_eq!(estimator.target_delay_ms, 4_600);
        estimator.observe(40_000, 39_000);
        assert_eq!(estimator.target_delay_ms, 4_400);
    }

    #[test]
    fn delay_is_clamped_and_display_cursor_never_rewinds() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinator.tick(17_500);
        assert_eq!(coordinator.display_cursor_ms, 15_000);
        coordinator.on_translation(
            TimedText {
                event_id: Some("slow".into()),
                text: "Slow".into(),
                elapsed_ms: Some(0),
            },
            20_000,
        );
        assert_eq!(coordinator.lag.target_delay_ms, 6_000);
        coordinator.tick(6_000);
        assert_eq!(coordinator.display_cursor_ms, 15_000);
    }

    #[test]
    fn coordinated_mode_waits_for_translation_and_fast_source_does_not() {
        let source = TimedText {
            event_id: Some("s1".into()),
            text: "今日はちょっと…。".into(),
            elapsed_ms: Some(200),
        };
        let mut coordinated = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinated.on_source(source.clone(), 1_000);
        coordinated.tick(2_300);
        assert_eq!(coordinated.visible_segment_id, None);
        coordinated.on_translation(
            TimedText {
                event_id: Some("t1".into()),
                text: "Today is difficult.".into(),
                elapsed_ms: Some(200),
            },
            2_400,
        );
        coordinated.tick(3_700);
        assert_eq!(coordinated.visible_segment_id.as_deref(), Some("live-1"));

        let mut fast = CaptionCoordinator::new(LiveSyncMode::FastSource);
        fast.on_source(source, 1_000);
        assert_eq!(fast.visible_segment_id.as_deref(), Some("live-1"));
    }

    #[test]
    fn coordinated_mode_falls_back_to_source_after_six_seconds() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinator.on_source(
            TimedText {
                event_id: Some("s1".into()),
                text: "長い話です。".into(),
                elapsed_ms: Some(200),
            },
            1_000,
        );
        coordinator.tick(2_300);
        coordinator.tick(8_300);
        assert_eq!(coordinator.visible_segment_id.as_deref(), Some("live-1"));
        assert_eq!(
            coordinator
                .last_emitted_sync
                .as_ref()
                .map(|sync| &sync.status),
            Some(&LiveSyncStatus::Degraded)
        );
        coordinator.on_translation(
            TimedText {
                event_id: Some("t1".into()),
                text: "This is a long story.".into(),
                elapsed_ms: Some(200),
            },
            8_500,
        );
        coordinator.tick(9_800);
        assert_eq!(coordinator.drafts.len(), 1);
        assert_eq!(coordinator.drafts[0].id, "live-1");
        assert_eq!(coordinator.drafts[0].translation, "This is a long story.");
    }

    #[test]
    fn reconnect_starts_a_new_alignment_epoch() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinator.begin_epoch(12_000);
        coordinator.on_source(
            TimedText {
                event_id: Some("s1".into()),
                text: "再接続".into(),
                elapsed_ms: Some(400),
            },
            12_500,
        );
        assert_eq!(coordinator.drafts[0].start_ms, 12_400);
    }

    #[test]
    fn missing_alignment_is_kept_out_of_lag_estimation() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinator.on_translation(
            TimedText {
                event_id: Some("t1".into()),
                text: "Untimed".into(),
                elapsed_ms: None,
            },
            4_000,
        );
        assert_eq!(coordinator.lag.observed_lag_ms, 0);
        assert_eq!(coordinator.drafts[0].translation, "Untimed");
    }

    #[test]
    fn aligned_audio_span_forces_a_new_source_clause() {
        let mut coordinator = CaptionCoordinator::new(LiveSyncMode::Coordinated);
        coordinator.on_source(
            TimedText {
                event_id: Some("s1".into()),
                text: "First clause".into(),
                elapsed_ms: Some(0),
            },
            1_000,
        );
        coordinator.on_source(
            TimedText {
                event_id: Some("s2".into()),
                text: " keeps going".into(),
                elapsed_ms: Some(8_200),
            },
            9_200,
        );
        coordinator.tick(9_200);
        coordinator.on_source(
            TimedText {
                event_id: Some("s3".into()),
                text: "Second clause".into(),
                elapsed_ms: Some(8_400),
            },
            9_400,
        );
        assert_eq!(coordinator.drafts.len(), 2);
        assert!(coordinator.drafts[0].source_closed);
        assert_eq!(coordinator.drafts[1].source, "Second clause");
    }

    #[test]
    fn requests_source_transcripts_for_bilingual_live_captions() {
        let event = realtime_session_update("ja");
        assert_eq!(
            event["session"]["audio"]["input"]["transcription"]["model"],
            "gpt-realtime-whisper"
        );
        assert_eq!(event["session"]["audio"]["output"]["language"], "ja");
    }
}
