use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use crate::{
    chunking::{normalize_chunk_timestamp, AudioChunk},
    contracts::{
        CaptionProcessingMode, LanguageSettings, RecoverableError, SegmentStatus, SessionEvent,
        SessionMode, SpeakerProfile, SubtitleSegment,
    },
    media::{write_wav, DecodedAudio},
    openai::{
        ApiError, ApiErrorKind, DiarizedSegment, OpenAiClient, SpeakerReference, TranslationInput,
    },
};
use base64::{engine::general_purpose::STANDARD, Engine as _};

pub type EventSink = Arc<dyn Fn(SessionEvent) -> Result<(), ApiError> + Send + Sync>;

pub async fn run(
    client: OpenAiClient,
    audio: Arc<DecodedAudio>,
    chunks: Vec<AudioChunk>,
    languages: LanguageSettings,
    processing_mode: CaptionProcessingMode,
    cancelled: Arc<AtomicBool>,
    events: EventSink,
) -> Result<(), ApiError> {
    let result = run_inner(
        client,
        audio,
        chunks,
        languages,
        processing_mode,
        cancelled,
        &events,
    )
    .await;
    match result {
        Err(error) if error.kind == ApiErrorKind::Cancelled => Ok(()),
        Err(error) => {
            let _ = send(
                &events,
                SessionEvent::FatalError {
                    message: error.message.clone(),
                },
            );
            Err(error)
        }
        Ok(()) => Ok(()),
    }
}

