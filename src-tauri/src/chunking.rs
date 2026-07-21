use std::{
    ops::Range,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::media::{write_wav, DecodedAudio};

const FIRST_TARGET_SECONDS: usize = 30;
const NEXT_TARGET_SECONDS: usize = 120;
const SEARCH_SECONDS: usize = 5;
const OVERLAP_MILLISECONDS: usize = 1_500;
const RMS_WINDOW_MILLISECONDS: usize = 200;
const QUIET_RMS_THRESHOLD: u128 = 1_800;

#[derive(Debug, Clone, PartialEq)]
pub struct AudioChunk {
    pub index: usize,
    pub start_sample: usize,
    pub end_sample: usize,
    pub timeline_start_ms: u64,
    pub overlapped: bool,
    pub path: PathBuf,
}

pub fn select_quiet_boundary(
    samples: &[i16],
    sample_rate: u32,
    target: usize,
    search_radius: usize,
) -> Option<usize> {
    if samples.is_empty() || target >= samples.len() {
        return None;
    }
    let window = (sample_rate as usize * RMS_WINDOW_MILLISECONDS / 1_000).max(1);
    let start = target.saturating_sub(search_radius).max(window);
    let end = (target + search_radius).min(samples.len().saturating_sub(window));
    if start >= end {
        return None;
    }
    let quietest = (start..=end)
        .step_by((window / 2).max(1))
        .min_by_key(|candidate| {
            let range = candidate.saturating_sub(window)..(*candidate + window).min(samples.len());
            mean_square(samples, range)
        })?;
    let range = quietest.saturating_sub(window)..(quietest + window).min(samples.len());
    (mean_square(samples, range) <= QUIET_RMS_THRESHOLD * QUIET_RMS_THRESHOLD).then_some(quietest)
}

fn mean_square(samples: &[i16], range: Range<usize>) -> u128 {
    let slice = &samples[range];
    if slice.is_empty() {
        return u128::MAX;
    }
    slice
        .iter()
        .map(|sample| {
            let value = *sample as i64;
            (value * value) as u128
        })
        .sum::<u128>()
        / slice.len() as u128
}

#[cfg(test)]
pub fn create_chunks(
    audio: &DecodedAudio,
    directory: &std::path::Path,
) -> Result<Vec<AudioChunk>, String> {
    create_chunks_cancellable(audio, directory, &AtomicBool::new(false))
}

pub fn create_chunks_cancellable(
    audio: &DecodedAudio,
    directory: &std::path::Path,
    cancelled: &AtomicBool,
) -> Result<Vec<AudioChunk>, String> {
    let rate = audio.sample_rate as usize;
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < audio.samples.len() {
        if cancelled.load(Ordering::Relaxed) {
            return Err("Media preparation was cancelled.".into());
        }
        let target_seconds = if chunks.is_empty() {
            FIRST_TARGET_SECONDS
        } else {
            NEXT_TARGET_SECONDS
        };
        let target = start.saturating_add(target_seconds * rate);
        let (end, overlapped) = if target >= audio.samples.len() {
            (audio.samples.len(), false)
        } else if let Some(boundary) = select_quiet_boundary(
            &audio.samples,
            audio.sample_rate,
            target,
            SEARCH_SECONDS * rate,
        ) {
            (boundary, false)
        } else {
            (target.min(audio.samples.len()), true)
        };
        if end <= start {
            return Err("Audio chunk scheduling did not advance.".into());
        }
        let index = chunks.len();
        let path = directory.join(format!("chunk-{index:03}.wav"));
        write_wav(&path, &audio.samples[start..end], audio.sample_rate)?;
        chunks.push(AudioChunk {
            index,
            start_sample: start,
            end_sample: end,
            timeline_start_ms: (start as u64 * 1_000) / audio.sample_rate as u64,
            overlapped,
            path,
        });
        if end == audio.samples.len() {
            break;
        }
        start = if overlapped {
            end.saturating_sub(rate * OVERLAP_MILLISECONDS / 1_000)
        } else {
            end
        };
    }
    Ok(chunks)
}

pub fn normalize_chunk_timestamp(chunk: &AudioChunk, seconds: f64) -> u64 {
    chunk.timeline_start_ms + (seconds.max(0.0) * 1_000.0).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quietest_boundary_wins_within_search_window() {
        let mut samples = vec![10_000i16; 2_000];
        samples[750..=1_250].fill(0);
        let boundary = select_quiet_boundary(&samples, 1_000, 1_000, 300).unwrap();
        assert!((900..=1_100).contains(&boundary));
    }

    #[test]
    fn global_timestamp_includes_chunk_offset() {
        let chunk = AudioChunk {
            index: 2,
            start_sample: 0,
            end_sample: 1,
            timeline_start_ms: 150_000,
            overlapped: false,
            path: PathBuf::new(),
        };
        assert_eq!(normalize_chunk_timestamp(&chunk, 2.25), 152_250);
    }

    #[test]
    fn sustained_loud_audio_uses_fallback_overlap() {
        let directory = tempfile::tempdir().unwrap();
        let audio = DecodedAudio {
            samples: vec![12_000; 35_000],
            sample_rate: 1_000,
        };
        let chunks = create_chunks(&audio, directory.path()).unwrap();
        assert!(chunks[0].overlapped);
        assert_eq!(chunks[1].start_sample, chunks[0].end_sample - 1_500);
    }
}
