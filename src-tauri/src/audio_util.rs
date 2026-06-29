//! Shared Core Audio primitives used by both system-audio (`audio_capture`) and
//! microphone (`mic_capture`) capture: streaming resampler, mono downmix, PCM
//! format validation, property reads, and base64 emission.
#![cfg(target_os = "macos")]

use objc2::rc::Retained;
use objc2_core_audio::{
    kAudioObjectPropertyElementMain, AudioObjectGetPropertyData, AudioObjectPropertyAddress,
};
use objc2_core_audio_types::{
    kAudioFormatFlagIsBigEndian, kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked,
    kAudioFormatLinearPCM, kLinearPCMFormatFlagIsNonInterleaved, AudioBufferList,
    AudioStreamBasicDescription,
};
use objc2_foundation::NSString;
use std::ffi::{c_void, CStr};
use std::ptr::NonNull;

pub(crate) type AudioObjectID = u32;

pub(crate) fn ns(c: &CStr) -> Retained<NSString> {
    NSString::from_str(c.to_str().expect("Core Audio key is valid UTF-8"))
}

/// Streaming linear resampler from `src_rate` to `dst_rate`, carrying fractional
/// position and the last input sample across callbacks so block boundaries don't click.
pub(crate) struct Resampler {
    step: f64,
    pos: f64,
    prev: f32,
    primed: bool,
}

impl Resampler {
    pub(crate) fn new(src_rate: f64, dst_rate: f64) -> Self {
        Self { step: src_rate / dst_rate, pos: 0.0, prev: 0.0, primed: false }
    }

    pub(crate) fn process(&mut self, input: &[f32], out: &mut Vec<u8>) {
        if input.is_empty() {
            return;
        }
        if !self.primed {
            // Seed `prev` with the first sample so the leading interpolation
            // doesn't ramp up from silence.
            self.prev = input[0];
            self.primed = true;
        }
        let n = input.len();
        let sample_at = |j: f64| -> f32 {
            let idx = j as isize;
            if idx <= 0 { self.prev } else { input[(idx - 1).min(n as isize - 1) as usize] }
        };
        while self.pos < n as f64 {
            let base = self.pos.floor();
            let frac = (self.pos - base) as f32;
            let a = sample_at(base);
            let b = sample_at(base + 1.0);
            let s = a + (b - a) * frac;
            let clamped = s.clamp(-1.0, 1.0);
            let v: i16 = if clamped < 0.0 { (clamped * 32768.0) as i16 } else { (clamped * 32767.0) as i16 };
            out.extend_from_slice(&v.to_le_bytes());
            self.pos += self.step;
        }
        self.pos -= n as f64;
        self.prev = input[n - 1];
    }
}

pub(crate) fn prop_address(selector: u32, scope: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    }
}

pub(crate) unsafe fn get_property<T>(
    object: AudioObjectID,
    selector: u32,
    scope: u32,
) -> Result<T, String> {
    let addr = prop_address(selector, scope);
    let mut value = std::mem::MaybeUninit::<T>::uninit();
    let mut size = std::mem::size_of::<T>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::new(value.as_mut_ptr() as *mut c_void).unwrap(),
        )
    };
    if status != 0 {
        return Err(format!("AudioObjectGetPropertyData({selector}) failed: {status}"));
    }
    let expected = std::mem::size_of::<T>() as u32;
    if size != expected {
        return Err(format!(
            "AudioObjectGetPropertyData({selector}) returned {size} bytes; expected {expected}"
        ));
    }
    Ok(unsafe { value.assume_init() })
}

/// Whether a stream format is packed, little-endian, interleaved 32-bit float
/// LinearPCM with a positive sample rate — the layout `downmix_to_mono` assumes.
pub(crate) fn is_supported_pcm_format(asbd: &AudioStreamBasicDescription) -> bool {
    asbd.mFormatID == kAudioFormatLinearPCM
        && asbd.mFormatFlags & kAudioFormatFlagIsFloat != 0
        && asbd.mFormatFlags & kAudioFormatFlagIsPacked != 0
        && asbd.mFormatFlags & kAudioFormatFlagIsBigEndian == 0
        && asbd.mFormatFlags & kLinearPCMFormatFlagIsNonInterleaved == 0
        && asbd.mBitsPerChannel == 32
        && asbd.mSampleRate > 0.0
}

