//! System-audio capture via Core Audio process taps (macOS 14.4+).
//!
//! This replaces the previous ScreenCaptureKit implementation. ScreenCaptureKit
//! gated audio behind the broad "Screen & System Audio Recording" permission;
//! Core Audio process taps capture system audio under the narrow
//! "System Audio Recording" permission (declared as `NSAudioCaptureUsageDescription`
//! in Info.plist), which is what users see and grant.
//!
//! Flow: create a mono global process tap → wrap it in a private aggregate
//! device whose main sub-device is the current default output → install an IO
//! block that receives Float32 PCM at the device's native rate → downmix to
//! mono, resample to 16 kHz, convert to Int16, and emit as `system-audio-data`
//! (base64) to match the contract the recorder frontend already consumes.

#[cfg(not(target_os = "macos"))]
mod imp {
    /// No system-audio capture off macOS; the recorder treats this as "no
    /// system source available" and falls back to mic-only.
    pub fn start(_app: tauri::AppHandle) -> Result<(), String> {
        Err("System audio capture is only supported on macOS".into())
    }
    pub fn stop() -> Result<(), String> {
        Ok(())
    }
    pub fn request_permission() -> bool {
        true
    }
    pub fn check_permission() -> bool {
        true
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::AllocAnyThread;
    use objc2_core_audio::{
        kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceMainSubDeviceKey,
        kAudioAggregateDeviceNameKey, kAudioAggregateDeviceSubDeviceListKey,
        kAudioAggregateDeviceTapAutoStartKey, kAudioAggregateDeviceTapListKey,
        kAudioAggregateDeviceUIDKey, kAudioDevicePropertyDeviceUID,
        kAudioHardwarePropertyDefaultSystemOutputDevice, kAudioObjectPropertyElementMain,
        kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, kAudioSubDeviceUIDKey,
        kAudioSubTapDriftCompensationKey, kAudioSubTapUIDKey, kAudioTapPropertyFormat,
        AudioDeviceCreateIOProcIDWithBlock, AudioDeviceDestroyIOProcID, AudioDeviceIOProcID,
        AudioDeviceStart, AudioDeviceStop, AudioHardwareCreateAggregateDevice,
        AudioHardwareCreateProcessTap, AudioHardwareDestroyAggregateDevice,
        AudioHardwareDestroyProcessTap, AudioObjectGetPropertyData, AudioObjectPropertyAddress,
        CATapDescription, CATapMuteBehavior,
    };
    use objc2_core_audio_types::{
        kAudioFormatFlagIsFloat, kAudioFormatLinearPCM, AudioBufferList, AudioStreamBasicDescription,
        AudioTimeStamp,
    };
    use objc2_core_foundation::{CFDictionary, CFRetained, CFString};
    use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSObject, NSString};
    use std::ffi::{c_void, CStr};
    use std::ptr::NonNull;
    use std::sync::{Arc, Mutex};
    use tauri::Emitter;

    type AudioObjectID = u32;

    /// Live capture resources, torn down in reverse creation order on stop.
    /// All fields are plain integers; the IO block is owned by Core Audio
    /// (retained via `Block_copy` inside `AudioDeviceCreateIOProcIDWithBlock`
    /// and released by `AudioDeviceDestroyIOProcID`), so we don't need to
    /// keep a !Send `RcBlock` in this cross-thread state.
    struct CaptureState {
        tap_id: AudioObjectID,
        aggregate_id: AudioObjectID,
        proc_id: AudioDeviceIOProcID,
    }

    static CAPTURE: Mutex<Option<CaptureState>> = Mutex::new(None);

    fn ns(c: &CStr) -> Retained<NSString> {
        NSString::from_str(c.to_str().expect("Core Audio key is valid UTF-8"))
    }

    /// Streaming linear resampler from `src_rate` to 16 kHz, carrying fractional
    /// position and the last input sample across IO callbacks so block
    /// boundaries don't click.
    struct Resampler {
        step: f64,
        pos: f64,
        prev: f32,
        primed: bool,
    }

    impl Resampler {
        fn new(src_rate: f64) -> Self {
            Self {
                step: src_rate / 16_000.0,
                pos: 0.0,
                prev: 0.0,
                primed: false,
            }
        }

