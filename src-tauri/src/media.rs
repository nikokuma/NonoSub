use std::{fs::File, path::Path};

use symphonia::core::{
    audio::sample::Sample,
    codecs::audio::AudioDecoderOptions,
    codecs::video::well_known::CODEC_ID_HEVC,
    errors::Error as SymphoniaError,
    formats::{probe::Hint, FormatOptions, TrackType},
    io::MediaSourceStream,
    meta::MetadataOptions,
};

pub const TARGET_SAMPLE_RATE: u32 = 16_000;
pub const MAX_MEDIA_DURATION_SECONDS: u64 = 4 * 60 * 60;

#[derive(Debug, Clone)]
pub struct DecodedAudio {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
}

#[derive(Debug)]
struct StreamingLinearResampler {
    source_rate: u32,
    target_rate: u32,
    input_len: u64,
    next_output: u64,
    previous: Option<f32>,
    samples: Vec<i16>,
}

impl StreamingLinearResampler {
    fn new(source_rate: u32, target_rate: u32) -> Self {
        Self {
            source_rate,
            target_rate,
            input_len: 0,
            next_output: 0,
            previous: None,
            samples: Vec::new(),
        }
    }

    fn push(&mut self, sample: f32) {
        if self.source_rate == 0 || self.target_rate == 0 {
            return;
        }
        let current_index = self.input_len;
        self.input_len = self.input_len.saturating_add(1);
        if self.source_rate == self.target_rate {
            self.samples.push(pcm16(sample));
            self.previous = Some(sample);
            self.next_output = self.next_output.saturating_add(1);
            return;
        }
        let Some(previous) = self.previous.replace(sample) else {
            if self.next_output == 0 {
                self.samples.push(pcm16(sample));
                self.next_output = 1;
            }
            return;
        };
        while self.next_output.saturating_mul(self.source_rate as u64)
            <= current_index.saturating_mul(self.target_rate as u64)
        {
            let position = self.next_output as f64 * self.source_rate as f64
                / self.target_rate as f64;
            let fraction = (position - (current_index - 1) as f64).clamp(0.0, 1.0) as f32;
            self.samples
                .push(pcm16(previous * (1.0 - fraction) + sample * fraction));
            self.next_output = self.next_output.saturating_add(1);
        }
    }

    fn finish(mut self) -> Vec<i16> {
        let output_len = self
            .input_len
            .saturating_mul(self.target_rate as u64)
            / self.source_rate.max(1) as u64;
        if let Some(last) = self.previous {
            while self.next_output < output_len {
                self.samples.push(pcm16(last));
                self.next_output += 1;
            }
        }
        self.samples
    }
}

fn pcm16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

fn ensure_duration_limit(decoded_frames: u64, sample_rate: u32) -> Result<(), String> {
    if decoded_frames > u64::from(sample_rate).saturating_mul(MAX_MEDIA_DURATION_SECONDS) {
        Err("NonoSub supports local videos up to four hours long.".into())
    } else {
        Ok(())
    }
}

pub fn needs_macos_playback_proxy(path: &Path) -> Result<bool, String> {
    let file = File::open(path).map_err(|error| format!("Could not inspect the video: {error}"))?;
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }
    let format = symphonia::default::get_probe()
        .probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|error| format!("Unsupported or unreadable media container: {error}"))?;
    Ok(format
        .default_track(TrackType::Video)
        .and_then(|track| track.codec_params.as_ref())
        .and_then(|parameters| parameters.video())
        .is_some_and(|parameters| parameters.codec == CODEC_ID_HEVC))
}

pub fn decode_to_mono_16k(path: &Path) -> Result<DecodedAudio, String> {
    let file = File::open(path).map_err(|error| format!("Could not open the video: {error}"))?;
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|error| format!("Unsupported or unreadable media container: {error}"))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| "The selected video has no decodable audio track.".to_string())?;
    let track_id = track.id;
    let audio_parameters = track
        .codec_params
        .as_ref()
        .and_then(|parameters| parameters.audio())
        .ok_or_else(|| "The selected track is not audio.".to_string())?;
    let source_rate = audio_parameters
        .sample_rate
        .ok_or_else(|| "The audio track does not declare a sample rate.".to_string())?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(audio_parameters, &AudioDecoderOptions::default())
        .map_err(|error| {
            format!("Unsupported audio codec (AAC is required for the MVP): {error}")
        })?;
    let mut resampler = StreamingLinearResampler::new(source_rate, TARGET_SAMPLE_RATE);
    let mut decoded_frames = 0_u64;

    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(SymphoniaError::ResetRequired) => {
                return Err("The audio stream changed format partway through the file.".into())
            }
            Err(error) => return Err(format!("Could not read the audio stream: {error}")),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(error) => return Err(format!("Could not decode AAC audio: {error}")),
        };
        let channels = decoded.spec().channels().count();
        let mut interleaved = vec![f32::MID; decoded.samples_interleaved()];
        decoded.copy_to_slice_interleaved(&mut interleaved);
        for frame in interleaved.chunks_exact(channels) {
            decoded_frames = decoded_frames.saturating_add(1);
            ensure_duration_limit(decoded_frames, source_rate)?;
            resampler.push(frame.iter().sum::<f32>() / channels as f32);
        }
    }

    if decoded_frames == 0 {
        return Err("No audio samples could be decoded from this file.".into());
    }
    let samples = resampler.finish();
    Ok(DecodedAudio {
        samples,
        sample_rate: TARGET_SAMPLE_RATE,
    })
}

