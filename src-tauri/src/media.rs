use std::{fs::File, path::Path};

use symphonia::core::{
    audio::sample::Sample,
    codecs::audio::AudioDecoderOptions,
    errors::Error as SymphoniaError,
    formats::{probe::Hint, FormatOptions, TrackType},
    io::MediaSourceStream,
    meta::MetadataOptions,
};

pub const TARGET_SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, Clone)]
pub struct DecodedAudio {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
}

pub fn decode_to_mono_16k(path: &Path) -> Result<DecodedAudio, String> {
    let file = File::open(path).map_err(|error| format!("Could not open the video: {error}"))?;
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, source, FormatOptions::default(), MetadataOptions::default())
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
        .map_err(|error| format!("Unsupported audio codec (AAC is required for the MVP): {error}"))?;
    let mut mono = Vec::<f32>::new();

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
            mono.push(frame.iter().sum::<f32>() / channels as f32);
        }
    }

    if mono.is_empty() {
        return Err("No audio samples could be decoded from this file.".into());
    }
    let resampled = resample_linear(&mono, source_rate, TARGET_SAMPLE_RATE);
    let samples = resampled
        .into_iter()
        .map(|sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16)
        .collect();
    Ok(DecodedAudio { samples, sample_rate: TARGET_SAMPLE_RATE })
}

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
            let directory = tempfile::Builder::new().prefix("nonosub-cleanup-test-").tempdir().unwrap();
            let path = directory.path().to_owned();
            write_wav(&path.join("chunk.wav"), &[0, 1, -1], TARGET_SAMPLE_RATE).unwrap();
            path
        };
        assert!(!path.exists());
    }

    #[test]
    fn decodes_external_aac_fixture_when_configured() {
        let Ok(path) = std::env::var("NONOSUB_MEDIA_FIXTURE") else { return };
        let audio = decode_to_mono_16k(Path::new(&path)).expect("configured AAC fixture should decode");
        assert_eq!(audio.sample_rate, TARGET_SAMPLE_RATE);
        assert!(!audio.samples.is_empty());
    }
}