/// Average all channels of an interleaved Float32 `AudioBufferList` into mono.
pub(crate) unsafe fn downmix_to_mono(list: *const AudioBufferList) -> Vec<f32> {
    let list = unsafe { &*list };
    let n = list.mNumberBuffers as usize;
    if n == 0 {
        return Vec::new();
    }
    let buffers = list.mBuffers.as_ptr();
    let mut out: Vec<f32> = Vec::new();
    for i in 0..n {
        let buf = unsafe { &*buffers.add(i) };
        if buf.mData.is_null() || buf.mDataByteSize == 0 {
            continue;
        }
        let ch = buf.mNumberChannels.max(1);
        let count = buf.mDataByteSize as usize / std::mem::size_of::<f32>();
        let data = unsafe { std::slice::from_raw_parts(buf.mData as *const f32, count) };
        if ch <= 1 {
            out.extend_from_slice(data);
        } else {
            for frame in data.chunks_exact(ch as usize) {
                let sum: f32 = frame.iter().copied().sum();
                out.push(sum / ch as f32);
            }
        }
    }
    out
}

pub(crate) fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(CHARS[(b0 >> 2) as usize]);
        out.push(CHARS[((b0 & 0x03) << 4 | b1 >> 4) as usize]);
        out.push(if chunk.len() > 1 { CHARS[((b1 & 0x0f) << 2 | b2 >> 6) as usize] } else { b'=' });
        out.push(if chunk.len() > 2 { CHARS[(b2 & 0x3f) as usize] } else { b'=' });
    }
    unsafe { String::from_utf8_unchecked(out) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_core_audio_types::kAudioFormatFlagIsPacked;

    fn float32_pcm() -> AudioStreamBasicDescription {
        AudioStreamBasicDescription {
            mSampleRate: 48_000.0,
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
            mBytesPerPacket: 4, mFramesPerPacket: 1, mBytesPerFrame: 4,
            mChannelsPerFrame: 1, mBitsPerChannel: 32, mReserved: 0,
        }
    }

    #[test]
    fn accepts_float32_linear_pcm() { assert!(is_supported_pcm_format(&float32_pcm())); }

    #[test]
    fn rejects_non_32_bit_depth() {
        let mut a = float32_pcm(); a.mBitsPerChannel = 16;
        assert!(!is_supported_pcm_format(&a));
    }

    #[test]
    fn rejects_non_positive_sample_rate() {
        let mut z = float32_pcm(); z.mSampleRate = 0.0; assert!(!is_supported_pcm_format(&z));
    }

    #[test]
    fn resampler_emits_int16_le_and_does_not_ramp_from_silence() {
        // 16 kHz → 16 kHz is 1:1; a constant 0.5 input must stay ~0.5 from the first sample.
        let mut rs = Resampler::new(16_000.0, 16_000.0);
        let mut out = Vec::new();
        rs.process(&[0.5_f32; 8], &mut out);
        assert_eq!(out.len(), 16); // 8 samples * 2 bytes
        let first = i16::from_le_bytes([out[0], out[1]]);
        assert!((first as i32 - 16383).abs() < 50, "leading sample should be ~0.5 full-scale, got {first}");
    }

    #[test]
    fn resampler_downsamples_count() {
        // 44.1 kHz → 16 kHz produces ~16/44.1 as many samples.
        let mut rs = Resampler::new(44_100.0, 16_000.0);
        let mut out = Vec::new();
        rs.process(&[0.0_f32; 441], &mut out);
        let produced = out.len() / 2;
        assert!((150..=170).contains(&produced), "expected ~160 samples, got {produced}");
    }
}
