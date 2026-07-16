//! Audio normalization for the Windows inference pipeline.
//!
//! Container and codec concerns stop in this module. Downstream ASR and
//! diarization code receives mono 16 kHz `f32` samples, which bounds recording
//! memory independently of the source device rate. This module does not know
//! about model layouts or transcript semantics.

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

const INFERENCE_SAMPLE_RATE: i32 = 16_000;

#[derive(Debug)]
/// The codec-independent audio representation shared by ASR and diarization.
/// Channels have already been mixed down and normalized to the inference rate
/// so model adapters cannot retain separate full-recording conversions.
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
/// accepts on Windows. Normalizing packet-by-packet avoids retaining the source
/// rate recording alongside the model-ready copy for the entire inference run.
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
    let source_sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| anyhow!("audio sample rate missing in {}", path.display()))?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .with_context(|| format!("create decoder for {}", path.display()))?;
    let mut resampler =
        StreamingLinearResampler::new(source_sample_rate as i32, INFERENCE_SAMPLE_RATE)?;
    let mut sample_buf: Option<SampleBuffer<f32>> = None;
    let mut mono_packet = Vec::new();

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
        mono_packet.clear();
        mono_packet.reserve(buf.samples().len() / channels);
        for frame in buf.samples().chunks(channels) {
            let sum: f32 = frame.iter().copied().sum();
            mono_packet.push(sum / frame.len() as f32);
        }
        resampler.push(&mono_packet);
    }

    let samples = resampler.finish();
    if samples.is_empty() {
        bail!("decoded no samples from {}", path.display());
    }

    Ok(Audio {
        sample_rate: INFERENCE_SAMPLE_RATE,
        samples,
    })
}

/// Carries interpolation state across decoder packet boundaries. Keeping only
/// one prior source sample makes chunked conversion equivalent to a whole-file
/// linear resample without ever retaining the whole source-rate recording.
struct StreamingLinearResampler {
    step: f64,
    target_ratio: f64,
    next_source_position: f64,
    previous: Option<f32>,
    input_samples: usize,
    output: Vec<f32>,
}

impl StreamingLinearResampler {
    fn new(from_rate: i32, to_rate: i32) -> Result<Self> {
        if from_rate <= 0 || to_rate <= 0 {
            bail!("audio sample rates must be positive");
        }
        Ok(Self {
            step: from_rate as f64 / to_rate as f64,
            target_ratio: to_rate as f64 / from_rate as f64,
            next_source_position: 0.0,
            previous: None,
            input_samples: 0,
            output: Vec::new(),
        })
    }

    fn push(&mut self, samples: &[f32]) {
        for &sample in samples {
            let current_index = self.input_samples;
            match self.previous {
                None => {
                    self.output.push(sample);
                    self.next_source_position = self.step;
                }
                Some(previous) => {
                    let previous_index = current_index - 1;
                    while self.next_source_position <= current_index as f64 {
                        let fraction = (self.next_source_position - previous_index as f64) as f32;
                        self.output
                            .push(previous * (1.0 - fraction) + sample * fraction);
                        self.next_source_position += self.step;
                    }
                }
            }
            self.previous = Some(sample);
            self.input_samples += 1;
        }
    }

    fn finish(mut self) -> Vec<f32> {
        if self.input_samples == 0 {
            return self.output;
        }

        // Whole-buffer resampling rounds the target length and clamps positions
        // beyond the final source sample. Mirror that tail policy after the last
        // packet so chunk boundaries cannot change duration.
        let expected = ((self.input_samples as f64 * self.target_ratio).round() as usize).max(1);
        self.output.truncate(expected);
        if let Some(last) = self.previous {
            self.output.resize(expected, last);
        }
        self.output
    }
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

    #[test]
    fn streaming_resample_matches_whole_buffer_across_packet_boundaries() {
        let input = vec![0.0, 2.0, 4.0];
        let mut streaming = StreamingLinearResampler::new(3, 6).unwrap();
        streaming.push(&input[..2]);
        streaming.push(&input[2..]);

        assert_eq!(streaming.finish(), resample_linear(&input, 3, 6));
    }

    #[test]
    fn streaming_downsample_has_the_expected_model_rate_length() {
        let input = (0..441).map(|sample| sample as f32).collect::<Vec<_>>();
        let mut streaming = StreamingLinearResampler::new(44_100, 16_000).unwrap();
        for packet in input.chunks(37) {
            streaming.push(packet);
        }

        let output = streaming.finish();
        assert_eq!(output.len(), 160);
        assert_eq!(output, resample_linear(&input, 44_100, 16_000));
    }
}