async fn run_inner(
    client: OpenAiClient,
    audio: Arc<DecodedAudio>,
    chunks: Vec<AudioChunk>,
    languages: LanguageSettings,
    processing_mode: CaptionProcessingMode,
    cancelled: Arc<AtomicBool>,
    events: &EventSink,
) -> Result<(), ApiError> {
    send(
        events,
        SessionEvent::PhaseChanged {
            phase: "transcribing".into(),
        },
    )?;
    let mut all_segments = Vec::<SubtitleSegment>::new();
    let mut speaker_ids = HashMap::<String, String>::new();
    let mut references = Vec::<SpeakerReference>::new();
    let mut emitted_sources = HashMap::<String, String>::new();

    for chunk in &chunks {
        check_cancelled(&cancelled)?;
        let remote =
            retry_transcription(&client, &chunk.path, &references, &languages.source, events)
                .await?;
        let mut incoming = Vec::new();
        let mut last_stable_speaker = all_segments
            .iter()
            .rev()
            .find_map(|segment| segment.speaker_id.clone());
        for remote_segment in remote {
            if !has_speech_content(&remote_segment.text) {
                continue;
            }
            let stable_speaker = stable_speaker_id(
                &mut speaker_ids,
                &remote_segment.speaker,
                chunk.index == 0,
                last_stable_speaker.as_deref(),
            );
            if let Some(stable_speaker) = stable_speaker.as_ref() {
                last_stable_speaker = Some(stable_speaker.clone());
                if !all_segments
                    .iter()
                    .any(|segment| segment.speaker_id.as_deref() == Some(stable_speaker))
                    && !incoming.iter().any(|segment: &SubtitleSegment| {
                        segment.speaker_id.as_deref() == Some(stable_speaker)
                    })
                {
                    let palette = ["#ff83bd", "#78dfc2", "#a995ff", "#ffbb6e"];
                    let number = stable_speaker
                        .strip_prefix("speaker-")
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(1);
                    send(
                        events,
                        SessionEvent::SpeakerDiscovered {
                            speaker: SpeakerProfile {
                                id: stable_speaker.clone(),
                                display_name: format!("Speaker {number}"),
                                color: palette[(number - 1).min(palette.len() - 1)].into(),
                            },
                        },
                    )?;
                }
            }
            incoming.extend(split_long_segment(to_pipeline_segment(
                chunk,
                remote_segment,
                stable_speaker,
            )));
        }
        merge_boundary_segments(&mut all_segments, incoming);
        if processing_mode == CaptionProcessingMode::OriginalOnly {
            for segment in &mut all_segments {
                if segment.translation_status == SegmentStatus::Pending {
                    segment.translation_status = SegmentStatus::Skipped;
                }
            }
        }
        for segment in all_segments
            .iter()
            .filter(|segment| mark_source_revision(&mut emitted_sources, segment))
        {
            send(
                events,
                SessionEvent::TranscriptFinalized {
                    segment: segment.clone(),
                },
            )?;
        }

        if chunk.index == 0 {
            references = build_speaker_references(&audio, chunk, &all_segments)?;
        }
        send(
            events,
            SessionEvent::PhaseChanged {
                phase: "buffering".into(),
            },
        )?;
        if processing_mode == CaptionProcessingMode::Translated {
            translate_pending(&client, &mut all_segments, &languages, events, &cancelled).await?;
        } else {
            let ready_through_ms = all_segments.last().map_or(0, |segment| segment.end_ms);
            send(
                events,
                SessionEvent::CoverageChanged { ready_through_ms },
            )?;
            if ready_through_ms >= 15_000 {
                send(
                    events,
                    SessionEvent::PhaseChanged {
                        phase: "ready".into(),
                    },
                )?;
            }
        }
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
    let first = client
        .transcribe_chunk(path, references, source_language)
        .await;
    match first {
        Err(error) if error.retryable => {
            send(
                events,
                SessionEvent::RecoverableError {
                    error: RecoverableError {
                        code: format!("{:?}", error.kind).to_ascii_lowercase(),
                        message: error.message,
                        segment_id: None,
                    },
                },
            )?;
            client
                .transcribe_chunk(path, references, source_language)
                .await
        }
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
        .filter(|(_, segment)| segment.translation_status == SegmentStatus::Pending)
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
        let batch = indices
            .iter()
            .map(|index| translation_input(&segments[*index]))
            .collect::<Vec<_>>();
        let translations =
            match translate_batch_with_retry(client, &context, &batch, languages).await {
                Ok(output) => output,
                Err(error) if translation_error_is_session_fatal(&error) => return Err(error),
                Err(error) => {
                    mark_translation_batch_failed(segments, indices, &error, events)?;
                    publish_translation_coverage(segments, events)?;
                    continue;
                }
            };
        for (index, translation) in indices.iter().zip(translations) {
            debug_assert_eq!(segments[*index].id, translation.segment_id);
            let segment = &mut segments[*index];
            segment.translation_text = Some(translation.translation.clone());
            segment.ambiguity_note = translation.ambiguity_note.clone();
            segment.translation_status = SegmentStatus::Complete;
            send(
                events,
                SessionEvent::TranslationFinalized {
                    segment_id: segment.id.clone(),
                    translation_text: translation.translation,
                    ambiguity_note: translation.ambiguity_note,
                },
            )?;
        }
        publish_translation_coverage(segments, events)?;
    }
    Ok(())
}

pub(crate) async fn translate_batch_with_retry(
    client: &OpenAiClient,
    context: &[TranslationInput],
    batch: &[TranslationInput],
    languages: &LanguageSettings,
) -> Result<Vec<crate::openai::TranslationOutput>, ApiError> {
    match client.translate(context, batch, languages).await {
        Err(error) if error.retryable => client.translate(context, batch, languages).await,
        result => result,
    }
}

pub(crate) fn translation_error_is_session_fatal(error: &ApiError) -> bool {
    matches!(
        error.kind,
        ApiErrorKind::Authentication | ApiErrorKind::ModelUnavailable | ApiErrorKind::Cancelled
    )
}

fn mark_translation_batch_failed(
    segments: &mut [SubtitleSegment],
    indices: &[usize],
    error: &ApiError,
    events: &EventSink,
) -> Result<(), ApiError> {
    for index in indices {
        let segment = &mut segments[*index];
        segment.translation_text = None;
        segment.ambiguity_note = None;
        segment.translation_status = SegmentStatus::Failed;
        send(
            events,
            SessionEvent::TranscriptFinalized {
                segment: segment.clone(),
            },
        )?;
        send(
            events,
            SessionEvent::RecoverableError {
                error: RecoverableError {
                    code: "translation_failed".into(),
                    message: error.message.clone(),
                    segment_id: Some(segment.id.clone()),
                },
            },
        )?;
    }
    Ok(())
}

fn translation_ready_through_ms(segments: &[SubtitleSegment]) -> u64 {
    segments
        .iter()
        .take_while(|segment| segment.translation_status != SegmentStatus::Pending)
        .map(|segment| segment.end_ms)
        .max()
        .unwrap_or(0)
}

fn publish_translation_coverage(
    segments: &[SubtitleSegment],
    events: &EventSink,
) -> Result<(), ApiError> {
    let ready_through_ms = translation_ready_through_ms(segments);
    send(events, SessionEvent::CoverageChanged { ready_through_ms })?;
    if ready_through_ms >= 15_000 {
        send(
            events,
            SessionEvent::PhaseChanged {
                phase: "ready".into(),
            },
        )?;
    }
    Ok(())
}

fn translation_input(segment: &SubtitleSegment) -> TranslationInput {
    TranslationInput {
        segment_id: segment.id.clone(),
        speaker: segment
            .speaker_id
            .clone()
            .unwrap_or_else(|| "speaker-unknown".into()),
        source_text: segment.source_text.clone(),
    }
}

fn stable_speaker_id(
    map: &mut HashMap<String, String>,
    remote: &str,
    allow_new: bool,
    fallback: Option<&str>,
) -> Option<String> {
    if let Some(existing) = map.get(remote) {
        return Some(existing.clone());
    }
    if remote.starts_with("speaker-") && map.values().any(|stable| stable == remote) {
        map.insert(remote.to_owned(), remote.to_owned());
        return Some(remote.to_owned());
    }
    if !allow_new {
        return fallback.map(str::to_owned);
    }
    let stable_count = map.values().collect::<HashSet<_>>().len();
    let id = format!("speaker-{}", stable_count + 1);
    map.insert(remote.to_owned(), id.clone());
    Some(id)
}

fn has_speech_content(text: &str) -> bool {
    text.chars().any(char::is_alphanumeric)
}

fn to_pipeline_segment(
    chunk: &AudioChunk,
    remote: DiarizedSegment,
    speaker_id: Option<String>,
) -> SubtitleSegment {
    let id = if remote.id.is_empty() {
        format!(
            "chunk-{}-{:.0}",
            chunk.index,
            remote.start_seconds * 1_000.0
        )
    } else {
        format!("chunk-{}-{}", chunk.index, remote.id)
    };
    SubtitleSegment {
        id,
        origin: SessionMode::File,
        start_ms: normalize_chunk_timestamp(chunk, remote.start_seconds),
        end_ms: normalize_chunk_timestamp(chunk, remote.end_seconds),
        source_text: remote.text,
        translation_text: None,
        ambiguity_note: None,
        speaker_id,
        is_provisional: false,
        transcription_status: SegmentStatus::Complete,
        translation_status: SegmentStatus::Pending,
    }
}

const MAX_SUBTITLE_DURATION_MS: u64 = 6_500;
const MAX_SUBTITLE_UNITS: usize = 64;

fn split_long_segment(segment: SubtitleSegment) -> Vec<SubtitleSegment> {
    let duration = segment.end_ms.saturating_sub(segment.start_ms);
    let units = display_units(&segment.source_text);
    if duration <= MAX_SUBTITLE_DURATION_MS && units <= MAX_SUBTITLE_UNITS {
        return vec![segment];
    }

    let duration_parts = duration.div_ceil(MAX_SUBTITLE_DURATION_MS).max(1) as usize;
    let target_units = MAX_SUBTITLE_UNITS
        .min(units.div_ceil(duration_parts))
        .max(16);
    let parts = split_text_for_subtitles(&segment.source_text, target_units);
    if parts.len() <= 1 {
        return vec![segment];
    }

    let total_weight = parts
        .iter()
        .map(|part| display_units(part).max(1))
        .sum::<usize>() as u64;
    let mut consumed_weight = 0_u64;
    parts
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let mut part = segment.clone();
            let start_weight = consumed_weight;
            consumed_weight += display_units(&text).max(1) as u64;
            part.id = format!("{}-part-{}", segment.id, index + 1);
            part.start_ms = segment.start_ms + duration * start_weight / total_weight;
            part.end_ms = if consumed_weight == total_weight {
                segment.end_ms
            } else {
                segment.start_ms + duration * consumed_weight / total_weight
            };
            part.source_text = text;
            part
        })
        .collect()
}

