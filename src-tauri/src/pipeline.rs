use std::{
    collections::{HashMap, HashSet},
    sync::{atomic::{AtomicBool, Ordering}, Arc},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use tauri::ipc::Channel;

use crate::{
    chunking::{normalize_chunk_timestamp, AudioChunk},
    media::{write_wav, DecodedAudio},
    openai::{ApiError, ApiErrorKind, DiarizedSegment, OpenAiClient, SpeakerReference, TranslationInput},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStatus {
    Complete,
    Failed,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PipelineSegment {
    pub id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub source_text: String,
    pub natural_english: Option<String>,
    pub ambiguity_note: Option<String>,
    pub speaker_id: String,
    pub transcription_status: PipelineStatus,
    pub translation_status: PipelineStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineSpeaker {
    pub id: String,
    pub display_name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum PipelineEvent {
    PhaseChanged { phase: &'static str },
    TranscriptFinalized { segment: PipelineSegment },
    TranslationFinalized { segment_id: String, natural_english: String, ambiguity_note: Option<String> },
    SpeakerDiscovered { speaker: PipelineSpeaker },
    CoverageChanged { translated_through_ms: u64 },
    RecoverableError { code: String, message: String, segment_id: Option<String> },
    FatalError { message: String },
    Complete,
}

pub async fn run(
    client: OpenAiClient,
    audio: Arc<DecodedAudio>,
    chunks: Vec<AudioChunk>,
    cancelled: Arc<AtomicBool>,
    events: Channel<PipelineEvent>,
) -> Result<(), ApiError> {
    let result = run_inner(client, audio, chunks, cancelled, &events).await;
    if let Err(error) = &result {
        let _ = send(&events, PipelineEvent::FatalError { message: error.message.clone() });
    }
    result
}

async fn run_inner(
    client: OpenAiClient,
    audio: Arc<DecodedAudio>,
    chunks: Vec<AudioChunk>,
    cancelled: Arc<AtomicBool>,
    events: &Channel<PipelineEvent>,
) -> Result<(), ApiError> {
    send(events, PipelineEvent::PhaseChanged { phase: "transcribing" })?;
    let mut all_segments = Vec::<PipelineSegment>::new();
    let mut speaker_ids = HashMap::<String, String>::new();
    let mut references = Vec::<SpeakerReference>::new();

    for chunk in &chunks {
        check_cancelled(&cancelled)?;
        let remote = retry_transcription(&client, &chunk.path, &references, events).await?;
        let mut incoming = Vec::new();
        for remote_segment in remote {
            let stable_speaker = stable_speaker_id(&mut speaker_ids, &remote_segment.speaker);
            if !all_segments.iter().any(|segment| segment.speaker_id == stable_speaker)
                && !incoming.iter().any(|segment: &PipelineSegment| segment.speaker_id == stable_speaker)
            {
                let palette = ["#ff83bd", "#78dfc2", "#a995ff", "#ffbb6e"];
                let number = speaker_ids.len();
                send(events, PipelineEvent::SpeakerDiscovered { speaker: PipelineSpeaker {
                    id: stable_speaker.clone(),
                    display_name: format!("Speaker {number}"),
                    color: palette[(number - 1).min(palette.len() - 1)].into(),
                } })?;
            }
            incoming.push(to_pipeline_segment(chunk, remote_segment, stable_speaker));
        }
        merge_boundary_segments(&mut all_segments, incoming);
        for segment in all_segments.iter().filter(|segment| segment.translation_status == PipelineStatus::Failed) {
            send(events, PipelineEvent::TranscriptFinalized { segment: segment.clone() })?;
        }

        if chunk.index == 0 {
            references = build_speaker_references(&audio, chunk, &all_segments)?;
        }
        send(events, PipelineEvent::PhaseChanged { phase: "buffering" })?;
        translate_pending(&client, &mut all_segments, events, &cancelled).await?;
    }
    send(events, PipelineEvent::Complete)?;
    Ok(())
}

async fn retry_transcription(
    client: &OpenAiClient,
    path: &std::path::Path,
    references: &[SpeakerReference],
    events: &Channel<PipelineEvent>,
) -> Result<Vec<DiarizedSegment>, ApiError> {
    let first = client.transcribe_chunk(path, references).await;
    match first {
        Err(error) if error.retryable => {
            send(events, PipelineEvent::RecoverableError { code: format!("{:?}", error.kind).to_ascii_lowercase(), message: error.message, segment_id: None })?;
            client.transcribe_chunk(path, references).await
        },
        result => result,
    }
}

async fn translate_pending(
    client: &OpenAiClient,
    segments: &mut [PipelineSegment],
    events: &Channel<PipelineEvent>,
    cancelled: &AtomicBool,
) -> Result<(), ApiError> {
    let pending_indices = segments
        .iter()
        .enumerate()
        .filter(|(_, segment)| segment.translation_status == PipelineStatus::Failed)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    for indices in pending_indices.chunks(6) {
        check_cancelled(cancelled)?;
        let first_index = *indices.first().unwrap_or(&0);
        let context_start = first_index.saturating_sub(80);
        let context = segments[context_start..first_index]
            .iter()
            .map(translation_input)
            .collect::<Vec<_>>();
        let batch = indices.iter().map(|index| translation_input(&segments[*index])).collect::<Vec<_>>();
        let translations = match client.translate(&context, &batch).await {
            Ok(output) => output,
            Err(first) if first.retryable => match client.translate(&context, &batch).await {
                Ok(output) => output,
                Err(error) if matches!(error.kind, ApiErrorKind::Authentication | ApiErrorKind::ModelUnavailable) => return Err(error),
                Err(error) => {
                    for index in indices {
                        send(events, PipelineEvent::RecoverableError { code: "translation_failed".into(), message: error.message.clone(), segment_id: Some(segments[*index].id.clone()) })?;
                    }
                    continue;
                }
            },
            Err(error) => return Err(error),
        };
        for translation in translations {
            if let Some(segment) = segments.iter_mut().find(|segment| segment.id == translation.segment_id) {
                segment.natural_english = Some(translation.natural_english.clone());
                segment.ambiguity_note = translation.ambiguity_note.clone();
                segment.translation_status = PipelineStatus::Complete;
                send(events, PipelineEvent::TranslationFinalized {
                    segment_id: segment.id.clone(),
                    natural_english: translation.natural_english,
                    ambiguity_note: translation.ambiguity_note,
                })?;
            }
        }
        let translated_through_ms = segments
            .iter()
            .take_while(|segment| segment.translation_status == PipelineStatus::Complete)
            .map(|segment| segment.end_ms)
            .max()
            .unwrap_or(0);
        send(events, PipelineEvent::CoverageChanged { translated_through_ms })?;
        if translated_through_ms >= 15_000 {
            send(events, PipelineEvent::PhaseChanged { phase: "ready" })?;
        }
    }
    Ok(())
}

fn translation_input(segment: &PipelineSegment) -> TranslationInput {
    TranslationInput {
        segment_id: segment.id.clone(),
        speaker: segment.speaker_id.clone(),
        japanese: segment.source_text.clone(),
    }
}

fn stable_speaker_id(map: &mut HashMap<String, String>, remote: &str) -> String {
    if let Some(existing) = map.get(remote) {
        return existing.clone();
    }
    if remote.starts_with("speaker-") {
        map.insert(remote.to_owned(), remote.to_owned());
        return remote.to_owned();
    }
    let id = format!("speaker-{}", map.len() + 1);
    map.insert(remote.to_owned(), id.clone());
    id
}

fn to_pipeline_segment(chunk: &AudioChunk, remote: DiarizedSegment, speaker_id: String) -> PipelineSegment {
    let id = if remote.id.is_empty() { format!("chunk-{}-{:.0}", chunk.index, remote.start_seconds * 1_000.0) } else { format!("chunk-{}-{}", chunk.index, remote.id) };
    PipelineSegment {
        id,
        start_ms: normalize_chunk_timestamp(chunk, remote.start_seconds),
        end_ms: normalize_chunk_timestamp(chunk, remote.end_seconds),
        source_text: remote.text,
        natural_english: None,
        ambiguity_note: None,
        speaker_id,
        transcription_status: PipelineStatus::Complete,
        translation_status: PipelineStatus::Failed,
    }
}

pub fn merge_boundary_segments(existing: &mut Vec<PipelineSegment>, incoming: Vec<PipelineSegment>) {
    for candidate in incoming {
        let duplicate = existing.iter().position(|segment| {
            let overlap_start = segment.start_ms.max(candidate.start_ms);
            let overlap_end = segment.end_ms.min(candidate.end_ms);
            let overlap = overlap_end.saturating_sub(overlap_start);
            let shorter = (segment.end_ms - segment.start_ms).min(candidate.end_ms - candidate.start_ms).max(1);
            overlap * 100 / shorter >= 60 && (segment.source_text.contains(&candidate.source_text) || candidate.source_text.contains(&segment.source_text))
        });
        if let Some(index) = duplicate {
            if candidate.source_text.chars().count() > existing[index].source_text.chars().count() {
                existing[index] = candidate;
            }
        } else {
            existing.push(candidate);
        }
    }
    existing.sort_by_key(|segment| segment.start_ms);
}

fn build_speaker_references(
    audio: &DecodedAudio,
    chunk: &AudioChunk,
    segments: &[PipelineSegment],
) -> Result<Vec<SpeakerReference>, ApiError> {
    let mut seen = HashSet::new();
    let mut references = Vec::new();
    let directory = chunk.path.parent().ok_or_else(|| service_error("Temporary chunk directory is missing."))?;
    for segment in segments {
        let duration = segment.end_ms.saturating_sub(segment.start_ms);
        if !(2_000..=10_000).contains(&duration) || !seen.insert(segment.speaker_id.clone()) {
            continue;
        }
        let start = ((segment.start_ms as u128 * audio.sample_rate as u128) / 1_000) as usize;
        let end = ((segment.end_ms as u128 * audio.sample_rate as u128) / 1_000) as usize;
        if start >= end || end > audio.samples.len() {
            continue;
        }
        let path = directory.join(format!("reference-{}.wav", references.len() + 1));
        write_wav(&path, &audio.samples[start..end], audio.sample_rate).map_err(|message| service_error(&message))?;
        let bytes = std::fs::read(&path).map_err(|error| service_error(&format!("Could not read speaker reference: {error}")))?;
        references.push(SpeakerReference {
            name: segment.speaker_id.clone(),
            data_url: format!("data:audio/wav;base64,{}", STANDARD.encode(bytes)),
        });
        if references.len() == 4 {
            break;
        }
    }
    Ok(references)
}

fn send(events: &Channel<PipelineEvent>, event: PipelineEvent) -> Result<(), ApiError> {
    events.send(event).map_err(|error| service_error(&format!("The subtitle display disconnected: {error}")))
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<(), ApiError> {
    if cancelled.load(Ordering::Relaxed) {
        Err(ApiError { kind: ApiErrorKind::Service, message: "Analysis was cancelled.".into(), retryable: false })
    } else {
        Ok(())
    }
}

fn service_error(message: &str) -> ApiError {
    ApiError { kind: ApiErrorKind::Service, message: message.to_owned(), retryable: false }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(id: &str, start: u64, end: u64, text: &str) -> PipelineSegment {
        PipelineSegment { id: id.into(), start_ms: start, end_ms: end, source_text: text.into(), natural_english: None, ambiguity_note: None, speaker_id: "speaker-1".into(), transcription_status: PipelineStatus::Complete, translation_status: PipelineStatus::Failed }
    }

    #[test]
    fn overlapping_boundary_prefers_more_complete_text() {
        let mut existing = vec![segment("old", 10_000, 12_000, "今日は")];
        merge_boundary_segments(&mut existing, vec![segment("new", 10_050, 12_100, "今日はちょっと")]);
        assert_eq!(existing.len(), 1);
        assert_eq!(existing[0].id, "new");
    }

    #[test]
    fn distinct_overlap_is_preserved_for_stacked_subtitles() {
        let mut existing = vec![segment("a", 10_000, 12_000, "こんにちは")];
        merge_boundary_segments(&mut existing, vec![segment("b", 10_100, 11_900, "待って")]);
        assert_eq!(existing.len(), 2);
    }
}
