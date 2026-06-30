//! Microphone capture via a plain Core Audio HAL input IO proc (macOS only).
//!
//! Captures the system's default input device without using Voice-Processing I/O
//! (AUVoiceIO), so it does not trigger macOS audio ducking of system audio.
//!
//! Flow: query the default input device → read its native stream format (input
//! scope) → verify it is 32-bit float LinearPCM (interleaved, or non-interleaved
//! mono) → install an IO block that receives Float32 PCM at the device's native
//! rate → downmix to mono, resample to 44.1 kHz, convert to Int16, and emit as
//! `mic-audio-data` (base64) for the recorder frontend.

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn start(_app: tauri::AppHandle) -> Result<(), String> {
        Err("Microphone capture is only supported on macOS".into())
    }
    pub fn stop() -> Result<(), String> {
        Ok(())
    }
    pub fn check_permission() -> bool {
        false
    }
    pub fn request_permission() -> bool {
        false
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use crate::audio_util::{
        base64_encode, downmix_to_mono, get_property, AudioObjectID, Resampler,
    };
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
    use objc2_core_audio::{
        kAudioDevicePropertyStreamFormat, kAudioHardwarePropertyDefaultInputDevice,
        kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyScopeInput,
        kAudioObjectSystemObject, AudioDeviceCreateIOProcIDWithBlock,
        AudioDeviceDestroyIOProcID, AudioDeviceIOProcID, AudioDeviceStart, AudioDeviceStop,
    };
    use objc2_core_audio_types::{
        kAudioFormatFlagIsBigEndian, kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked,
        kAudioFormatLinearPCM, kLinearPCMFormatFlagIsNonInterleaved, AudioBufferList,
        AudioStreamBasicDescription, AudioTimeStamp,
    };
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

    /// Like `audio_util::is_supported_pcm_format`, but tolerant of the
    /// non-interleaved flag for a single-channel device. HAL hardware inputs
    /// (including the built-in mic) commonly report a non-interleaved native
    /// format even for mono, where interleaving is a no-op and `downmix_to_mono`
    /// handles it correctly. Non-interleaved *multi*-channel is still rejected:
    /// `downmix_to_mono` would concatenate rather than mix those channels.
    fn is_supported_input_format(asbd: &AudioStreamBasicDescription) -> bool {
        let interleaved_ok = asbd.mFormatFlags & kLinearPCMFormatFlagIsNonInterleaved == 0
            || asbd.mChannelsPerFrame == 1;
        asbd.mFormatID == kAudioFormatLinearPCM
            && asbd.mFormatFlags & kAudioFormatFlagIsFloat != 0
            && asbd.mFormatFlags & kAudioFormatFlagIsPacked != 0
            && asbd.mFormatFlags & kAudioFormatFlagIsBigEndian == 0
            && interleaved_ok
            && asbd.mBitsPerChannel == 32
            && asbd.mSampleRate > 0.0
    }

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
            if !is_supported_input_format(&asbd) {
                return Err(format!(
                    "unsupported input stream format (id={}, flags={:#x}, bits={}); \
                     expected 32-bit float LinearPCM (interleaved, or non-interleaved mono)",
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

    /// Returns `true` if the app is already authorized for microphone access.
    pub fn check_permission() -> bool {
        unsafe {
            let audio_type = AVMediaTypeAudio.expect("AVMediaTypeAudio must be non-null");
            AVCaptureDevice::authorizationStatusForMediaType(audio_type)
                == AVAuthorizationStatus::Authorized
        }
    }

    /// Requests microphone access from the user if not yet determined.
    ///
    /// - Already `Authorized`: returns `true` immediately.
    /// - `NotDetermined`: presents the TCC prompt and blocks until the user
    ///   responds, then returns the result.
    /// - `Denied` / `Restricted`: returns `false` immediately.
    pub fn request_permission() -> bool {
        unsafe {
            let audio_type = AVMediaTypeAudio.expect("AVMediaTypeAudio must be non-null");
            let status = AVCaptureDevice::authorizationStatusForMediaType(audio_type);
            match status {
                AVAuthorizationStatus::Authorized => true,
                AVAuthorizationStatus::NotDetermined => {
                    // requestAccessForMediaType:completionHandler: is async; block
                    // on the result with a channel so the command returns a
                    // definite bool rather than racing with the prompt.
                    let (tx, rx) = std::sync::mpsc::channel::<bool>();
                    let handler = RcBlock::new(move |granted: Bool| {
                        let _ = tx.send(granted.as_bool());
                    });
                    AVCaptureDevice::requestAccessForMediaType_completionHandler(
                        audio_type,
                        &*handler,
                    );
                    rx.recv().unwrap_or(false)
                }
                _ => false, // Denied or Restricted
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn interleaved_mono_f32() -> AudioStreamBasicDescription {
            AudioStreamBasicDescription {
                mSampleRate: 48_000.0,
                mFormatID: kAudioFormatLinearPCM,
                mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
                mBytesPerPacket: 4,
                mFramesPerPacket: 1,
                mBytesPerFrame: 4,
                mChannelsPerFrame: 1,
                mBitsPerChannel: 32,
                mReserved: 0,
            }
        }

        #[test]
        fn accepts_interleaved_mono_float32() {
            assert!(is_supported_input_format(&interleaved_mono_f32()));
        }

        #[test]
        fn accepts_non_interleaved_mono_float32() {
            // macOS commonly reports the built-in mic as non-interleaved mono.
            let mut asbd = interleaved_mono_f32();
            asbd.mFormatFlags |= kLinearPCMFormatFlagIsNonInterleaved;
            assert!(is_supported_input_format(&asbd));
        }

        #[test]
        fn rejects_non_interleaved_stereo_float32() {
            // Non-interleaved multi-channel would be mishandled by downmix_to_mono
            // (it concatenates rather than mixes channels in that layout).
            let mut asbd = interleaved_mono_f32();
            asbd.mFormatFlags |= kLinearPCMFormatFlagIsNonInterleaved;
            asbd.mChannelsPerFrame = 2;
            assert!(!is_supported_input_format(&asbd));
        }
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

/// Prompt for (or verify) the macOS microphone TCC permission.
///
/// Returns `true` if the user granted (or had already granted) access,
/// `false` otherwise. On non-macOS platforms always returns `false`.
#[tauri::command]
pub async fn request_microphone_permission() -> bool {
    tokio::task::spawn_blocking(|| imp::request_permission())
        .await
        .unwrap_or(false)
}

/// Current microphone TCC permission status.
///
/// Returns `true` if access is already authorized, `false` in all other
/// states (not-determined, denied, restricted, or non-macOS).
#[tauri::command]
pub fn check_microphone_permission() -> bool {
    imp::check_permission()
}