#[cfg(test)]
pub fn resample_linear(input: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if input.is_empty() || source_rate == 0 || target_rate == 0 {
        return Vec::new();
    }
    if source_rate == target_rate {
        return input.to_vec();
    }
    let output_len = ((input.len() as u64 * target_rate as u64) / source_rate as u64) as usize;
    let ratio = source_rate as f64 / target_rate as f64;
    (0..output_len)
        .map(|index| {
            let source_position = index as f64 * ratio;
            let lower = source_position.floor() as usize;
            let upper = (lower + 1).min(input.len() - 1);
            let fraction = (source_position - lower as f64) as f32;
            input[lower] * (1.0 - fraction) + input[upper] * fraction
        })
        .collect()
}

pub fn write_wav(path: &Path, samples: &[i16], sample_rate: u32) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .map_err(|error| format!("Could not create a temporary audio chunk: {error}"))?;
    for sample in samples {
        writer
            .write_sample(*sample)
            .map_err(|error| format!("Could not write a temporary audio chunk: {error}"))?;
    }
    writer
        .finalize()
        .map_err(|error| format!("Could not finalize a temporary audio chunk: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampler_preserves_duration_and_endpoints() {
        let source = vec![0.0, 0.5, 1.0, 0.5];
        let output = resample_linear(&source, 4, 8);
        assert_eq!(output.len(), 8);
        assert!((output[2] - 0.5).abs() < 0.001);
        assert_eq!(*output.last().unwrap(), 0.5);
    }

    #[test]
    fn streaming_resampler_matches_batch_output_across_packet_boundaries() {
        let source = [0.0, 0.5, 1.0, 0.25, -0.5, -1.0, 0.75];
        let expected = resample_linear(&source, 7, 11)
            .into_iter()
            .map(pcm16)
            .collect::<Vec<_>>();
        let mut streaming = StreamingLinearResampler::new(7, 11);
        for packet in source.chunks(2) {
            for sample in packet {
                streaming.push(*sample);
            }
        }
        assert_eq!(streaming.finish(), expected);
    }

    #[test]
    fn four_hour_limit_is_inclusive() {
        let maximum = u64::from(48_000_u32) * MAX_MEDIA_DURATION_SECONDS;
        assert!(ensure_duration_limit(maximum, 48_000).is_ok());
        assert_eq!(
            ensure_duration_limit(maximum + 1, 48_000).unwrap_err(),
            "NonoSub supports local videos up to four hours long."
        );
    }

    #[test]
    fn wav_writer_produces_mono_sixteen_bit_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("chunk.wav");
        write_wav(&path, &[0, 120, -120], TARGET_SAMPLE_RATE).unwrap();
        let reader = hound::WavReader::open(path).unwrap();
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().bits_per_sample, 16);
        assert_eq!(reader.duration(), 3);
    }

    #[test]
    fn temporary_audio_directory_is_removed_on_drop() {
        let path = {
            let directory = tempfile::Builder::new()
                .prefix("nonosub-cleanup-test-")
                .tempdir()
                .unwrap();
            let path = directory.path().to_owned();
            write_wav(&path.join("chunk.wav"), &[0, 1, -1], TARGET_SAMPLE_RATE).unwrap();
            path
        };
        assert!(!path.exists());
    }

    #[test]
    fn decodes_external_aac_fixture_when_configured() {
        let Ok(path) = std::env::var("NONOSUB_MEDIA_FIXTURE") else {
            return;
        };
        let audio =
            decode_to_mono_16k(Path::new(&path)).expect("configured AAC fixture should decode");
        assert_eq!(audio.sample_rate, TARGET_SAMPLE_RATE);
        assert!(!audio.samples.is_empty());
    }

    #[test]
    fn inspects_external_video_codec_when_configured() {
        let Ok(path) = std::env::var("NONOSUB_MEDIA_FIXTURE") else {
            return;
        };
        needs_macos_playback_proxy(Path::new(&path))
            .expect("configured video codec should be inspectable");
    }
}
