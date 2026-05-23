use screencapturekit::prelude::*;
use std::sync::{Arc, Mutex};
use tauri::Emitter;

/// Holds the active SCStream so we can stop it later.
struct CaptureState {
    stream: Option<SCStream>,
}

static CAPTURE: Mutex<Option<CaptureState>> = Mutex::new(None);

/// Start capturing system audio via ScreenCaptureKit.
/// Audio samples are emitted as `system-audio-data` events containing
/// base64-encoded PCM Int16 mono 16 kHz data.
#[tauri::command]
pub fn start_system_audio_capture(app: tauri::AppHandle) -> Result<(), String> {
    let mut guard = CAPTURE.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        return Err("System audio capture already running".into());
    }

    // Get the primary display for the content filter
    let content = SCShareableContent::get().map_err(|e| format!("Failed to get content: {e}"))?;
    let display = content
        .displays()
        .into_iter()
        .next()
        .ok_or("No display found")?;

    // Filter: capture the whole display (we only care about audio)
    let filter = SCContentFilter::create()
        .with_display(&display)
        .with_excluding_windows(&[])
        .build();

    // Configure: audio only at 16 kHz mono (matches Deepgram expectations)
    let config = SCStreamConfiguration::new()
        .with_width(2)
        .with_height(2)
        .with_captures_audio(true)
        .with_sample_rate(16000)
        .with_channel_count(1);

    let app_handle = Arc::new(app);

    let mut stream = SCStream::new(&filter, &config);

    let app_for_handler = app_handle.clone();
    stream.add_output_handler(
        move |sample: CMSampleBuffer, of_type: SCStreamOutputType| {
            if of_type != SCStreamOutputType::Audio {
                return;
            }

            // Extract PCM audio data from the sample buffer
            if let Some(data) = extract_audio_i16_bytes(&sample) {
                // Send as base64 to the frontend
                let b64 = base64_encode(&data);
                let _ = app_for_handler.emit("system-audio-data", b64);
            }
        },
        SCStreamOutputType::Audio,
    );

    stream
        .start_capture()
        .map_err(|e| format!("Failed to start capture: {e}"))?;

    *guard = Some(CaptureState {
        stream: Some(stream),
    });

    Ok(())
}

/// Stop the system audio capture.
#[tauri::command]
pub fn stop_system_audio_capture() -> Result<(), String> {
    let mut guard = CAPTURE.lock().map_err(|e| e.to_string())?;
    if let Some(mut state) = guard.take() {
        if let Some(stream) = state.stream.take() {
            stream
                .stop_capture()
                .map_err(|e| format!("Failed to stop capture: {e}"))?;
        }
    }
    Ok(())
}

/// Extract raw PCM Int16 bytes from a CMSampleBuffer.
/// ScreenCaptureKit delivers audio as Float32 PCM; we convert to Int16
/// to match the Deepgram linear16 format.
fn extract_audio_i16_bytes(sample: &CMSampleBuffer) -> Option<Vec<u8>> {
    let audio_buffers = sample.audio_buffer_list()?;
    let num_buffers = audio_buffers.num_buffers();

    let mut all_bytes: Vec<u8> = Vec::new();

    for i in 0..num_buffers {
        let Some(buffer) = audio_buffers.buffer(i) else {
            continue;
        };
        let data: &[u8] = buffer.data();
        if data.is_empty() {
            continue;
        }

        // ScreenCaptureKit delivers Float32 PCM — parse safely without alignment assumptions
        for chunk in data.chunks_exact(std::mem::size_of::<f32>()) {
            let s = f32::from_ne_bytes(chunk.try_into().unwrap());
            let clamped = s.clamp(-1.0, 1.0);
            let i16_val: i16 = if clamped < 0.0 {
                (clamped * 32768.0) as i16
            } else {
                (clamped * 32767.0) as i16
            };
            all_bytes.extend_from_slice(&i16_val.to_le_bytes());
        }
    }

    if all_bytes.is_empty() {
        None
    } else {
        Some(all_bytes)
    }
}

fn base64_encode(data: &[u8]) -> String {
    use std::io::Write;
    let mut buf = Vec::with_capacity(data.len() * 4 / 3 + 4);
    let mut encoder = Base64Encoder::new(&mut buf);
    encoder.write_all(data).unwrap();
    encoder.finish();
    // Safety: base64 output is always valid UTF-8
    unsafe { String::from_utf8_unchecked(buf) }
}

/// Minimal base64 encoder (avoids adding a dependency)
struct Base64Encoder<'a> {
    out: &'a mut Vec<u8>,
    buf: [u8; 3],
    len: usize,
}

const B64_CHARS: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

impl<'a> Base64Encoder<'a> {
    fn new(out: &'a mut Vec<u8>) -> Self {
        Self {
            out,
            buf: [0; 3],
            len: 0,
        }
    }

    fn flush_buf(&mut self) {
        if self.len == 0 {
            return;
        }
        let b = self.buf;
        self.out.push(B64_CHARS[(b[0] >> 2) as usize]);
        self.out
            .push(B64_CHARS[((b[0] & 0x03) << 4 | b[1] >> 4) as usize]);
        if self.len > 1 {
            self.out
                .push(B64_CHARS[((b[1] & 0x0f) << 2 | b[2] >> 6) as usize]);
        } else {
            self.out.push(b'=');
        }
        if self.len > 2 {
            self.out.push(B64_CHARS[(b[2] & 0x3f) as usize]);
        } else {
            self.out.push(b'=');
        }
        self.buf = [0; 3];
        self.len = 0;
    }

    fn finish(mut self) {
        self.flush_buf();
    }
}

impl<'a> std::io::Write for Base64Encoder<'a> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        for &byte in data {
            self.buf[self.len] = byte;
            self.len += 1;
            if self.len == 3 {
                self.flush_buf();
            }
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