fn split_text_for_subtitles(text: &str, target_units: usize) -> Vec<String> {
    let mut remaining = text.trim();
    let mut parts = Vec::new();
    while display_units(remaining) > target_units {
        let mut units = 0;
        let mut current_end = 0;
        let mut terminal_boundary = None;
        let mut clause_boundary = None;
        for (index, character) in remaining.char_indices() {
            units += character_units(character);
            current_end = index + character.len_utf8();
            if units >= target_units / 2 {
                if is_terminal_boundary(character) {
                    terminal_boundary = Some(current_end);
                } else if is_clause_boundary(character) {
                    clause_boundary = Some(current_end);
                }
            }
            if units >= target_units {
                break;
            }
        }
        let boundary = terminal_boundary.or(clause_boundary).unwrap_or(current_end);
        let part = remaining[..boundary].trim();
        if part.is_empty() || boundary == 0 {
            break;
        }
        parts.push(part.to_owned());
        remaining = remaining[boundary..].trim_start();
    }
    if !remaining.is_empty() {
        parts.push(remaining.to_owned());
    }
    parts
}

fn display_units(text: &str) -> usize {
    text.chars().map(character_units).sum()
}

fn character_units(character: char) -> usize {
    if character.is_whitespace() {
        0
    } else if matches!(character as u32, 0x3000..=0x30ff | 0x3400..=0x9fff | 0xf900..=0xfaff | 0xff01..=0xffee)
    {
        2
    } else {
        1
    }
}

