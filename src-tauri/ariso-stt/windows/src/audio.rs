//! Audio normalization for the Windows inference pipeline.
//!
//! Container and codec concerns stop in this module. Downstream ASR and
//! diarization code receives mono `f32` samples and chooses its own sample-rate
//! requirements; this module does not know about model layouts or transcript
//! semantics.

use anyhow::{Context, Result, anyhow, bail};
use std::fs;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

#[derive(Debug)]
/// The codec-independent audio representation shared by ASR and diarization.
/// Channels have already been mixed down so model adapters cannot accidentally
/// disagree about channel ordering or select only one side of a meeting.
pub(crate) struct Audio {
    pub(crate) sample_rate: i32,
    pub(crate) samples: Vec<f32>,
}

/// Keeps derived metadata attached to the normalized representation while
/// leaving transformations in free functions that make allocations explicit.
impl Audio {
    /// Supplies timing metadata without carrying container timestamps into the
    /// inference layer. Invalid rates collapse to zero because callers use this
    /// value for output metadata, not as an input-validation boundary.
    pub(crate) fn duration_seconds(&self) -> f64 {
        if self.sample_rate <= 0 {
            0.0
        } else {
            self.samples.len() as f64 / self.sample_rate as f64
        }
    }
}

/// Establishes the single decoding boundary for every recording format oats
/// accepts on Windows. It deliberately preserves the source sample rate; ASR
/// and diarization may require different rates and own those conversions.
pub(crate) fn decode_audio(path: &Path) -> Result<Audio> {
    let file = fs::File::open(path).with_context(|| format!("open audio {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .with_context(|| format!("probe audio {}", path.display()))?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("no supported audio track in {}", path.display()))?
        .clone();
    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| anyhow!("audio sample rate missing in {}", path.display()))?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .with_context(|| format!("create decoder for {}", path.display()))?;
    let mut samples = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::ResetRequired) => bail!("audio decoder reset required"),
            Err(err) => return Err(err).with_context(|| format!("read packet {}", path.display())),
        };
        if packet.track_id() != track.id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(err) => return Err(err).with_context(|| format!("decode {}", path.display())),
        };
        let spec = *decoded.spec();
        let channels = spec.channels.count().max(1);
        let buf =
            sample_buf.get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, spec));
        buf.copy_interleaved_ref(decoded);
        for frame in buf.samples().chunks(channels) {
            let sum: f32 = frame.iter().copied().sum();
            samples.push(sum / frame.len() as f32);
        }
    }

    if samples.is_empty() {
        bail!("decoded no samples from {}", path.display());
    }

    Ok(Audio {
        sample_rate: sample_rate as i32,
        samples,
    })
}

/// Extracts a model time span from normalized audio. Padding and minimum-span
/// policy stay with diarization, while this helper only guarantees clamped,
/// sample-aligned bounds.
pub(crate) fn slice_audio(audio: &Audio, start: f64, end: f64) -> Vec<f32> {
    let sample_rate = audio.sample_rate.max(1) as f64;
    let start_index = ((start.max(0.0) * sample_rate).floor() as usize).min(audio.samples.len());
    let end_index = ((end.max(start) * sample_rate).ceil() as usize).min(audio.samples.len());
    audio.samples[start_index..end_index].to_vec()
}

/// Provides a dependency-light conversion for model adapters whose required
/// rate differs from the recording. This is intended for inference inputs, not
/// mastering-quality audio or user-visible export.
pub(crate) fn resample_linear(samples: &[f32], from_rate: i32, to_rate: i32) -> Vec<f32> {
    if samples.is_empty() || from_rate <= 0 || to_rate <= 0 || from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = to_rate as f64 / from_rate as f64;
    let out_len = ((samples.len() as f64 * ratio).round() as usize).max(1);
    let mut out = Vec::with_capacity(out_len);
    for index in 0..out_len {
        let source = index as f64 / ratio;
        let left = source.floor() as usize;
        let right = (left + 1).min(samples.len() - 1);
        let fraction = (source - left as f64) as f32;
        out.push(samples[left] * (1.0 - fraction) + samples[right] * fraction);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resamples_and_slices_audio_for_diarized_segments() {
        let audio = Audio {
            sample_rate: 4,
            samples: vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0],
        };
        assert_eq!(slice_audio(&audio, 0.25, 1.25), vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(
            resample_linear(&[0.0, 2.0, 4.0], 3, 6),
            vec![0.0, 1.0, 2.0, 3.0, 4.0, 4.0]
        );
    }
}
