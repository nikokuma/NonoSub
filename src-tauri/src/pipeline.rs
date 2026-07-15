use std::{
    collections::{HashMap, HashSet},
    sync::{atomic::{AtomicBool, Ordering}, Arc},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use crate::{
    chunking::{normalize_chunk_timestamp, AudioChunk},
    contracts::{LanguageSettings, RecoverableError, SegmentStatus, SessionEvent, SessionMode, SpeakerProfile, SubtitleSegment},
    media::{write_wav, DecodedAudio},
    openai::{ApiError, ApiErrorKind, DiarizedSegment, OpenAiClient, SpeakerReference, TranslationInput},
};

pub type EventSink = Arc<dyn Fn(SessionEvent) -> Result<(), ApiError> + Send + Sync>;

pub async fn run(
    client: OpenAiClient,
    audio: Arc<DecodedAudio>,
    chunks: Vec<AudioChunk>,
    languages: LanguageSettings,
    cancelled: Arc<AtomicBool>,
    events: EventSink,
) -> Result<(), ApiError> {
    let result = run_inner(client, audio, chunks, languages, cancelled, &events).await;
    if let Err(error) = &result {
        let _ = send(&events, SessionEvent::FatalError { message: error.message.clone() });
    }
    result
}

async fn run_inner(
    client: OpenAiClient,
    audio: Arc<DecodedAudio>,
    chunks: Vec<AudioChunk>,
    languages: LanguageSettings,
    cancelled: Arc<AtomicBool>,
    events: &EventSink,
) -> Result<(), ApiError> {
    send(events, SessionEvent::PhaseChanged { phase: "transcribing".into() })?;
    let mut all_segments = Vec::<SubtitleSegment>::new();
    let mut speaker_ids = HashMap::<String, String>::new();
    let mut references = Vec::<SpeakerReference>::new();

    for chunk in &chunks {
        check_cancelled(&cancelled)?;
        let remote = retry_transcription(&client, &chunk.path, &references, &languages.source, events).await?;
        let mut incoming = Vec::new();
        for remote_segment in remote {
            let stable_speaker = stable_speaker_id(&mut speaker_ids, &remote_segment.speaker);
            if !all_segments.iter().any(|segment| segment.speaker_id.as_deref() == Some(&stable_speaker))
                && !incoming.iter().any(|segment: &SubtitleSegment| segment.speaker_id.as_deref() == Some(&stable_speaker))
            {
                let palette = ["#ff83bd", "#78dfc2", "#a995ff", "#ffbb6e"];
                let number = speaker_ids.len();
                send(events, SessionEvent::SpeakerDiscovered { speaker: SpeakerProfile {
                    id: stable_speaker.clone(),
                    display_name: format!("Speaker {number}"),
                    color: palette[(number - 1).min(palette.len() - 1)].into(),
                } })?;
            }
            incoming.push(to_pipeline_segment(chunk, remote_segment, stable_speaker));
        }
        merge_boundary_segments(&mut all_segments, incoming);
        for segment in all_segments.iter().filter(|segment| segment.translation_status == SegmentStatus::Failed) {
            send(events, SessionEvent::TranscriptFinalized { segment: segment.clone() })?;
        }

        if chunk.index == 0 {
            references = build_speaker_references(&audio, chunk, &all_segments)?;
        }
        send(events, SessionEvent::PhaseChanged { phase: "buffering".into() })?;
        translate_pending(&client, &mut all_segments, &languages, events, &cancelled).await?;
    }
    send(events, SessionEvent::Complete)?;
    Ok(())
}

async fn retry_transcription(
    client: &OpenAiClient,
    path: &std::path::Path,
    references: &[SpeakerReference],
    source_language: &str,
    events: &EventSink,
) -> Result<Vec<DiarizedSegment>, ApiError> {
    let first = client.transcribe_chunk(path, references, source_language).await;
    match first {
        Err(error) if error.retryable => {
            send(events, SessionEvent::RecoverableError { error: RecoverableError { code: format!("{:?}", error.kind).to_ascii_lowercase(), message: error.message, segment_id: None } })?;
            client.transcribe_chunk(path, references, source_language).await
        },
        result => result,
    }
}

async fn translate_pending(
    client: &OpenAiClient,
    segments: &mut [SubtitleSegment],
    languages: &LanguageSettings,
    events: &EventSink,
    cancelled: &AtomicBool,
) -> Result<(), ApiError> {
    let pending_indices = segments
        .iter()
        .enumerate()
        .filter(|(_, segment)| segment.translation_status == SegmentStatus::Failed)
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
        let translations = match client.translate(&context, &batch, languages).await {
            Ok(output) => output,
            Err(first) if first.retryable => match client.translate(&context, &batch, languages).await {
                Ok(output) => output,
                Err(error) if matches!(error.kind, ApiErrorKind::Authentication | ApiErrorKind::ModelUnavailable) => return Err(error),
                Err(error) => {
                    for index in indices {
                        send(events, SessionEvent::RecoverableError { error: RecoverableError { code: "translation_failed".into(), message: error.message.clone(), segment_id: Some(segments[*index].id.clone()) } })?;
                    }
                    continue;
                }
            },
            Err(error) => return Err(error),
        };
        for translation in translations {
            if let Some(segment) = segments.iter_mut().find(|segment| segment.id == translation.segment_id) {
                segment.translation_text = Some(translation.translation.clone());
                segment.ambiguity_note = translation.ambiguity_note.clone();
                segment.translation_status = SegmentStatus::Complete;
                send(events, SessionEvent::TranslationFinalized {
                    segment_id: segment.id.clone(),
                    translation_text: translation.translation,
                    ambiguity_note: translation.ambiguity_note,
                })?;
            }
        }
        let translated_through_ms = segments
            .iter()
            .take_while(|segment| segment.translation_status == SegmentStatus::Complete)
            .map(|segment| segment.end_ms)
            .max()
            .unwrap_or(0);
        send(events, SessionEvent::CoverageChanged { translated_through_ms })?;
        if translated_through_ms >= 15_000 {
            send(events, SessionEvent::PhaseChanged { phase: "ready".into() })?;
        }
    }
    Ok(())
}