fn is_terminal_boundary(character: char) -> bool {
    matches!(character, '。' | '！' | '？' | '!' | '?' | '.' | '…')
}

fn is_clause_boundary(character: char) -> bool {
    character.is_whitespace() || matches!(character, '、' | '，' | ',' | ';' | '；' | ':' | '：')
}

pub fn merge_boundary_segments(
    existing: &mut Vec<SubtitleSegment>,
    mut incoming: Vec<SubtitleSegment>,
) {
    let existing_count = existing.len();
    let mut matched_existing = HashSet::new();
    incoming.sort_by(|left, right| {
        boundary_text_completeness(&right.source_text)
            .cmp(&boundary_text_completeness(&left.source_text))
            .then_with(|| left.start_ms.cmp(&right.start_ms))
            .then_with(|| left.end_ms.cmp(&right.end_ms))
            .then_with(|| left.id.cmp(&right.id))
    });
    for mut candidate in incoming {
        let duplicate = (0..existing_count)
            .filter(|index| !matched_existing.contains(index))
            .filter_map(|index| {
                boundary_match_score(&existing[index], &candidate).map(|score| (index, score))
            })
            .max_by(|(left_index, left), (right_index, right)| {
                left.coverage_per_mille
                    .cmp(&right.coverage_per_mille)
                    .then_with(|| left.overlap_ms.cmp(&right.overlap_ms))
                    .then_with(|| right.edge_distance_ms.cmp(&left.edge_distance_ms))
                    .then_with(|| right_index.cmp(left_index))
            })
            .map(|(index, _)| index);
        let duplicates_consumed_match = duplicate.is_none()
            && matched_existing.iter().any(|index| {
                boundary_match_score(&existing[*index], &candidate).is_some()
            });
        if let Some(index) = duplicate {
            matched_existing.insert(index);
            if boundary_text_completeness(&candidate.source_text)
                > boundary_text_completeness(&existing[index].source_text)
            {
                candidate.id.clone_from(&existing[index].id);
                if candidate.speaker_id.is_none() {
                    candidate.speaker_id.clone_from(&existing[index].speaker_id);
                }
                candidate.translation_text = None;
                candidate.ambiguity_note = None;
                candidate.translation_status = SegmentStatus::Pending;
                existing[index] = candidate;
            }
        } else if !duplicates_consumed_match {
            existing.push(candidate);
        }
    }
    existing.sort_by(|left, right| {
        left.start_ms
            .cmp(&right.start_ms)
            .then_with(|| left.end_ms.cmp(&right.end_ms))
            .then_with(|| left.id.cmp(&right.id))
    });
}

