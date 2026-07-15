//! Native system-audio capture. macOS uses Core Audio process taps; Windows
//! uses WASAPI shared-mode loopback on the default render endpoint.
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

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    /// Linux has no capture backend yet; capability reporting keeps this path
    /// out of normal recording flows.
    pub fn start(_app: tauri::AppHandle) -> Result<(), String> {
        Err("System audio capture is not supported on this platform".into())
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

#[cfg(target_os = "windows")]
mod imp {
    use crate::audio_util::{
        Resampler, base64_encode, downmix_interleaved_f32,
    };
    use std::ptr;
    use std::slice;
    use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
    use std::sync::Mutex;
    use std::thread::JoinHandle;
    use std::time::Duration;
    use tauri::Emitter;
    use wasapi::{SampleType, WaveFormat, deinitialize, initialize_mta};
    use windows::Win32::Foundation::{
        CloseHandle, HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows::Win32::Media::Audio::{
        AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
        AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
        IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator, eConsole,
        eRender,
    };
    use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
    use windows::Win32::System::Threading::{CreateEventA, WaitForSingleObject};
    use windows::core::PCSTR;

    /// The capture thread owns every COM/WASAPI object. Stop communicates over
    /// a channel and joins that thread so no endpoint handle survives a retry.
    struct CaptureState {
        stop: Sender<()>,
        thread: JoinHandle<()>,
    }

    static CAPTURE: Mutex<Option<CaptureState>> = Mutex::new(None);

    struct ComGuard;

    impl Drop for ComGuard {
        fn drop(&mut self) {
            deinitialize();
        }
    }

    /// Keep the event alive for the full stream lifetime and close the native
    /// handle deterministically on every initialization and capture error.
    struct EventHandle(HANDLE);

    impl Drop for EventHandle {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    fn run_capture(
        app: tauri::AppHandle,
        ready: Sender<Result<(), String>>,
        stop: Receiver<()>,
    ) {
        let result = (|| -> Result<(), String> {
            let hr = initialize_mta();
            if hr.is_err() {
                return Err(format!("CoInitializeEx for WASAPI failed: {hr:?}"));
            }
            let _com = ComGuard;

            let enumerator: IMMDeviceEnumerator = unsafe {
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
            }
            .map_err(|e| e.to_string())?;
            let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
                .map_err(|e| format!("no default Windows output device: {e}"))?;
            let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }
                .map_err(|e| e.to_string())?;

            // Ask the Windows audio engine for a predictable interleaved Float32
            // layout. Shared-mode autoconversion handles the endpoint's native
            // rate/format, keeping the conversion contract hardware-independent.
            let source_rate = 48_000_u32;
            let channels = 2_usize;
            let format = WaveFormat::new(
                32,
                32,
                &SampleType::Float,
                source_rate as usize,
                channels,
                None,
            );
            let mut min_period = 0_i64;
            unsafe { client.GetDevicePeriod(None, Some(&mut min_period)) }
                .map_err(|e| e.to_string())?;
            let stream_flags = AUDCLNT_STREAMFLAGS_LOOPBACK
                | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY
                | AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
            // Shared-mode conversion gives every endpoint the same Float32
            // format while LOOPBACK selects rendered output instead of a mic.
            unsafe {
                client.Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    stream_flags,
                    min_period,
                    0,
                    ptr::addr_of!(format.wave_fmt.Format),
                    None,
                )
            }
            .map_err(|e| format!("initialize WASAPI loopback: {e}"))?;
            let event = EventHandle(
                unsafe { CreateEventA(None, false, false, PCSTR::null()) }
                    .map_err(|e| e.to_string())?,
            );
            unsafe { client.SetEventHandle(event.0) }.map_err(|e| e.to_string())?;
            let capture: IAudioCaptureClient = unsafe { client.GetService() }
                .map_err(|e| e.to_string())?;
            let bytes_per_frame = format.get_blockalign() as usize;
            let mut resampler = Resampler::new(source_rate as f64, 16_000.0);

            unsafe { client.Start() }
                .map_err(|e| format!("start WASAPI loopback: {e}"))?;
            if ready.send(Ok(())).is_err() {
                let _ = unsafe { client.Stop() };
                return Ok(());
            }

            let capture_result = (|| -> Result<(), String> {
                loop {
                    match stop.try_recv() {
                        Ok(()) | Err(TryRecvError::Disconnected) => break,
                        Err(TryRecvError::Empty) => {}
                    }

                    // Timeouts are expected while the render endpoint is quiet;
                    // the Vue mixer fills those spans with zeroes.
                    match unsafe { WaitForSingleObject(event.0, 200) } {
                        WAIT_OBJECT_0 => {}
                        WAIT_TIMEOUT => continue,
                        WAIT_FAILED => return Err("wait for WASAPI loopback packet failed".into()),
                        status => {
                            return Err(format!(
                                "unexpected WASAPI loopback wait status: {}",
                                status.0
                            ));
                        }
                    }

                    loop {
                        let frames = unsafe { capture.GetNextPacketSize() }
                            .map_err(|e| e.to_string())?;
                        if frames == 0 {
                            break;
                        }

                        let mut data = ptr::null_mut();
                        let mut read_frames = 0_u32;
                        let mut flags = 0_u32;
                        unsafe {
                            capture.GetBuffer(
                                &mut data,
                                &mut read_frames,
                                &mut flags,
                                None,
                                None,
                            )
                        }
                        .map_err(|e| format!("read WASAPI loopback packet: {e}"))?;

                        // Windows may return a null data pointer for SILENT
                        // packets. Build the mono data before releasing the
                        // packet, but never form a slice from that null pointer.
                        let mono_result = if read_frames == 0 {
                            Ok(Vec::new())
                        } else if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                            Ok(vec![0.0; read_frames as usize])
                        } else if data.is_null() {
                            Err("WASAPI returned a null non-silent buffer".into())
                        } else {
                            match (read_frames as usize).checked_mul(bytes_per_frame) {
                                Some(byte_len) => {
                                    let bytes = unsafe { slice::from_raw_parts(data, byte_len) };
                                    downmix_interleaved_f32(bytes, channels)
                                }
                                None => Err("WASAPI packet size overflow".into()),
                            }
                        };
                        unsafe { capture.ReleaseBuffer(read_frames) }
                            .map_err(|e| format!("release WASAPI loopback packet: {e}"))?;
                        let mono = mono_result?;

                        let mut pcm = Vec::with_capacity(mono.len() * 2);
                        resampler.process(&mono, &mut pcm);
                        if !pcm.is_empty() {
                            let _ = app.emit("system-audio-data", base64_encode(&pcm));
                        }
                    }
                }
                Ok(())
            })();

            let stop_result = unsafe { client.Stop() }
                .map_err(|e| format!("stop WASAPI loopback: {e}"));
            capture_result.and(stop_result)
        })();

        if let Err(error) = result {
            let _ = ready.send(Err(error.clone()));
            eprintln!("windows system-audio capture stopped: {error}");
        }
    }

    pub fn start(app: tauri::AppHandle) -> Result<(), String> {
        let mut guard = CAPTURE.lock().map_err(|e| e.to_string())?;
        if guard.is_some() {
            return Err("System audio capture already running".into());
        }

        let (stop_tx, stop_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("oats-wasapi-loopback".into())
            .spawn(move || run_capture(app, ready_tx, stop_rx))
            .map_err(|e| format!("start WASAPI capture thread: {e}"))?;

        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => {
                *guard = Some(CaptureState {
                    stop: stop_tx,
                    thread,
                });
                Ok(())
            }
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(error) => {
                let _ = stop_tx.send(());
                let _ = thread.join();
                Err(format!("WASAPI capture initialization timed out: {error}"))
            }
        }
    }

    pub fn stop() -> Result<(), String> {
        let state = CAPTURE.lock().map_err(|e| e.to_string())?.take();
        let Some(state) = state else {
            return Ok(());
        };
        let _ = state.stop.send(());
        state
            .thread
            .join()
            .map_err(|_| "WASAPI capture thread panicked".to_string())
    }

    // Endpoint loopback does not require a Windows privacy prompt. A missing
    // or disabled output device is reported by `start`, where it is actionable.
    pub fn request_permission() -> bool {
        true
    }

    pub fn check_permission() -> bool {
        true
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use crate::audio_util::{
        base64_encode, downmix_to_mono, get_property, is_supported_pcm_format, ns,
        AudioObjectID, Resampler,
    };
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::AllocAnyThread;
    use objc2_core_audio::{
        kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceMainSubDeviceKey,
        kAudioAggregateDeviceNameKey, kAudioAggregateDeviceSubDeviceListKey,
        kAudioAggregateDeviceTapAutoStartKey, kAudioAggregateDeviceTapListKey,
        kAudioAggregateDeviceUIDKey, kAudioDevicePropertyDeviceUID,
        kAudioHardwarePropertyDefaultSystemOutputDevice,
        kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, kAudioSubDeviceUIDKey,
        kAudioSubTapDriftCompensationKey, kAudioSubTapUIDKey, kAudioTapPropertyFormat,
        AudioDeviceCreateIOProcIDWithBlock, AudioDeviceDestroyIOProcID, AudioDeviceIOProcID,
        AudioDeviceStart, AudioDeviceStop, AudioHardwareCreateAggregateDevice,
        AudioHardwareCreateProcessTap, AudioHardwareDestroyAggregateDevice,
        AudioHardwareDestroyProcessTap,
        CATapDescription, CATapMuteBehavior,
    };
    use objc2_core_audio_types::{
        AudioBufferList,
        AudioStreamBasicDescription, AudioTimeStamp,
    };
    use objc2_core_foundation::{CFDictionary, CFRetained, CFString};
    use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSObject, NSString};
    use std::ptr::NonNull;
    use std::sync::{Arc, Mutex};
    use tauri::Emitter;

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
                kAudioObjectPropertyScopeGlobal,
            ) {
                Ok(v) => v,
                Err(e) => {
                    AudioHardwareDestroyProcessTap(tap_id);
                    return Err(e);
                }
            };
            let output_uid_cf: CFRetained<CFString> =
                match get_property::<*const CFString>(output_id, kAudioDevicePropertyDeviceUID, kAudioObjectPropertyScopeGlobal) {
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
                match get_property(tap_id, kAudioTapPropertyFormat, kAudioObjectPropertyScopeGlobal) {
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
            if !is_supported_pcm_format(&asbd) {
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
            let resampler = Arc::new(Mutex::new(Resampler::new(src_rate, 16_000.0)));
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