fn translation_input(segment: &SubtitleSegment) -> TranslationInput {
    TranslationInput {
        segment_id: segment.id.clone(),
        speaker: segment.speaker_id.clone().unwrap_or_else(|| "speaker-unknown".into()),
        source_text: segment.source_text.clone(),
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

fn to_pipeline_segment(chunk: &AudioChunk, remote: DiarizedSegment, speaker_id: String) -> SubtitleSegment {
    let id = if remote.id.is_empty() { format!("chunk-{}-{:.0}", chunk.index, remote.start_seconds * 1_000.0) } else { format!("chunk-{}-{}", chunk.index, remote.id) };
    SubtitleSegment {
        id,
        origin: SessionMode::File,
        start_ms: normalize_chunk_timestamp(chunk, remote.start_seconds),
        end_ms: normalize_chunk_timestamp(chunk, remote.end_seconds),
        source_text: remote.text,
        translation_text: None,
        ambiguity_note: None,
        speaker_id: Some(speaker_id),
        is_provisional: false,
        transcription_status: SegmentStatus::Complete,
        translation_status: SegmentStatus::Failed,
    }
}

pub fn merge_boundary_segments(existing: &mut Vec<SubtitleSegment>, incoming: Vec<SubtitleSegment>) {
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
    segments: &[SubtitleSegment],
) -> Result<Vec<SpeakerReference>, ApiError> {
    let mut seen = HashSet::new();
    let mut references = Vec::new();
    let directory = chunk.path.parent().ok_or_else(|| service_error("Temporary chunk directory is missing."))?;
    for segment in segments {
        let duration = segment.end_ms.saturating_sub(segment.start_ms);
        let Some(speaker_id) = segment.speaker_id.as_ref() else { continue };
        if !(2_000..=10_000).contains(&duration) || !seen.insert(speaker_id.clone()) {
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
            name: speaker_id.clone(),
            data_url: format!("data:audio/wav;base64,{}", STANDARD.encode(bytes)),
        });
        if references.len() == 4 {
            break;
        }
    }
    Ok(references)
}

fn send(events: &EventSink, event: SessionEvent) -> Result<(), ApiError> {
    events(event)
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

    fn segment(id: &str, start: u64, end: u64, text: &str) -> SubtitleSegment {
        SubtitleSegment { id: id.into(), origin: SessionMode::File, start_ms: start, end_ms: end, source_text: text.into(), translation_text: None, ambiguity_note: None, speaker_id: Some("speaker-1".into()), is_provisional: false, transcription_status: SegmentStatus::Complete, translation_status: SegmentStatus::Failed }
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
