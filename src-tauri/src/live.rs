use crate::{
    contracts::{LanguageSettings, RecoverableError, SegmentStatus, SessionEvent, SessionMode, SubtitleSegment},
    openai::{ApiError, ApiErrorKind},
    record_event,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures_util::{stream::{SplitSink, SplitStream}, SinkExt, StreamExt};
use screencapturekit::{
    async_api::{AsyncSCContentSharingPicker, AsyncSCStream},
    cm::CMSampleBufferExt,
    content_sharing_picker::{SCContentSharingPickerConfiguration, SCContentSharingPickerMode, SCPickerOutcome},
    stream::{configuration::SCStreamConfiguration, output_type::SCStreamOutputType},
};
use serde_json::{json, Value};
use std::{
    sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex},
    time::{Duration, Instant},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream, WebSocketStream,
};

const REALTIME_URL: &str = "wss://api.openai.com/v1/realtime/translations?model=gpt-realtime-translate";
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
enum RealtimeEvent {
    SourceDelta(String),
    TranslationDelta(String),
    SourceDone(Option<String>),
    TranslationDone(Option<String>),
    Closed,
    Error(String),
    Ignored,
}

#[derive(Debug)]
struct CaptionAssembler {
    session_started: Instant,
    segment_started_ms: u64,
    segment_index: u64,
    source: String,
    translation: String,
    last_delta: Instant,
}

impl CaptionAssembler {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            session_started: now,
            segment_started_ms: 0,
            segment_index: 1,
            source: String::new(),
            translation: String::new(),
            last_delta: now,
        }
    }

    fn has_text(&self) -> bool {
        !self.source.trim().is_empty() || !self.translation.trim().is_empty()
    }

    fn elapsed_ms(&self) -> u64 {
        self.session_started.elapsed().as_millis() as u64
    }

    fn id(&self) -> String {
        format!("live-{}", self.segment_index)
    }

    fn provisional(&self) -> SubtitleSegment {
        self.segment(true)
    }

    fn segment(&self, provisional: bool) -> SubtitleSegment {
        SubtitleSegment {
            id: self.id(),
            origin: SessionMode::Live,
            start_ms: self.segment_started_ms,
            end_ms: self.elapsed_ms().max(self.segment_started_ms + 250),
            source_text: self.source.trim().to_owned(),
            translation_text: (!self.translation.trim().is_empty()).then(|| self.translation.trim().to_owned()),
            ambiguity_note: None,
            speaker_id: Some("live-audio".into()),
            is_provisional: provisional,
            transcription_status: if provisional { SegmentStatus::Pending } else { SegmentStatus::Complete },
            translation_status: if provisional || self.translation.trim().is_empty() {
                SegmentStatus::Pending
            } else {
                SegmentStatus::Complete
            },
        }
    }

    fn finish(&mut self) -> Option<SubtitleSegment> {
        if !self.has_text() { return None; }
        let segment = self.segment(false);
        self.segment_index += 1;
        self.segment_started_ms = segment.end_ms;
        self.source.clear();
        self.translation.clear();
        Some(segment)
    }
}

pub async fn start(
    app: tauri::AppHandle,
    state: &LiveState,
    api_key: String,
    languages: LanguageSettings,
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
        SCPickerOutcome::Cancelled => return Err(ApiError {
            kind: ApiErrorKind::Service,
            message: "Live caption selection was cancelled.".into(),
            retryable: true,
        }),
        SCPickerOutcome::Error(error) => return Err(capture_error(&format!("Apple's capture picker failed: {error}"))),
    };

    let config = SCStreamConfiguration::new()
        .with_captures_audio(true)
        .with_sample_rate(48_000)
        .with_channel_count(1)
        .with_excludes_current_process_audio(true);
    let stream = AsyncSCStream::new(&filter, &config, 8, SCStreamOutputType::Audio);

    let (mut writer, mut reader) = connect_translation(&api_key, &languages.target).await?;

    stream.start_capture().await
        .map_err(|error| capture_error(&format!("System audio capture could not start: {error}")))?;
    let _ = record_event(&app, SessionEvent::PhaseChanged { phase: "buffering".into() });

    let cancelled = state.cancelled.clone();
    let task = tauri::async_runtime::spawn(async move {
        let mut pcm = Vec::<i16>::with_capacity(SEND_SAMPLES * 2);
        let mut assembler = CaptionAssembler::new();
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
                let _ = writer.send(Message::Text(json!({ "type": "session.close" }).to_string().into())).await;
            }
            if closed || close_started.is_some_and(|started| started.elapsed() > Duration::from_secs(2)) { break; }

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
                                                let _ = record_event(&app, SessionEvent::PhaseChanged { phase: "ready".into() });
                                                continue 'capture;
                                            }
                                        }
                                        emit_recoverable(&app, "live_disconnected", "Live translation disconnected and the automatic reconnect did not succeed.");
                                        closing = true;
                                        break;
                                    }
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
                                assembler.source.push_str(&delta);
                                assembler.last_delta = Instant::now();
                                if !ready {
                                    ready = true;
                                    let _ = record_event(&app, SessionEvent::PhaseChanged { phase: "ready".into() });
                                }
                                let _ = record_event(&app, SessionEvent::CaptionUpserted { segment: assembler.provisional() });
                            }
                            RealtimeEvent::TranslationDelta(delta) => {
                                assembler.translation.push_str(&delta);
                                assembler.last_delta = Instant::now();
                                if !ready {
                                    ready = true;
                                    let _ = record_event(&app, SessionEvent::PhaseChanged { phase: "ready".into() });
                                }
                                let _ = record_event(&app, SessionEvent::CaptionUpserted { segment: assembler.provisional() });
                            }
                            RealtimeEvent::SourceDone(text) => {
                                if let Some(text) = text { assembler.source = text; }
                                assembler.last_delta = Instant::now();
                            }
                            RealtimeEvent::TranslationDone(text) => {
                                if let Some(text) = text { assembler.translation = text; }
                                if let Some(segment) = assembler.finish() {
                                    let _ = record_event(&app, SessionEvent::TranscriptFinalized { segment });
                                }
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
                    if assembler.has_text()
                        && !assembler.translation.trim().is_empty()
                        && assembler.last_delta.elapsed() > Duration::from_millis(1_200)
                    {
                        if let Some(segment) = assembler.finish() {
                            let _ = record_event(&app, SessionEvent::TranscriptFinalized { segment });
                        }
                    }
                }
            }
        }

        if let Some(segment) = assembler.finish() {
            let _ = record_event(&app, SessionEvent::TranscriptFinalized { segment });
        }
        let _ = stream.stop_capture().await;
        let _ = record_event(&app, SessionEvent::Complete);
    });
    *state.task.lock().map_err(|_| capture_error("Live task state is unavailable."))? = Some(task);
    Ok(())
}

