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
    use std::ffi::c_void;
    use std::ptr;
    use std::slice;
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
    use std::sync::Mutex;
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};
    use tauri::Emitter;
    use wasapi::{SampleType, WaveFormat, deinitialize, initialize_mta};
    use windows::Win32::Foundation::{
        CloseHandle, HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows::Win32::Media::Audio::{
        AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_E_DEVICE_INVALIDATED, AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
        AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
        IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
        eConsole, eRender,
    };
    use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};
    use windows::Win32::System::Threading::{CreateEventA, WaitForSingleObject};
    use windows::core::PCSTR;

    /// How often the capture loop re-checks which endpoint Windows considers
    /// the default. A shared-mode loopback client stays bound to the endpoint
    /// it was activated on, so this poll is what makes capture follow a switch
    /// (default output changed, headset connected) mid-recording.
    const ENDPOINT_POLL: Duration = Duration::from_secs(1);

    /// The capture thread owns every COM/WASAPI object. Stop communicates over
    /// a channel and joins that thread so no endpoint handle survives a retry.
    struct CaptureState {
        stop: Sender<()>,
        thread: JoinHandle<()>,
    }

    static CAPTURE: Mutex<Option<CaptureState>> = Mutex::new(None);

    /// A capture failure is reported from the worker thread, which can finish
    /// before the recorder calls `stop`. Reap that completed worker here so a
    /// later recording is not rejected by state that no longer owns resources.
    fn reap_finished_capture(state: &mut Option<CaptureState>) {
        if state
            .as_ref()
            .is_some_and(|capture| capture.thread.is_finished())
        {
            if let Some(finished) = state.take() {
                let _ = finished.thread.join();
            }
        }
    }

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

    /// One activated loopback stream, bound to the endpoint that was default
    /// when it was opened. Recreated whenever that stops being the default or
    /// the endpoint is invalidated underneath us.
    struct Loopback {
        client: IAudioClient,
        capture: IAudioCaptureClient,
        event: EventHandle,
        bytes_per_frame: usize,
        endpoint_id: String,
    }

    impl Drop for Loopback {
        fn drop(&mut self) {
            // Best-effort: an endpoint that was invalidated (unplugged, or
            // disabled) fails here and there is nothing left to do about it.
            // Fields drop afterwards, so the event handle outlives this stop.
            let _ = unsafe { self.client.Stop() };
        }
    }

    /// Why reading packets ended. A lost endpoint is recoverable by reopening;
    /// anything else ends the capture and is reported to the recorder.
    enum PacketError {
        Invalidated,
        Fatal(String),
    }

    fn classify(error: windows::core::Error, context: &str) -> PacketError {
        if error.code() == AUDCLNT_E_DEVICE_INVALIDATED {
            PacketError::Invalidated
        } else {
            PacketError::Fatal(format!("{context}: {error}"))
        }
    }

    /// The endpoint's stable id string. `GetId` hands back a COM allocation the
    /// caller owns, so free it rather than leaking one string per poll.
    fn endpoint_id(device: &IMMDevice) -> Result<String, String> {
        let raw = unsafe { device.GetId() }.map_err(|e| e.to_string())?;
        let id = unsafe { raw.to_string() }.map_err(|e| e.to_string());
        unsafe { CoTaskMemFree(Some(raw.0 as *const c_void)) };
        id
    }

    fn default_endpoint(enumerator: &IMMDeviceEnumerator) -> Result<(IMMDevice, String), String> {
        let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
            .map_err(|e| format!("no default Windows output device: {e}"))?;
        let id = endpoint_id(&device)?;
        Ok((device, id))
    }

    /// Activate loopback capture on whatever endpoint is default right now.
    fn open_default_loopback(
        enumerator: &IMMDeviceEnumerator,
        source_rate: u32,
        channels: usize,
    ) -> Result<Loopback, String> {
        let (device, endpoint_id) = default_endpoint(enumerator)?;
        let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }
            .map_err(|e| e.to_string())?;

        // Ask the Windows audio engine for a predictable interleaved Float32
        // layout. Shared-mode autoconversion handles the endpoint's native
        // rate/format, keeping the conversion contract hardware-independent —
        // which is also why a new endpoint needs no new resampler rate.
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

        unsafe { client.Start() }.map_err(|e| format!("start WASAPI loopback: {e}"))?;
        Ok(Loopback {
            client,
            capture,
            event,
            bytes_per_frame,
            endpoint_id,
        })
    }

    /// Drain every packet the endpoint currently holds, emitting 16 kHz PCM.
    fn drain_packets(
        stream: &Loopback,
        channels: usize,
        resampler: &mut Resampler,
        app: &tauri::AppHandle,
    ) -> Result<(), PacketError> {
        loop {
            let frames = unsafe { stream.capture.GetNextPacketSize() }
                .map_err(|e| classify(e, "size WASAPI loopback packet"))?;
            if frames == 0 {
                return Ok(());
            }

            let mut data = ptr::null_mut();
            let mut read_frames = 0_u32;
            let mut flags = 0_u32;
            unsafe {
                stream
                    .capture
                    .GetBuffer(&mut data, &mut read_frames, &mut flags, None, None)
            }
            .map_err(|e| classify(e, "read WASAPI loopback packet"))?;

            // Windows may return a null data pointer for SILENT packets. Build
            // the mono data before releasing the packet, but never form a slice
            // from that null pointer.
            let mono_result = if read_frames == 0 {
                Ok(Vec::new())
            } else if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                Ok(vec![0.0; read_frames as usize])
            } else if data.is_null() {
                Err("WASAPI returned a null non-silent buffer".into())
            } else {
                match (read_frames as usize).checked_mul(stream.bytes_per_frame) {
                    Some(byte_len) => {
                        let bytes = unsafe { slice::from_raw_parts(data, byte_len) };
                        downmix_interleaved_f32(bytes, channels)
                    }
                    None => Err("WASAPI packet size overflow".into()),
                }
            };
            unsafe { stream.capture.ReleaseBuffer(read_frames) }
                .map_err(|e| classify(e, "release WASAPI loopback packet"))?;
            let mono = mono_result.map_err(PacketError::Fatal)?;

            let mut pcm = Vec::with_capacity(mono.len() * 2);
            resampler.process(&mono, &mut pcm);
            if !pcm.is_empty() {
                let _ = app.emit("system-audio-data", base64_encode(&pcm));
            }
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

            let source_rate = 48_000_u32;
            let channels = 2_usize;
            // Only the first open reports failure to the recorder: once
            // recording is under way, a device that briefly cannot be opened is
            // retried rather than treated as the end of system audio.
            let mut stream = Some(open_default_loopback(&enumerator, source_rate, channels)?);
            let mut resampler = Resampler::new(source_rate as f64, 16_000.0);

            if ready.send(Ok(())).is_err() {
                return Ok(());
            }

            let mut next_endpoint_check = Instant::now() + ENDPOINT_POLL;
            loop {
                match stop.try_recv() {
                    Ok(()) | Err(TryRecvError::Disconnected) => break,
                    Err(TryRecvError::Empty) => {}
                }

                // Follow the default render endpoint. The activated client stays
                // bound to the device it was opened on, so an output switch
                // mid-recording is only picked up by re-reading it here.
                if Instant::now() >= next_endpoint_check {
                    next_endpoint_check = Instant::now() + ENDPOINT_POLL;
                    let current = default_endpoint(&enumerator).ok().map(|(_, id)| id);
                    let stale = match (stream.as_ref(), current.as_ref()) {
                        (Some(active), Some(id)) => *id != active.endpoint_id,
                        (None, Some(_)) => true,
                        // No usable endpoint at all (every output removed):
                        // keep what we have and look again on the next poll.
                        (_, None) => false,
                    };
                    if stale {
                        // Stop the old stream before activating the new one.
                        drop(stream.take());
                        match open_default_loopback(&enumerator, source_rate, channels) {
                            Ok(fresh) => {
                                // The engine converts every endpoint to
                                // `source_rate`, so the rate is unchanged; the
                                // resampler is still replaced so the new device
                                // doesn't interpolate from the old one's last
                                // sample.
                                resampler = Resampler::new(source_rate as f64, 16_000.0);
                                stream = Some(fresh);
                            }
                            Err(e) => {
                                eprintln!(
                                    "windows system-audio: reopening the default output failed: {e}"
                                );
                            }
                        }
                    }
                }

                let Some(active) = stream.as_ref() else {
                    // Nothing to read until a reopen succeeds. The Vue mixer
                    // zero-fills this span, as it does for a quiet device. Wait
                    // on the stop channel so stopping stays immediate.
                    match stop.recv_timeout(ENDPOINT_POLL) {
                        Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                        Err(RecvTimeoutError::Timeout) => continue,
                    }
                };

                // Timeouts are expected while the render endpoint is quiet;
                // the Vue mixer fills those spans with zeroes.
                match unsafe { WaitForSingleObject(active.event.0, 200) } {
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

                match drain_packets(active, channels, &mut resampler, &app) {
                    Ok(()) => {}
                    // The endpoint went away underneath us. Drop it and reopen
                    // on the next pass instead of ending system audio for the
                    // rest of the recording.
                    Err(PacketError::Invalidated) => {
                        eprintln!("windows system-audio: endpoint invalidated; reopening");
                        drop(stream.take());
                        next_endpoint_check = Instant::now();
                    }
                    Err(PacketError::Fatal(e)) => return Err(e),
                }
            }
            Ok(())
        })();

        if let Err(error) = result {
            let _ = ready.send(Err(error.clone()));
            eprintln!("windows system-audio capture stopped: {error}");
        }
    }

    pub fn start(app: tauri::AppHandle) -> Result<(), String> {
        let mut guard = CAPTURE.lock().map_err(|e| e.to_string())?;
        reap_finished_capture(&mut guard);
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
                // A hung COM call cannot be cancelled safely. Dropping the
                // JoinHandle detaches the worker so this five-second boundary
                // remains meaningful; the stop signal makes it exit if the
                // initialization call eventually returns.
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

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn reaps_finished_capture_but_preserves_active_worker() {
            let (active_stop, active_stop_rx) = mpsc::channel();
            let active_thread = std::thread::spawn(move || {
                let _ = active_stop_rx.recv();
            });
            let mut state = Some(CaptureState {
                stop: active_stop,
                thread: active_thread,
            });

            reap_finished_capture(&mut state);
            assert!(state.is_some());
            let active = state.take().unwrap();
            let _ = active.stop.send(());
            active.thread.join().unwrap();

            let (finished_stop, _finished_stop_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            let finished_thread = std::thread::spawn(move || {
                let _ = done_tx.send(());
            });
            done_rx.recv_timeout(Duration::from_secs(1)).unwrap();
            while !finished_thread.is_finished() {
                std::thread::yield_now();
            }
            state = Some(CaptureState {
                stop: finished_stop,
                thread: finished_thread,
            });

            reap_finished_capture(&mut state);
            assert!(state.is_none());
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use crate::audio_util::{
        base64_encode, downmix_to_mono, get_property, is_supported_pcm_format, ns, prop_address,
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
        AudioHardwareDestroyProcessTap, AudioObjectAddPropertyListener,
        AudioObjectPropertyAddress, AudioObjectRemovePropertyListener,
        CATapDescription, CATapMuteBehavior,
    };
    use objc2_core_audio_types::{
        AudioBufferList,
        AudioStreamBasicDescription, AudioTimeStamp,
    };
    use objc2_core_foundation::{CFDictionary, CFRetained, CFString};
    use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSObject, NSString};
    use std::ffi::c_void;
    use std::ptr::{self, NonNull};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::Duration;
    use tauri::Emitter;

    /// How long to let Core Audio settle after a change notification. One
    /// headset transition fires a burst of them, and rebuilding on the first
    /// would race the HAL while it is still re-publishing the new device.
    const SETTLE: Duration = Duration::from_millis(250);
    /// A device that is mid-transition can refuse a fresh tap for a moment.
    /// Failing outright would silently drop system audio for the rest of the
    /// meeting, so retry a couple of times before giving up.
    const REBUILD_ATTEMPTS: u32 = 3;
    const REBUILD_BACKOFF: Duration = Duration::from_millis(300);

    /// The device- and format-dependent inputs baked into a live capture: the
    /// UID is the aggregate device's clock sub-device, the rate is what the
    /// resampler converts from. Neither can be changed in place, so capture is
    /// rebuilt whenever the current values stop matching these.
    #[derive(Clone, Debug)]
    struct TapConfig {
        output_uid: String,
        src_rate: f64,
    }

    impl TapConfig {
        /// Sample rates are compared with a 1 Hz tolerance. Real transitions
        /// move in kilohertz (48 kHz A2DP → 16/24 kHz HFP); a last-bit
        /// difference is not a transition, and acting on one would matter
        /// because each rebuild itself re-fires these notifications.
        fn differs_from(&self, other: &TapConfig) -> bool {
            self.output_uid != other.output_uid || (self.src_rate - other.src_rate).abs() >= 1.0
        }
    }

    /// Which Core Audio property listeners a capture actually registered, so
    /// teardown removes exactly those. Registration is best-effort: losing a
    /// listener costs the follow-the-device behaviour, not the recording.
    #[derive(Clone, Copy)]
    struct Listeners {
        default_output: bool,
        tap_format: bool,
    }

    /// Live capture resources, torn down in reverse creation order on stop.
    /// The Core Audio handles are plain integers; the IO block is owned by
    /// Core Audio (retained via `Block_copy` inside
    /// `AudioDeviceCreateIOProcIDWithBlock` and released by
    /// `AudioDeviceDestroyIOProcID`), so we don't need to keep a !Send
    /// `RcBlock` in this cross-thread state. The `AppHandle` is kept so the
    /// watcher thread can rebuild capture without the frontend re-invoking.
    struct CaptureState {
        tap_id: AudioObjectID,
        aggregate_id: AudioObjectID,
        proc_id: AudioDeviceIOProcID,
        config: TapConfig,
        listeners: Listeners,
        app: tauri::AppHandle,
    }

    static CAPTURE: Mutex<Option<CaptureState>> = Mutex::new(None);

    /// Wakes the watcher thread. Held in a `OnceLock` rather than in
    /// `CaptureState` so the listener callback never touches the `CAPTURE`
    /// mutex: `AudioObjectRemovePropertyListener` blocks until an in-flight
    /// callback returns, and teardown calls it while holding that lock.
    static CHANGE_TX: OnceLock<Sender<()>> = OnceLock::new();

    /// Core Audio property listener. Runs on a HAL notification thread, where
    /// tearing down a tap or aggregate device would deadlock against the very
    /// reconfiguration that triggered it — so this only signals the watcher.
    unsafe extern "C-unwind" fn on_audio_change(
        _object: AudioObjectID,
        _address_count: u32,
        _addresses: NonNull<AudioObjectPropertyAddress>,
        _client_data: *mut c_void,
    ) -> i32 {
        if let Some(tx) = CHANGE_TX.get() {
            let _ = tx.send(());
        }
        0
    }

    /// Start (once) the thread that rebuilds capture after a device change and
    /// return the sender the listeners signal on.
    fn change_signaler() -> &'static Sender<()> {
        CHANGE_TX.get_or_init(|| {
            let (tx, rx) = mpsc::channel();
            if let Err(e) = std::thread::Builder::new()
                .name("oats-system-audio-watch".into())
                .spawn(move || {
                    while wait_for_change(&rx, SETTLE) {
                        rebuild_capture();
                    }
                })
            {
                // The receiver is dropped with the failed spawn, so signals
                // become no-ops: capture still works, it just stops following
                // the output device.
                eprintln!("system-audio device watcher unavailable: {e}");
            }
            tx
        })
    }

    /// Block until a change notification arrives, then swallow the burst behind
    /// it so one device transition causes one rebuild. Returns `false` when the
    /// sender is gone, which ends the watcher loop.
    fn wait_for_change(rx: &Receiver<()>, settle: Duration) -> bool {
        if rx.recv().is_err() {
            return false;
        }
        std::thread::sleep(settle);
        while rx.try_recv().is_ok() {}
        true
    }

    /// Register `on_audio_change` for one property. Returns whether it took.
    unsafe fn add_listener(object: AudioObjectID, selector: u32) -> bool {
        let addr = prop_address(selector, kAudioObjectPropertyScopeGlobal);
        let status = unsafe {
            AudioObjectAddPropertyListener(
                object,
                NonNull::from(&addr),
                Some(on_audio_change),
                ptr::null_mut(),
            )
        };
        if status != 0 {
            eprintln!("AudioObjectAddPropertyListener({selector}) failed: {status}");
        }
        status == 0
    }

    unsafe fn remove_listener(object: AudioObjectID, selector: u32) -> Result<(), String> {
        let addr = prop_address(selector, kAudioObjectPropertyScopeGlobal);
        let status = unsafe {
            AudioObjectRemovePropertyListener(
                object,
                NonNull::from(&addr),
                Some(on_audio_change),
                ptr::null_mut(),
            )
        };
        if status != 0 {
            return Err(format!(
                "AudioObjectRemovePropertyListener({selector}) failed: {status}"
            ));
        }
        Ok(())
    }

    /// The default output device's UID paired with the rate `tap_id` is
    /// currently delivering — the live counterpart of `CaptureState::config`.
    unsafe fn current_config(tap_id: AudioObjectID) -> Result<TapConfig, String> {
        let output_id = unsafe { default_output_device()? };
        let output_uid = unsafe { device_uid(output_id)? };
        let asbd: AudioStreamBasicDescription = unsafe {
            get_property(tap_id, kAudioTapPropertyFormat, kAudioObjectPropertyScopeGlobal)?
        };
        Ok(TapConfig {
            output_uid,
            src_rate: asbd.mSampleRate,
        })
    }

    unsafe fn default_output_device() -> Result<AudioObjectID, String> {
        unsafe {
            get_property(
                kAudioObjectSystemObject as AudioObjectID,
                kAudioHardwarePropertyDefaultSystemOutputDevice,
                kAudioObjectPropertyScopeGlobal,
            )
        }
    }

    unsafe fn device_uid(device_id: AudioObjectID) -> Result<String, String> {
        // Core Audio can return status 0 with a null/absent UID for some
        // virtual or aggregate output devices. Guard the pointer instead of
        // unwrapping: a null here would panic, and handing a non-owned null to
        // `CFRetained::from_raw` (which assumes a +1 retained object) is the
        // start of a refcount/UAF bug, not just a crash.
        let ptr = unsafe {
            get_property::<*const CFString>(
                device_id,
                kAudioDevicePropertyDeviceUID,
                kAudioObjectPropertyScopeGlobal,
            )?
        };
        match NonNull::new(ptr as *mut CFString) {
            Some(nn) => Ok(unsafe { CFRetained::from_raw(nn) }.to_string()),
            None => Err("default output device has no UID".into()),
        }
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
        *guard = Some(unsafe { build_capture(app) }?);
        Ok(())
    }

    /// Build a live capture bound to whatever output device is current: tap →
    /// aggregate device → IO proc, plus the listeners that report when that
    /// binding goes stale.
    unsafe fn build_capture(app: tauri::AppHandle) -> Result<CaptureState, String> {
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
            let output_uid_str = match default_output_device().and_then(|id| device_uid(id)) {
                Ok(uid) => uid,
                Err(e) => {
                    AudioHardwareDestroyProcessTap(tap_id);
                    return Err(e);
                }
            };
            let output_uid = NSString::from_str(&output_uid_str);

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

            // 5. IO block: downmix → resample → emit. The resampler is built
            // from the rate read above and is discarded with this capture, so a
            // later rate change never converts from a stale source rate.
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

            // 6. Follow the device. Both the aggregate's clock sub-device and
            // the resampler's source rate are fixed at this point, so a later
            // output switch (earbuds → speakers) or format switch (a headset
            // dropping to HFP when another app takes the mic) is only
            // recoverable by rebuilding. Start the watcher before registering,
            // so the first notification already has somewhere to go.
            let _ = change_signaler();
            let listeners = Listeners {
                default_output: add_listener(
                    kAudioObjectSystemObject as AudioObjectID,
                    kAudioHardwarePropertyDefaultSystemOutputDevice,
                ),
                tap_format: add_listener(tap_id, kAudioTapPropertyFormat),
            };

            Ok(CaptureState {
                tap_id,
                aggregate_id,
                proc_id,
                config: TapConfig {
                    output_uid: output_uid_str,
                    src_rate,
                },
                listeners,
                app,
            })
        }
    }

    /// Tear down in reverse creation order. Every step is attempted even if an
    /// earlier one fails, so a single failure doesn't leak the remaining
    /// resources; the collected statuses are returned for the caller to report.
    unsafe fn teardown(state: CaptureState) -> Vec<String> {
        let mut errors: Vec<String> = Vec::new();
        unsafe {
            // Listeners first, so nothing can signal a rebuild of the capture
            // that this teardown is dismantling.
            if state.listeners.default_output {
                if let Err(e) = remove_listener(
                    kAudioObjectSystemObject as AudioObjectID,
                    kAudioHardwarePropertyDefaultSystemOutputDevice,
                ) {
                    errors.push(e);
                }
            }
            if state.listeners.tap_format {
                if let Err(e) = remove_listener(state.tap_id, kAudioTapPropertyFormat) {
                    errors.push(e);
                }
            }
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
        errors
    }

    /// Rebind capture to the current output device, on the watcher thread.
    ///
    /// The resampler is deliberately not carried over: its filter state belongs
    /// to the old source rate. The recording keeps its timeline — the frontend
    /// zero-fills the short gap this leaves, which is what a device switch
    /// sounds like anyway.
    fn rebuild_capture() {
        let Ok(mut guard) = CAPTURE.lock() else {
            return;
        };
        // Notifications fire for reasons that don't concern this capture —
        // including the rebuilds it performs itself — so act only on a binding
        // that has genuinely changed. A tap whose format can no longer be read
        // is broken and rebuilt regardless.
        let change = match guard.as_ref() {
            // Not recording: nothing to rebind.
            None => return,
            Some(state) => match unsafe { current_config(state.tap_id) } {
                Ok(latest) if !latest.differs_from(&state.config) => return,
                Ok(latest) => format!(
                    "{} @ {} Hz -> {} @ {} Hz",
                    state.config.output_uid,
                    state.config.src_rate,
                    latest.output_uid,
                    latest.src_rate
                ),
                Err(e) => format!("tap format unreadable ({e})"),
            },
        };
        let Some(state) = guard.take() else {
            return;
        };

        eprintln!("system-audio: output changed, rebuilding capture ({change})");
        let app = state.app.clone();
        let errors = unsafe { teardown(state) };
        if !errors.is_empty() {
            eprintln!("system-audio teardown before rebuild: {}", errors.join("; "));
        }

        for attempt in 1..=REBUILD_ATTEMPTS {
            match unsafe { build_capture(app.clone()) } {
                Ok(rebuilt) => {
                    *guard = Some(rebuilt);
                    return;
                }
                Err(e) if attempt == REBUILD_ATTEMPTS => {
                    eprintln!("system-audio capture stopped after an output change: {e}");
                }
                Err(e) => {
                    eprintln!("system-audio rebuild attempt {attempt} failed: {e}");
                    std::thread::sleep(REBUILD_BACKOFF);
                }
            }
        }
    }

    pub fn stop() -> Result<(), String> {
        let mut guard = CAPTURE.lock().map_err(|e| e.to_string())?;
        if let Some(state) = guard.take() {
            let errors = unsafe { teardown(state) };
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

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::sync::mpsc;

        fn config(uid: &str, rate: f64) -> TapConfig {
            TapConfig {
                output_uid: uid.to_string(),
                src_rate: rate,
            }
        }

        #[test]
        fn rebuilds_when_the_tap_sample_rate_changes() {
            // A Bluetooth headset dropping from A2DP to HFP: same device, new rate.
            assert!(config("BT-headset", 16_000.0).differs_from(&config("BT-headset", 48_000.0)));
        }

        #[test]
        fn rebuilds_when_the_default_output_device_changes() {
            // Earbuds → speakers: the aggregate's clock sub-device is now stale.
            assert!(config("BuiltInSpeaker", 48_000.0).differs_from(&config("BT-headset", 48_000.0)));
        }

        #[test]
        fn does_not_rebuild_when_the_configuration_is_unchanged() {
            assert!(!config("BT-headset", 48_000.0).differs_from(&config("BT-headset", 48_000.0)));
        }

        #[test]
        fn does_not_rebuild_on_sub_hertz_sample_rate_jitter() {
            // Every rebuild re-notifies, so treating a last-bit difference as a
            // real change would tear the tap down in a loop.
            assert!(!config("BT-headset", 48_000.000_000_1)
                .differs_from(&config("BT-headset", 48_000.0)));
        }

        #[test]
        fn coalesces_a_burst_of_notifications_into_one_rebuild() {
            // One headset transition fires several Core Audio notifications.
            let (tx, rx) = mpsc::channel();
            for _ in 0..5 {
                tx.send(()).unwrap();
            }
            assert!(wait_for_change(&rx, Duration::from_millis(10)));
            assert!(
                rx.try_recv().is_err(),
                "the burst queued behind the first signal should be drained"
            );
        }

        #[test]
        fn stops_waiting_when_the_signal_sender_is_gone() {
            let (tx, rx) = mpsc::channel::<()>();
            drop(tx);
            assert!(!wait_for_change(&rx, Duration::from_millis(10)));
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