        /// Resample `input` (mono f32) into `out` as little-endian Int16 bytes.
        fn process(&mut self, input: &[f32], out: &mut Vec<u8>) {
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
            // Position coordinate: 0 == `prev`, k == input[k-1] for k in 1..=n.
            let sample_at = |j: f64| -> f32 {
                let idx = j as isize;
                if idx <= 0 {
                    self.prev
                } else {
                    input[(idx - 1).min(n as isize - 1) as usize]
                }
            };
            while self.pos < n as f64 {
                let base = self.pos.floor();
                let frac = (self.pos - base) as f32;
                let a = sample_at(base);
                let b = sample_at(base + 1.0);
                let s = a + (b - a) * frac;
                let clamped = s.clamp(-1.0, 1.0);
                let v: i16 = if clamped < 0.0 {
                    (clamped * 32768.0) as i16
                } else {
                    (clamped * 32767.0) as i16
                };
                out.extend_from_slice(&v.to_le_bytes());
                self.pos += self.step;
            }
            // Carry the unused fraction into the next block, whose origin moves
            // to input[n-1].
            self.pos -= n as f64;
            self.prev = input[n - 1];
        }
    }

    fn prop_address(selector: u32) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        }
    }

    /// Read a fixed-size Core Audio property into `T`.
    unsafe fn get_property<T>(object: AudioObjectID, selector: u32) -> Result<T, String> {
        let addr = prop_address(selector);
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
        Ok(unsafe { value.assume_init() })
    }

    /// Build the aggregate-device description dictionary (toll-free bridged to
    /// CFDictionary). Keys are the Core Audio C-string constants; values upcast
    /// to `&NSObject` via deref coercion.
    fn build_aggregate_dict(
        agg_uid: &str,
        output_uid: &NSString,
        tap_uuid: &str,
    ) -> Retained<NSDictionary<NSString, NSObject>> {
        // Inner sub-device list: [{ kAudioSubDeviceUIDKey: <output uid> }]
        let sub_dev_key = ns(kAudioSubDeviceUIDKey);
        let sub_device =
            NSDictionary::from_slices(&[&*sub_dev_key], &[output_uid as &NSObject]);
        let sub_list = NSArray::from_retained_slice(&[sub_device]);

        // Inner tap list: [{ drift: true, kAudioSubTapUIDKey: <tap uuid> }]
        let tap_drift_key = ns(kAudioSubTapDriftCompensationKey);
        let tap_uid_key = ns(kAudioSubTapUIDKey);
        let tap_drift_val = NSNumber::numberWithBool(true);
        let tap_uid_val = NSString::from_str(tap_uuid);
        let tap = NSDictionary::from_slices(
            &[&*tap_drift_key, &*tap_uid_key],
            &[&*tap_drift_val as &NSObject, &*tap_uid_val],
        );
        let tap_list = NSArray::from_retained_slice(&[tap]);

        let k_name = ns(kAudioAggregateDeviceNameKey);
        let k_uid = ns(kAudioAggregateDeviceUIDKey);
        let k_main = ns(kAudioAggregateDeviceMainSubDeviceKey);
        let k_priv = ns(kAudioAggregateDeviceIsPrivateKey);
        let k_auto = ns(kAudioAggregateDeviceTapAutoStartKey);
        let k_subs = ns(kAudioAggregateDeviceSubDeviceListKey);
        let k_taps = ns(kAudioAggregateDeviceTapListKey);

        let v_name = NSString::from_str("Oats System Audio Tap");
        let v_uid = NSString::from_str(agg_uid);
        let v_priv = NSNumber::numberWithBool(true);
        let v_auto = NSNumber::numberWithBool(true);

        let keys: [&NSString; 7] = [
            &k_name, &k_uid, &k_main, &k_priv, &k_auto, &k_subs, &k_taps,
        ];
        let values: [&NSObject; 7] = [
            &v_name,
            &v_uid,
            output_uid,
            &v_priv,
            &v_auto,
            &sub_list,
            &tap_list,
        ];
        NSDictionary::from_slices(&keys, &values)
    }

    pub fn start(app: tauri::AppHandle) -> Result<(), String> {
        let mut guard = CAPTURE.lock().map_err(|e| e.to_string())?;
        if guard.is_some() {
            return Err("System audio capture already running".into());
        }

        unsafe {
            // 1. Mono global tap over the whole system (exclude nothing).
            let exclude: Retained<NSArray<NSNumber>> = NSArray::new();
            let tap_desc = CATapDescription::initMonoGlobalTapButExcludeProcesses(
                CATapDescription::alloc(),
                &exclude,
            );
            tap_desc.setName(&NSString::from_str("Oats System Audio Tap"));
            tap_desc.setPrivate(true);
            tap_desc.setMuteBehavior(CATapMuteBehavior::Unmuted);
            let tap_uuid = tap_desc.UUID().UUIDString().to_string();

            let mut tap_id: AudioObjectID = 0;
            let status = AudioHardwareCreateProcessTap(Some(&tap_desc), &mut tap_id);
            if status != 0 {
                return Err(format!("AudioHardwareCreateProcessTap failed: {status}"));
            }

            // 2. Default output device + its UID (the aggregate's clock source).
            let output_id: AudioObjectID = match get_property(
                kAudioObjectSystemObject as AudioObjectID,
                kAudioHardwarePropertyDefaultSystemOutputDevice,
            ) {
                Ok(v) => v,
                Err(e) => {
                    AudioHardwareDestroyProcessTap(tap_id);
                    return Err(e);
                }
            };
            let output_uid_cf: CFRetained<CFString> =
                match get_property::<*const CFString>(output_id, kAudioDevicePropertyDeviceUID) {
                    // Core Audio can return status 0 with a null/absent UID for some
                    // virtual or aggregate output devices. Guard the pointer instead of
                    // unwrapping: a null here would panic, and handing a non-owned null to
                    // `CFRetained::from_raw` (which assumes a +1 retained object) is the
                    // start of a refcount/UAF bug, not just a crash.
                    Ok(ptr) => match NonNull::new(ptr as *mut CFString) {
                        Some(nn) => CFRetained::from_raw(nn),
                        None => {
                            AudioHardwareDestroyProcessTap(tap_id);
                            return Err("default output device has no UID".into());
                        }
                    },
                    Err(e) => {
                        AudioHardwareDestroyProcessTap(tap_id);
                        return Err(e);
                    }
                };
            let output_uid = NSString::from_str(&output_uid_cf.to_string());

            // 3. Tap stream format → native sample rate for the resampler.
            let asbd: AudioStreamBasicDescription =
                match get_property(tap_id, kAudioTapPropertyFormat) {
                    Ok(v) => v,
                    Err(e) => {
                        AudioHardwareDestroyProcessTap(tap_id);
                        return Err(e);
                    }
                };
            // The IO block reinterprets buffer bytes as `*const f32` in
            // `downmix_to_mono`, so the tap must actually deliver 32-bit float
            // LinearPCM. Taps normally do, but verify before trusting the cast:
            // a non-Float32 layout would otherwise be read as garbage samples.
            if asbd.mFormatID != kAudioFormatLinearPCM
                || asbd.mFormatFlags & kAudioFormatFlagIsFloat == 0
                || asbd.mBitsPerChannel != 32
            {
                AudioHardwareDestroyProcessTap(tap_id);
                return Err(format!(
                    "unsupported tap stream format (id={}, flags={:#x}, bits={}); expected 32-bit float LinearPCM",
                    asbd.mFormatID, asbd.mFormatFlags, asbd.mBitsPerChannel
                ));
            }
            let src_rate = asbd.mSampleRate;

            // 4. Private aggregate device wrapping the tap.
            let agg_uid = format!("ai.ariso.oats.tap.{tap_uuid}");
            let dict = build_aggregate_dict(&agg_uid, &output_uid, &tap_uuid);
            let cf_dict: &CFDictionary =
                &*(Retained::as_ptr(&dict) as *const CFDictionary);
            let mut aggregate_id: AudioObjectID = 0;
            let status =
                AudioHardwareCreateAggregateDevice(cf_dict, NonNull::from(&mut aggregate_id));
            if status != 0 {
                AudioHardwareDestroyProcessTap(tap_id);
                return Err(format!("AudioHardwareCreateAggregateDevice failed: {status}"));
            }

            // 5. IO block: downmix → resample → emit.
            let app = Arc::new(app);
            let resampler = Arc::new(Mutex::new(Resampler::new(src_rate)));
            let app_cb = app.clone();
            let block = RcBlock::new(
                move |_now: NonNull<AudioTimeStamp>,
                      input: NonNull<AudioBufferList>,
                      _intime: NonNull<AudioTimeStamp>,
                      _out: NonNull<AudioBufferList>,
                      _outtime: NonNull<AudioTimeStamp>| {
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
                        let _ = app_cb.emit("system-audio-data", b64);
                    }
                },
            );

            let mut proc_id: AudioDeviceIOProcID = None;
            let status = AudioDeviceCreateIOProcIDWithBlock(
                NonNull::from(&mut proc_id),
                aggregate_id,
                None,
                RcBlock::as_ptr(&block),
            );
            if status != 0 {
                AudioHardwareDestroyAggregateDevice(aggregate_id);
                AudioHardwareDestroyProcessTap(tap_id);
                return Err(format!("AudioDeviceCreateIOProcIDWithBlock failed: {status}"));
            }

            let status = AudioDeviceStart(aggregate_id, proc_id);
            if status != 0 {
                AudioDeviceDestroyIOProcID(aggregate_id, proc_id);
                AudioHardwareDestroyAggregateDevice(aggregate_id);
                AudioHardwareDestroyProcessTap(tap_id);
                return Err(format!("AudioDeviceStart failed: {status}"));
            }

            // Core Audio copied the block during AudioDeviceCreateIOProcIDWithBlock
            // and will release that copy in AudioDeviceDestroyIOProcID. Our local
            // RcBlock retain is no longer needed; let it drop at end of scope on
            // this start() thread (it's !Send, so we can't carry it across threads
            // in CAPTURE).
            drop(block);

            *guard = Some(CaptureState {
                tap_id,
                aggregate_id,
                proc_id,
            });
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
                let status = AudioDeviceStop(state.aggregate_id, state.proc_id);
                if status != 0 {
                    errors.push(format!("AudioDeviceStop failed: {status}"));
                }
                let status = AudioDeviceDestroyIOProcID(state.aggregate_id, state.proc_id);
                if status != 0 {
                    errors.push(format!("AudioDeviceDestroyIOProcID failed: {status}"));
                }
                let status = AudioHardwareDestroyAggregateDevice(state.aggregate_id);
                if status != 0 {
                    errors.push(format!("AudioHardwareDestroyAggregateDevice failed: {status}"));
                }
                let status = AudioHardwareDestroyProcessTap(state.tap_id);
                if status != 0 {
                    errors.push(format!("AudioHardwareDestroyProcessTap failed: {status}"));
                }
            }
            if !errors.is_empty() {
                return Err(errors.join("; "));
            }
        }
        Ok(())
    }

    /// Average all channels of an interleaved Float32 `AudioBufferList` into a
    /// mono Vec. Process taps deliver one buffer of interleaved floats.
    unsafe fn downmix_to_mono(list: *const AudioBufferList) -> Vec<f32> {
        let list = unsafe { &*list };
        let n = list.mNumberBuffers as usize;
        if n == 0 {
            return Vec::new();
        }
        // mBuffers is a flexible array; index from its address.
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
                // Interleaved: average each frame's channels.
                for frame in data.chunks_exact(ch as usize) {
                    let sum: f32 = frame.iter().copied().sum();
                    out.push(sum / ch as f32);
                }
            }
        }
        out
    }

    // macOS audio-capture (TCC) permission. There is no public API to preflight
    // or request the system-audio permission directly; the OS surfaces the
    // prompt the first time `AudioHardwareCreateProcessTap` actually taps audio.
    // We approximate request/check by attempting a throwaway tap: if the tap
    // creates successfully, access is (or has just been) granted.
    pub fn request_permission() -> bool {
        probe_tap()
    }

    pub fn check_permission() -> bool {
        probe_tap()
    }

    fn probe_tap() -> bool {
        unsafe {
            let exclude: Retained<NSArray<NSNumber>> = NSArray::new();
            let desc = CATapDescription::initMonoGlobalTapButExcludeProcesses(
                CATapDescription::alloc(),
                &exclude,
            );
            desc.setPrivate(true);
            desc.setMuteBehavior(CATapMuteBehavior::Unmuted);
            let mut tap_id: AudioObjectID = 0;
            let status = AudioHardwareCreateProcessTap(Some(&desc), &mut tap_id);
            if status == 0 {
                AudioHardwareDestroyProcessTap(tap_id);
                true
            } else {
                false
            }
        }
    }

    fn base64_encode(data: &[u8]) -> String {
        const CHARS: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = Vec::with_capacity(data.len().div_ceil(3) * 4);
        for chunk in data.chunks(3) {
            let b0 = chunk[0];
            let b1 = chunk.get(1).copied().unwrap_or(0);
            let b2 = chunk.get(2).copied().unwrap_or(0);
            out.push(CHARS[(b0 >> 2) as usize]);
            out.push(CHARS[((b0 & 0x03) << 4 | b1 >> 4) as usize]);
            out.push(if chunk.len() > 1 {
                CHARS[((b1 & 0x0f) << 2 | b2 >> 6) as usize]
            } else {
                b'='
            });
            out.push(if chunk.len() > 2 {
                CHARS[(b2 & 0x3f) as usize]
            } else {
                b'='
            });
        }
        // Safety: base64 output is always valid UTF-8.
        unsafe { String::from_utf8_unchecked(out) }
    }
}

/// Start capturing system audio. Emits `system-audio-data` events carrying
/// base64-encoded PCM Int16 mono 16 kHz data.
#[tauri::command]
pub fn start_system_audio_capture(app: tauri::AppHandle) -> Result<(), String> {
    imp::start(app)
}

/// Stop the system audio capture.
#[tauri::command]
pub fn stop_system_audio_capture() -> Result<(), String> {
    imp::stop()
}

/// Prompt for / verify the macOS system-audio (audio recording) permission.
#[tauri::command]
pub fn request_screen_capture_permission() -> bool {
    imp::request_permission()
}

/// Current system-audio permission status.
#[tauri::command]
pub fn check_screen_capture_permission() -> bool {
    imp::check_permission()
}
