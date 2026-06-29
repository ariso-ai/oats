//! Microphone capture via a plain Core Audio HAL input IO proc (macOS only).
//!
//! Captures the system's default input device without using Voice-Processing I/O
//! (AUVoiceIO), so it does not trigger macOS audio ducking of system audio.
//!
//! Flow: query the default input device → read its native stream format (input
//! scope) → verify it is 32-bit float interleaved LinearPCM → install an IO
//! block that receives Float32 PCM at the device's native rate → downmix to
//! mono, resample to 44.1 kHz, convert to Int16, and emit as `mic-audio-data`
//! (base64) for the recorder frontend.

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn start(_app: tauri::AppHandle) -> Result<(), String> {
        Err("Microphone capture is only supported on macOS".into())
    }
    pub fn stop() -> Result<(), String> {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use crate::audio_util::{
        base64_encode, downmix_to_mono, get_property, is_supported_pcm_format,
        AudioObjectID, Resampler,
    };
    use block2::RcBlock;
    use objc2_core_audio::{
        kAudioDevicePropertyStreamFormat, kAudioHardwarePropertyDefaultInputDevice,
        kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyScopeInput,
        kAudioObjectSystemObject, AudioDeviceCreateIOProcIDWithBlock,
        AudioDeviceDestroyIOProcID, AudioDeviceIOProcID, AudioDeviceStart, AudioDeviceStop,
    };
    use objc2_core_audio_types::{AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp};
    use std::ptr::NonNull;
    use std::sync::{Arc, Mutex};
    use tauri::Emitter;

    /// Live capture resources. Only the device id and IO proc id are needed —
    /// unlike the system-audio path there is no tap or aggregate device to track.
    /// All fields are plain integers; the IO block is owned by Core Audio
    /// (retained via `Block_copy` inside `AudioDeviceCreateIOProcIDWithBlock`
    /// and released by `AudioDeviceDestroyIOProcID`), so we don't need to
    /// keep a !Send `RcBlock` in this cross-thread state.
    struct CaptureState {
        device_id: AudioObjectID,
        proc_id: AudioDeviceIOProcID,
    }

    static CAPTURE: Mutex<Option<CaptureState>> = Mutex::new(None);

    pub fn start(app: tauri::AppHandle) -> Result<(), String> {
        let mut guard = CAPTURE.lock().map_err(|e| e.to_string())?;
        if guard.is_some() {
            return Err("Microphone capture already running".into());
        }

        unsafe {
            // 1. Default input device.
            let input_id: AudioObjectID = get_property(
                kAudioObjectSystemObject as AudioObjectID,
                kAudioHardwarePropertyDefaultInputDevice,
                kAudioObjectPropertyScopeGlobal,
            )?;
            if input_id == 0 {
                return Err("No default input device found".into());
            }

            // 2. Input stream format — use input scope so we read what the mic
            // actually delivers (not what a paired output would deliver).
            let asbd: AudioStreamBasicDescription = get_property(
                input_id,
                kAudioDevicePropertyStreamFormat,
                kAudioObjectPropertyScopeInput,
            )?;
            // The IO block reinterprets buffer bytes as `*const f32` in
            // `downmix_to_mono`, so the device must deliver 32-bit float
            // LinearPCM. Built-in mics do; non-float devices are out of scope
            // and must error rather than be misread as garbage.
            if !is_supported_pcm_format(&asbd) {
                return Err(format!(
                    "unsupported input stream format (id={}, flags={:#x}, bits={}); \
                     expected 32-bit float interleaved LinearPCM",
                    asbd.mFormatID, asbd.mFormatFlags, asbd.mBitsPerChannel
                ));
            }
            let src_rate = asbd.mSampleRate;

            // 3. IO block: downmix → resample → emit.
            let app = Arc::new(app);
            let resampler = Arc::new(Mutex::new(Resampler::new(src_rate, 44_100.0)));
            let app_cb = app.clone();
            let block = RcBlock::new(
                move |_now: NonNull<AudioTimeStamp>,
                      input: NonNull<AudioBufferList>,
                      _intime: NonNull<AudioTimeStamp>,
                      _out: NonNull<AudioBufferList>,
                      _outtime: NonNull<AudioTimeStamp>| {
                    // For an input device the captured samples arrive in the
                    // `input` AudioBufferList (not `_out`).
                    let mono = downmix_to_mono(input.as_ptr());
                    if mono.is_empty() {
                        return;
                    }
                    let mut bytes = Vec::with_capacity(mono.len() * 2);
                    if let Ok(mut rs) = resampler.lock() {
                        rs.process(&mono, &mut bytes);
                    }
                    if !bytes.is_empty() {
                        let b64 = base64_encode(&bytes);
                        let _ = app_cb.emit("mic-audio-data", b64);
                    }
                },
            );

            // 4. Register the IO proc and start capturing.
            let mut proc_id: AudioDeviceIOProcID = None;
            let status = AudioDeviceCreateIOProcIDWithBlock(
                NonNull::from(&mut proc_id),
                input_id,
                None,
                RcBlock::as_ptr(&block),
            );
            if status != 0 {
                return Err(format!(
                    "AudioDeviceCreateIOProcIDWithBlock failed: {status}"
                ));
            }

            let status = AudioDeviceStart(input_id, proc_id);
            if status != 0 {
                AudioDeviceDestroyIOProcID(input_id, proc_id);
                return Err(format!("AudioDeviceStart failed: {status}"));
            }

            // Core Audio copied the block during AudioDeviceCreateIOProcIDWithBlock
            // and will release that copy in AudioDeviceDestroyIOProcID. Our local
            // RcBlock retain is no longer needed; let it drop at end of scope on
            // this start() thread (it's !Send, so we can't carry it across threads
            // in CAPTURE).
            drop(block);

            *guard = Some(CaptureState { device_id: input_id, proc_id });
        }
        Ok(())
    }

    pub fn stop() -> Result<(), String> {
        let mut guard = CAPTURE.lock().map_err(|e| e.to_string())?;
        if let Some(state) = guard.take() {
            let mut errors: Vec<String> = Vec::new();
            unsafe {
                // Tear down in reverse creation order. Attempt every step even
                // if an earlier one fails, so a single failure doesn't leak the
                // remaining resources; collect statuses and report at the end.
                let status = AudioDeviceStop(state.device_id, state.proc_id);
                if status != 0 {
                    errors.push(format!("AudioDeviceStop failed: {status}"));
                }
                let status = AudioDeviceDestroyIOProcID(state.device_id, state.proc_id);
                if status != 0 {
                    errors.push(format!("AudioDeviceDestroyIOProcID failed: {status}"));
                }
            }
            if !errors.is_empty() {
                return Err(errors.join("; "));
            }
        }
        Ok(())
    }
}

/// Start capturing the microphone. Emits `mic-audio-data` events carrying
/// base64-encoded PCM Int16 mono 44.1 kHz data. Uses a plain Core Audio input
/// IO proc (not Voice-Processing I/O), so it does not duck system audio.
#[tauri::command]
pub fn start_microphone_capture(app: tauri::AppHandle) -> Result<(), String> {
    imp::start(app)
}

/// Stop the microphone capture.
#[tauri::command]
pub fn stop_microphone_capture() -> Result<(), String> {
    imp::stop()
}