pub fn stop(state: &LiveState) {
    state.cancelled.store(true, Ordering::Relaxed);
}

fn abort_previous(state: &LiveState) {
    state.cancelled.store(true, Ordering::Relaxed);
    if let Ok(mut task) = state.task.lock() {
        if let Some(task) = task.take() { task.abort(); }
    }
}

async fn connect_translation(api_key: &str, target: &str) -> Result<(RealtimeWriter, RealtimeReader), ApiError> {
    let mut request = REALTIME_URL.into_client_request()
        .map_err(|error| network_error(&format!("Could not prepare realtime connection: {error}")))?;
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {api_key}").parse()
            .map_err(|_| network_error("Could not authorize the realtime connection."))?,
    );
    let (socket, _) = connect_async(request).await
        .map_err(|error| network_error(&format!("Could not connect realtime translation: {error}")))?;
    let (mut writer, reader) = socket.split();
    writer.send(Message::Text(realtime_session_update(target).to_string().into())).await
        .map_err(|error| network_error(&format!("Could not configure realtime translation: {error}")))?;
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
    loop {
        let Some(first) = frames.next() else { break; };
        let Some(second) = frames.next() else { break; };
        let a = f32::from_ne_bytes(first.try_into().expect("four-byte float"));
        let b = f32::from_ne_bytes(second.try_into().expect("four-byte float"));
        let averaged = ((a + b) * 0.5).clamp(-1.0, 1.0);
        output.push((averaged * i16::MAX as f32).round() as i16);
    }
}

fn parse_realtime_event(text: &str) -> RealtimeEvent {
    let Ok(value) = serde_json::from_str::<Value>(text) else { return RealtimeEvent::Ignored; };
    let event_type = value.get("type").and_then(Value::as_str).unwrap_or_default();
    let delta = || value.get("delta").and_then(Value::as_str).unwrap_or_default().to_owned();
    let transcript = || value.get("transcript").or_else(|| value.get("text")).and_then(Value::as_str).map(str::to_owned);
    match event_type {
        "session.input_transcript.delta" => RealtimeEvent::SourceDelta(delta()),
        "session.output_transcript.delta" => RealtimeEvent::TranslationDelta(delta()),
        "session.input_transcript.done" | "session.input_transcript.completed" => RealtimeEvent::SourceDone(transcript()),
        "session.output_transcript.done" | "session.output_transcript.completed" => RealtimeEvent::TranslationDone(transcript()),
        "session.closed" => RealtimeEvent::Closed,
        "error" => RealtimeEvent::Error(value.pointer("/error/message").and_then(Value::as_str)
            .unwrap_or("Realtime translation reported an error.").to_owned()),
        _ => RealtimeEvent::Ignored,
    }
}

fn emit_recoverable(app: &tauri::AppHandle, code: &str, message: &str) {
    let _ = record_event(app, SessionEvent::RecoverableError { error: RecoverableError {
        code: code.into(), message: message.into(), segment_id: None,
    }});
}

fn capture_error(message: &str) -> ApiError {
    ApiError { kind: ApiErrorKind::Service, message: message.into(), retryable: true }
}

fn network_error(message: &str) -> ApiError {
    ApiError { kind: ApiErrorKind::Network, message: message.into(), retryable: true }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_48k_float_audio_to_continuous_24k_pcm16() {
        let input: Vec<u8> = [0.5_f32, 0.5, -2.0, -2.0].into_iter().flat_map(f32::to_ne_bytes).collect();
        let mut output = Vec::new();
        append_f32_48k_as_pcm16_24k(&input, &mut output);
        assert_eq!(output.len(), 2);
        assert!((output[0] - 16_384).abs() <= 1);
        assert_eq!(output[1], -32_767);
    }

    #[test]
    fn parses_realtime_translation_deltas_and_errors() {
        assert_eq!(
            parse_realtime_event(r#"{"type":"session.input_transcript.delta","delta":"今日は"}"#),
            RealtimeEvent::SourceDelta("今日は".into())
        );
        assert_eq!(
            parse_realtime_event(r#"{"type":"session.output_transcript.delta","delta":"Today"}"#),
            RealtimeEvent::TranslationDelta("Today".into())
        );
        assert_eq!(
            parse_realtime_event(r#"{"type":"error","error":{"message":"bad audio"}}"#),
            RealtimeEvent::Error("bad audio".into())
        );
    }

    #[test]
    fn requests_source_transcripts_for_bilingual_live_captions() {
        let event = realtime_session_update("ja");
        assert_eq!(event["session"]["audio"]["input"]["transcription"]["model"], "gpt-realtime-whisper");
        assert_eq!(event["session"]["audio"]["output"]["language"], "ja");
    }
}