#[derive(Debug, Clone, Copy)]
struct BoundaryMatchScore {
    coverage_per_mille: u64,
    overlap_ms: u64,
    edge_distance_ms: u64,
}

fn boundary_match_score(
    existing: &SubtitleSegment,
    candidate: &SubtitleSegment,
) -> Option<BoundaryMatchScore> {
    if existing.speaker_id.is_some()
        && candidate.speaker_id.is_some()
        && existing.speaker_id != candidate.speaker_id
    {
        return None;
    }
    let existing_text = normalize_boundary_text(&existing.source_text);
    let candidate_text = normalize_boundary_text(&candidate.source_text);
    if existing_text.is_empty()
        || candidate_text.is_empty()
        || (!existing_text.contains(&candidate_text) && !candidate_text.contains(&existing_text))
    {
        return None;
    }

    let overlap_start = existing.start_ms.max(candidate.start_ms);
    let overlap_end = existing.end_ms.min(candidate.end_ms);
    let overlap_ms = overlap_end.saturating_sub(overlap_start);
    let shorter = existing
        .end_ms
        .saturating_sub(existing.start_ms)
        .min(candidate.end_ms.saturating_sub(candidate.start_ms))
        .max(1);
    let coverage_per_mille = ((overlap_ms as u128 * 1_000) / shorter as u128) as u64;
    if coverage_per_mille < 600 {
        return None;
    }

    Some(BoundaryMatchScore {
        coverage_per_mille,
        overlap_ms,
        edge_distance_ms: existing
            .start_ms
            .abs_diff(candidate.start_ms)
            .saturating_add(existing.end_ms.abs_diff(candidate.end_ms)),
    })
}

fn normalize_boundary_text(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn boundary_text_completeness(text: &str) -> usize {
    normalize_boundary_text(text).chars().count()
}

fn mark_source_revision(
    emitted_sources: &mut HashMap<String, String>,
    segment: &SubtitleSegment,
) -> bool {
    if emitted_sources.get(&segment.id) == Some(&segment.source_text) {
        return false;
    }
    emitted_sources.insert(segment.id.clone(), segment.source_text.clone());
    true
}

fn build_speaker_references(
    audio: &DecodedAudio,
    chunk: &AudioChunk,
    segments: &[SubtitleSegment],
) -> Result<Vec<SpeakerReference>, ApiError> {
    let mut seen = HashSet::new();
    let mut references = Vec::new();
    let directory = chunk
        .path
        .parent()
        .ok_or_else(|| service_error("Temporary chunk directory is missing."))?;
    for segment in segments {
        let duration = segment.end_ms.saturating_sub(segment.start_ms);
        let Some(speaker_id) = segment.speaker_id.as_ref() else {
            continue;
        };
        if !(2_000..=10_000).contains(&duration) || !seen.insert(speaker_id.clone()) {
            continue;
        }
        let start = ((segment.start_ms as u128 * audio.sample_rate as u128) / 1_000) as usize;
        let end = ((segment.end_ms as u128 * audio.sample_rate as u128) / 1_000) as usize;
        if start >= end || end > audio.samples.len() {
            continue;
        }
        let path = directory.join(format!("reference-{}.wav", references.len() + 1));
        write_wav(&path, &audio.samples[start..end], audio.sample_rate)
            .map_err(|message| service_error(&message))?;
        let bytes = std::fs::read(&path).map_err(|error| {
            service_error(&format!("Could not read speaker reference: {error}"))
        })?;
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
        Err(ApiError {
            kind: ApiErrorKind::Cancelled,
            message: "Analysis was cancelled.".into(),
            retryable: false,
        })
    } else {
        Ok(())
    }
}

fn service_error(message: &str) -> ApiError {
    ApiError {
        kind: ApiErrorKind::Service,
        message: message.to_owned(),
        retryable: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(id: &str, start: u64, end: u64, text: &str) -> SubtitleSegment {
        SubtitleSegment {
            id: id.into(),
            origin: SessionMode::File,
            start_ms: start,
            end_ms: end,
            source_text: text.into(),
            translation_text: None,
            ambiguity_note: None,
            speaker_id: Some("speaker-1".into()),
            is_provisional: false,
            transcription_status: SegmentStatus::Complete,
            translation_status: SegmentStatus::Pending,
        }
    }

    #[test]
    fn overlapping_boundary_prefers_more_complete_text() {
        let mut existing = vec![segment("old", 10_000, 12_000, "今日は")];
        merge_boundary_segments(
            &mut existing,
            vec![segment("new", 10_050, 12_100, "今日はちょっと")],
        );
        assert_eq!(existing.len(), 1);
        assert_eq!(existing[0].id, "old");
        assert_eq!(existing[0].source_text, "今日はちょっと");
    }

    #[test]
    fn revised_boundary_text_invalidates_only_its_stale_translation() {
        let mut old = segment("stable", 10_000, 12_000, "今日は");
        old.translation_text = Some("As for today".into());
        old.ambiguity_note = Some("Incomplete phrase".into());
        old.translation_status = SegmentStatus::Complete;
        let mut candidate = segment("chunk-2-new", 10_050, 12_100, "今日はちょっと");
        candidate.translation_text = Some("stale candidate translation".into());
        candidate.translation_status = SegmentStatus::Complete;
        let mut existing = vec![old];

        merge_boundary_segments(&mut existing, vec![candidate]);

        assert_eq!(existing[0].id, "stable");
        assert_eq!(existing[0].source_text, "今日はちょっと");
        assert_eq!(existing[0].translation_text, None);
        assert_eq!(existing[0].ambiguity_note, None);
        assert_eq!(existing[0].translation_status, SegmentStatus::Pending);
    }

    #[test]
    fn failed_translation_is_display_ready_but_pending_translation_still_blocks_coverage() {
        let mut first = segment("first", 0, 5_000, "一");
        first.translation_status = SegmentStatus::Complete;
        let mut failed = segment("failed", 5_000, 10_000, "二");
        failed.translation_status = SegmentStatus::Failed;
        let pending = segment("pending", 10_000, 15_000, "三");
        let mut later = segment("later", 15_000, 20_000, "四");
        later.translation_status = SegmentStatus::Complete;

        assert_eq!(
            translation_ready_through_ms(&[first, failed, pending, later]),
            10_000
        );
    }

    #[test]
    fn terminal_batch_failure_marks_every_line_and_emits_addressable_errors() {
        let captured = Arc::new(std::sync::Mutex::new(Vec::<SessionEvent>::new()));
        let sink_events = Arc::clone(&captured);
        let sink: EventSink = Arc::new(move |event| {
            sink_events.lock().unwrap().push(event);
            Ok(())
        });
        let mut segments = vec![
            segment("first", 0, 2_000, "一"),
            segment("second", 2_000, 4_000, "二"),
        ];
        let error = ApiError {
            kind: ApiErrorKind::Refused,
            message: "Translation unavailable.".into(),
            retryable: false,
        };

        mark_translation_batch_failed(&mut segments, &[0, 1], &error, &sink).unwrap();

        assert!(segments
            .iter()
            .all(|segment| segment.translation_status == SegmentStatus::Failed));
        let events = captured.lock().unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, SessionEvent::TranscriptFinalized { .. }))
                .count(),
            2
        );
        let error_ids = events
            .iter()
            .filter_map(|event| match event {
                SessionEvent::RecoverableError { error } => error.segment_id.as_deref(),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(error_ids, vec!["first", "second"]);
    }

    #[test]
    fn exact_boundary_duplicate_keeps_the_existing_translation_and_timing() {
        let mut old = segment("stable", 10_000, 12_000, "I  understand.");
        old.translation_text = Some("分かります。".into());
        old.translation_status = SegmentStatus::Complete;
        let mut existing = vec![old.clone()];

        merge_boundary_segments(
            &mut existing,
            vec![segment("chunk-2-new", 10_100, 12_200, "i understand.")],
        );

        assert_eq!(existing, vec![old]);
    }

    #[test]
    fn incoming_boundary_matches_are_one_to_one() {
        let mut existing = vec![
            segment("first", 10_000, 12_000, "はい"),
            segment("second", 11_000, 13_000, "はい"),
        ];
        merge_boundary_segments(
            &mut existing,
            vec![
                segment("new-a", 10_500, 12_500, "はい、そうです"),
                segment("new-b", 10_500, 12_500, "はい、そうです"),
            ],
        );

        assert_eq!(existing.len(), 2);
        assert_eq!(
            existing
                .iter()
                .map(|segment| segment.id.as_str())
                .collect::<HashSet<_>>(),
            HashSet::from(["first", "second"])
        );
        assert!(existing
            .iter()
            .all(|segment| segment.source_text == "はい、そうです"));
    }

    #[test]
    fn incoming_segments_are_not_deduplicated_against_the_same_chunk() {
        let mut existing = Vec::new();
        merge_boundary_segments(
            &mut existing,
            vec![
                segment("new-a", 10_000, 12_000, "同じです"),
                segment("new-b", 10_100, 11_900, "同じです"),
            ],
        );

        assert_eq!(existing.len(), 2);
        assert_eq!(existing[0].id, "new-a");
        assert_eq!(existing[1].id, "new-b");
    }

    #[test]
    fn fuller_duplicate_wins_even_when_a_shorter_variant_arrives_first() {
        let mut existing = vec![segment("stable", 10_000, 12_000, "今日は")];
        merge_boundary_segments(
            &mut existing,
            vec![
                segment("shorter", 10_050, 12_050, "今日"),
                segment("fuller", 10_050, 12_100, "今日はちょっと"),
            ],
        );

        assert_eq!(existing.len(), 1);
        assert_eq!(existing[0].id, "stable");
        assert_eq!(existing[0].source_text, "今日はちょっと");
    }

    #[test]
    fn same_words_from_a_different_speaker_remain_distinct() {
        let mut existing = vec![segment("speaker-one", 10_000, 12_000, "待って")];
        let mut second_speaker = segment("speaker-two", 10_100, 11_900, "待って");
        second_speaker.speaker_id = Some("speaker-2".into());

        merge_boundary_segments(&mut existing, vec![second_speaker]);

        assert_eq!(existing.len(), 2);
        assert_eq!(
            existing
                .iter()
                .filter_map(|segment| segment.speaker_id.as_deref())
                .collect::<HashSet<_>>(),
            HashSet::from(["speaker-1", "speaker-2"])
        );
    }

    #[test]
    fn stable_id_source_revision_is_emitted_once_per_text_version() {
        let mut emitted = HashMap::new();
        let first = segment("stable", 10_000, 12_000, "今日は");
        let revised = segment("stable", 10_000, 12_100, "今日はちょっと");

        assert!(mark_source_revision(&mut emitted, &first));
        assert!(!mark_source_revision(&mut emitted, &first));
        assert!(mark_source_revision(&mut emitted, &revised));
        assert!(!mark_source_revision(&mut emitted, &revised));
        assert_eq!(emitted.len(), 1);
    }

    #[test]
    fn split_boundary_parts_keep_their_individual_stable_ids() {
        let mut existing = vec![
            segment("old-part-1", 10_000, 12_000, "今日はちょっと"),
            segment("old-part-2", 12_000, 14_000, "予定があります"),
        ];
        merge_boundary_segments(
            &mut existing,
            vec![
                segment("new-part-1", 10_050, 12_100, "今日はちょっとだけ"),
                segment("new-part-2", 12_050, 14_100, "予定がありますので"),
            ],
        );

        assert_eq!(existing[0].id, "old-part-1");
        assert_eq!(existing[0].source_text, "今日はちょっとだけ");
        assert_eq!(existing[1].id, "old-part-2");
        assert_eq!(existing[1].source_text, "予定がありますので");
    }

    #[test]
    fn distinct_overlap_is_preserved_for_stacked_subtitles() {
        let mut existing = vec![segment("a", 10_000, 12_000, "こんにちは")];
        merge_boundary_segments(&mut existing, vec![segment("b", 10_100, 11_900, "待って")]);
        assert_eq!(existing.len(), 2);
    }

    #[test]
    fn paragraph_sized_japanese_turn_becomes_readable_timed_subtitles() {
        let original = segment(
            "long",
            2_000,
            20_000,
            "最近は新しいことを学んだり、自分の好きなことに時間を使ったりしています。忙しい日もありますが、少しずつ前に進むことを大切にしています。これからもいろいろな人と話したり、新しい経験をしたりしながら、楽しく成長していきたいです。",
        );
        let parts = split_long_segment(original.clone());
        assert!(parts.len() >= 3);
        assert_eq!(parts.first().unwrap().start_ms, original.start_ms);
        assert_eq!(parts.last().unwrap().end_ms, original.end_ms);
        assert!(parts
            .windows(2)
            .all(|pair| pair[0].end_ms == pair[1].start_ms));
        assert!(parts
            .iter()
            .all(|part| display_units(&part.source_text) <= MAX_SUBTITLE_UNITS));
        assert_eq!(
            parts
                .iter()
                .map(|part| part.source_text.as_str())
                .collect::<String>(),
            original.source_text
        );
    }

    #[test]
    fn short_turn_is_not_split() {
        let original = segment("short", 1_000, 3_500, "何ですか？");
        assert_eq!(split_long_segment(original.clone()), vec![original]);
    }

    #[test]
    fn punctuation_only_diarization_tail_is_not_a_speaker() {
        assert!(!has_speech_content(" 。…… "));
        assert!(has_speech_content("え……"));
        assert!(has_speech_content("Wait—"));
    }

    #[test]
    fn later_unmatched_speaker_inherits_the_adjacent_known_voice() {
        let mut speakers = HashMap::new();
        assert_eq!(
            stable_speaker_id(&mut speakers, "A", true, None),
            Some("speaker-1".into())
        );
        assert_eq!(
            stable_speaker_id(&mut speakers, "B", true, Some("speaker-1")),
            Some("speaker-2".into())
        );
        assert_eq!(
            stable_speaker_id(&mut speakers, "C", false, Some("speaker-2")),
            Some("speaker-2".into())
        );
    }

    #[test]
    fn known_speaker_aliases_do_not_advance_numbering() {
        let mut speakers = HashMap::new();
        stable_speaker_id(&mut speakers, "A", true, None);
        stable_speaker_id(&mut speakers, "B", true, Some("speaker-1"));
        assert_eq!(
            stable_speaker_id(&mut speakers, "speaker-1", false, Some("speaker-2")),
            Some("speaker-1".into())
        );
        assert_eq!(
            stable_speaker_id(&mut speakers, "C", true, Some("speaker-1")),
            Some("speaker-3".into())
        );
    }

    #[tokio::test]
    async fn cancellation_finishes_without_emitting_a_fatal_error() {
        let captured = Arc::new(std::sync::Mutex::new(Vec::<SessionEvent>::new()));
        let sink_events = Arc::clone(&captured);
        let sink: EventSink = Arc::new(move |event| {
            sink_events.lock().unwrap().push(event);
            Ok(())
        });
        let result = run(
            OpenAiClient::new("sk-test-generation-isolation".into()).unwrap(),
            Arc::new(DecodedAudio {
                samples: vec![0; 16_000],
                sample_rate: 16_000,
            }),
            vec![AudioChunk {
                index: 0,
                start_sample: 0,
                end_sample: 16_000,
                timeline_start_ms: 0,
                overlapped: false,
                path: std::path::PathBuf::from("never-read-after-cancel.wav"),
            }],
            LanguageSettings {
                source: "ja".into(),
                target: "en".into(),
                explanation: "en".into(),
            },
            CaptionProcessingMode::Translated,
            Arc::new(AtomicBool::new(true)),
            sink,
        )
        .await;

        assert!(result.is_ok());
        assert!(!captured
            .lock()
            .unwrap()
            .iter()
            .any(|event| matches!(event, SessionEvent::FatalError { .. })));
    }
}
