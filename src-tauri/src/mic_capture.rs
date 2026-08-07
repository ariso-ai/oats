//! Native microphone capture. macOS uses a plain Core Audio HAL input IO proc;
//! Windows uses WASAPI shared mode on the selected capture endpoint.
//!
//! Captures the system's default input device without using Voice-Processing I/O
//! (AUVoiceIO), so it does not trigger macOS audio ducking of system audio.
//!
//! Flow: query the default input device → read its native stream format (input
//! scope) → verify it is 32-bit float LinearPCM (interleaved, or non-interleaved
//! mono) → install an IO block that receives Float32 PCM at the device's native
//! rate → downmix to mono, resample to 44.1 kHz, convert to Int16, and emit as
//! `mic-audio-data` (base64) for the recorder frontend.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrophoneInputDevice {
    pub device_id: String,
    pub label: String,
    pub is_default: bool,
}

const MAX_DEVICE_ID_LEN: usize = 2048;

fn validate_device_id(device_id: Option<String>) -> Result<Option<String>, String> {
    match device_id {
        None => Ok(None),
        Some(id) if id.is_empty() => Err("microphone endpoint ID must not be empty".into()),
        Some(id) if id.len() > MAX_DEVICE_ID_LEN => {
            Err("microphone endpoint ID is too long".into())
        }
        Some(id) if id.contains('\0') => {
            Err("microphone endpoint ID contains an invalid character".into())
        }
        Some(id) => Ok(Some(id)),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    use super::MicrophoneInputDevice;

    pub fn list() -> Result<Vec<MicrophoneInputDevice>, String> {
        Ok(Vec::new())
    }
    pub fn start(_app: tauri::AppHandle, _device_id: Option<String>) -> Result<(), String> {
        Err("Microphone capture is not supported on this platform".into())
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

#[cfg(target_os = "windows")]
mod imp {
    use super::MicrophoneInputDevice;
    use crate::audio_util::{Resampler, base64_encode, downmix_interleaved_f32};
    use std::ptr;
    use std::slice;
    use std::sync::Mutex;
    use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
    use std::thread::JoinHandle;
    use std::time::Duration;
    use tauri::Emitter;
    use wasapi::{
        DeviceEnumerator, DeviceState, Direction, SampleType, WaveFormat, deinitialize,
        initialize_mta,
    };
    use windows::Win32::Foundation::{
        CloseHandle, HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows::Win32::Media::Audio::{
        AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
        DEVICE_STATE_ACTIVE, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator,
        IMMEndpoint, MMDeviceEnumerator, eCapture, eConsole,
    };
    use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};
    use windows::Win32::System::Threading::{CreateEventA, WaitForSingleObject};
    use windows::core::{Interface, PCSTR, PCWSTR};

    struct CaptureState {
        stop: Sender<()>,
        thread: JoinHandle<()>,
    }

    static CAPTURE: Mutex<Option<CaptureState>> = Mutex::new(None);
    const STARTUP_ATTEMPTS: usize = 3;
    const STARTUP_SIGNAL_TIMEOUT: Duration = Duration::from_secs(3);
    const TRANSIENT_RETRY_DELAY: Duration = Duration::from_millis(250);

    fn pcm_has_signal(pcm: &[u8]) -> bool {
        pcm.chunks_exact(2)
            .any(|sample| sample[0] != 0 || sample[1] != 0)
    }

    fn is_transient_startup_error(error: &str) -> bool {
        // AUDCLNT_E_DEVICE_INVALIDATED is expected while a Bluetooth endpoint
        // switches from its stereo playback profile to hands-free capture.
        error.contains("0x88890004")
    }

    fn capture_layout(format: &WaveFormat) -> Result<(usize, usize, u32), String> {
        if format.get_subformat().ok() != Some(SampleType::Float)
            || format.get_bitspersample() != 32
        {
            return Err("Windows microphone mix format is not 32-bit float PCM".into());
        }
        let source_rate = format.get_samplespersec();
        let channels = format.get_nchannels() as usize;
        if source_rate == 0 || channels == 0 {
            return Err("Windows microphone reported an invalid mix format".into());
        }
        let bytes_per_frame = format.get_blockalign() as usize;
        if bytes_per_frame != channels * std::mem::size_of::<f32>() {
            return Err("Windows microphone reported an unsupported float PCM layout".into());
        }
        Ok((bytes_per_frame, channels, source_rate))
    }

    struct ComGuard;

    impl Drop for ComGuard {
        fn drop(&mut self) {
            deinitialize();
        }
    }

    struct EventHandle(HANDLE);

    impl Drop for EventHandle {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

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

    fn initialize_com() -> Result<ComGuard, String> {
        let result = initialize_mta();
        if result.is_err() {
            return Err(format!("initialize Windows audio: {result:?}"));
        }
        Ok(ComGuard)
    }

    pub fn list() -> Result<Vec<MicrophoneInputDevice>, String> {
        let _com = initialize_com()?;
        let enumerator = DeviceEnumerator::new().map_err(|error| error.to_string())?;
        let default_id = enumerator
            .get_default_device(&Direction::Capture)
            .and_then(|device| device.get_id())
            .ok();
        let collection = enumerator
            .get_device_collection(&Direction::Capture)
            .map_err(|error| error.to_string())?;
        let mut devices = Vec::new();
        for device in &collection {
            let Ok(device) = device else {
                continue;
            };
            if device.get_state().ok() != Some(DeviceState::Active) {
                continue;
            }
            let Ok(device_id) = device.get_id() else {
                continue;
            };
            let Ok(friendly_name) = device.get_friendlyname() else {
                continue;
            };
            let label = friendly_name.trim().to_string();
            devices.push(MicrophoneInputDevice {
                is_default: default_id.as_deref() == Some(device_id.as_str()),
                device_id,
                label: if label.is_empty() {
                    "Microphone".to_string()
                } else {
                    label
                },
            });
        }
        devices.sort_by(|left, right| {
            left.label
                .to_lowercase()
                .cmp(&right.label.to_lowercase())
                .then_with(|| left.device_id.cmp(&right.device_id))
        });
        Ok(devices)
    }

    fn select_device(device_id: Option<&str>) -> Result<IMMDevice, String> {
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
                .map_err(|error| format!("enumerate Windows microphones: {error}"))?;
        let device = match device_id {
            None => unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eConsole) }
                .map_err(|error| format!("no default Windows microphone: {error}"))?,
            Some(id) => {
                let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
                unsafe { enumerator.GetDevice(PCWSTR(wide.as_ptr())) }
                    .map_err(|_| "selected microphone is unavailable".to_string())?
            }
        };
        let state = unsafe { device.GetState() }
            .map_err(|error| format!("read Windows microphone state: {error}"))?;
        if state != DEVICE_STATE_ACTIVE {
            return Err("selected microphone is unavailable".into());
        }
        let endpoint: IMMEndpoint = device
            .cast()
            .map_err(|error| format!("inspect Windows microphone endpoint: {error}"))?;
        let direction = unsafe { endpoint.GetDataFlow() }
            .map_err(|error| format!("inspect Windows microphone direction: {error}"))?;
        if direction != eCapture {
            return Err("selected audio endpoint is not a microphone".into());
        }
        Ok(device)
    }

    fn initialize_client(
        device: &IMMDevice,
    ) -> Result<
        (
            IAudioClient,
            EventHandle,
            IAudioCaptureClient,
            usize,
            usize,
            u32,
        ),
        String,
    > {
        let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }
            .map_err(|error| format!("open Windows microphone: {error}"))?;
        let raw_format = unsafe { client.GetMixFormat() }
            .map_err(|error| format!("read Windows microphone mix format: {error}"))?;
        let format_result = if raw_format.is_null() {
            Err("Windows microphone returned a null mix format".to_string())
        } else {
            WaveFormat::parse(unsafe { &*raw_format })
                .map_err(|error| format!("parse Windows microphone mix format: {error}"))
        };
        if !raw_format.is_null() {
            unsafe { CoTaskMemFree(Some(raw_format.cast())) };
        }
        let format = format_result?;
        let (bytes_per_frame, channels, source_rate) = capture_layout(&format)?;
        let mut min_period = 0_i64;
        unsafe { client.GetDevicePeriod(None, Some(&mut min_period)) }
            .map_err(|error| format!("read Windows microphone period: {error}"))?;
        let stream_flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
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
        .map_err(|error| format!("initialize Windows microphone capture: {error}"))?;
        let event = EventHandle(
            unsafe { CreateEventA(None, false, false, PCSTR::null()) }
                .map_err(|error| format!("create Windows microphone event: {error}"))?,
        );
        unsafe { client.SetEventHandle(event.0) }
            .map_err(|error| format!("register Windows microphone event: {error}"))?;
        let capture: IAudioCaptureClient = unsafe { client.GetService() }
            .map_err(|error| format!("open Windows microphone capture buffer: {error}"))?;
        Ok((
            client,
            event,
            capture,
            bytes_per_frame,
            channels,
            source_rate,
        ))
    }

    fn run_capture(
        app: tauri::AppHandle,
        device_id: Option<String>,
        ready: Sender<Result<(), String>>,
        stop: Receiver<()>,
    ) {
        let mut ready = Some(ready);
        let result = (|| -> Result<(), String> {
            let _com = initialize_com()?;
            let device = select_device(device_id.as_deref())?;
            let (client, event, capture, bytes_per_frame, channels, source_rate) =
                initialize_client(&device)?;
            let mut resampler = Resampler::new(source_rate as f64, 44_100.0);

            unsafe { client.Start() }
                .map_err(|error| format!("start Windows microphone capture: {error}"))?;

            let capture_result = (|| -> Result<(), String> {
                loop {
                    match stop.try_recv() {
                        Ok(()) | Err(TryRecvError::Disconnected) => break,
                        Err(TryRecvError::Empty) => {}
                    }
                    match unsafe { WaitForSingleObject(event.0, 200) } {
                        WAIT_OBJECT_0 => {}
                        WAIT_TIMEOUT => continue,
                        WAIT_FAILED => {
                            return Err("wait for Windows microphone packet failed".into());
                        }
                        status => {
                            return Err(format!(
                                "unexpected Windows microphone wait status: {}",
                                status.0
                            ));
                        }
                    }

                    loop {
                        let frames = unsafe { capture.GetNextPacketSize() }
                            .map_err(|error| format!("query Windows microphone packet: {error}"))?;
                        if frames == 0 {
                            break;
                        }
                        let mut data = ptr::null_mut();
                        let mut read_frames = 0_u32;
                        let mut flags = 0_u32;
                        unsafe {
                            capture.GetBuffer(&mut data, &mut read_frames, &mut flags, None, None)
                        }
                        .map_err(|error| format!("read Windows microphone packet: {error}"))?;

                        let mono_result = if read_frames == 0 {
                            Ok(Vec::new())
                        } else if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                            Ok(vec![0.0; read_frames as usize])
                        } else if data.is_null() {
                            Err("Windows microphone returned a null non-silent buffer".into())
                        } else {
                            match (read_frames as usize).checked_mul(bytes_per_frame) {
                                Some(byte_len) => {
                                    let bytes = unsafe { slice::from_raw_parts(data, byte_len) };
                                    downmix_interleaved_f32(bytes, channels)
                                }
                                None => Err("Windows microphone packet size overflow".into()),
                            }
                        };
                        unsafe { capture.ReleaseBuffer(read_frames) }.map_err(|error| {
                            format!("release Windows microphone packet: {error}")
                        })?;
                        let mono = mono_result?;
                        let mut pcm = Vec::with_capacity(mono.len() * 2);
                        resampler.process(&mono, &mut pcm);
                        if !pcm.is_empty() {
                            let has_signal = pcm_has_signal(&pcm);
                            if ready.is_some() && !has_signal {
                                continue;
                            }
                            if let Some(ready_tx) = ready.take() {
                                if ready_tx.send(Ok(())).is_err() {
                                    return Ok(());
                                }
                            }
                            let _ = app.emit("mic-audio-data", base64_encode(&pcm));
                        }
                    }
                }
                Ok(())
            })();

            let stop_result = unsafe { client.Stop() }
                .map_err(|error| format!("stop Windows microphone capture: {error}"));
            capture_result.and(stop_result)
        })();

        if let Err(error) = result {
            if let Some(ready_tx) = ready.take() {
                let _ = ready_tx.send(Err(error.clone()));
            }
            let _ = app.emit("microphone-capture-error", error);
        }
    }

    pub fn start(app: tauri::AppHandle, device_id: Option<String>) -> Result<(), String> {
        let mut guard = CAPTURE.lock().map_err(|error| error.to_string())?;
        reap_finished_capture(&mut guard);
        if guard.is_some() {
            return Err("Microphone capture already running".into());
        }

        for attempt in 1..=STARTUP_ATTEMPTS {
            let (stop_tx, stop_rx) = mpsc::channel();
            let (ready_tx, ready_rx) = mpsc::channel();
            let capture_app = app.clone();
            let capture_device_id = device_id.clone();
            let thread = std::thread::Builder::new()
                .name("oats-wasapi-microphone".into())
                .spawn(move || run_capture(capture_app, capture_device_id, ready_tx, stop_rx))
                .map_err(|error| format!("start Windows microphone thread: {error}"))?;
            match ready_rx.recv_timeout(STARTUP_SIGNAL_TIMEOUT) {
                Ok(Ok(())) => {
                    *guard = Some(CaptureState {
                        stop: stop_tx,
                        thread,
                    });
                    return Ok(());
                }
                Ok(Err(error)) => {
                    let _ = thread.join();
                    if is_transient_startup_error(&error) {
                        if attempt == STARTUP_ATTEMPTS {
                            return Err("selected microphone is unavailable".into());
                        }
                        std::thread::sleep(TRANSIENT_RETRY_DELAY);
                        continue;
                    }
                    return Err(error);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = thread.join();
                    return Err("Windows microphone stopped during startup".into());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let _ = stop_tx.send(());
                    let _ = thread.join();
                    if attempt == STARTUP_ATTEMPTS {
                        return Err(
                            "Windows microphone connected but delivered no audio signal".into()
                        );
                    }
                }
            }
        }
        unreachable!("Windows microphone startup attempts are nonzero")
    }

    pub fn stop() -> Result<(), String> {
        let state = CAPTURE.lock().map_err(|error| error.to_string())?.take();
        let Some(state) = state else {
            return Ok(());
        };
        let _ = state.stop.send(());
        state
            .thread
            .join()
            .map_err(|_| "Windows microphone thread panicked".to_string())
    }

    fn probe() -> bool {
        let Ok(_com) = initialize_com() else {
            return false;
        };
        let Ok(device) = select_device(None) else {
            return false;
        };
        let Ok((client, _event, _capture, _bytes_per_frame, _channels, _source_rate)) =
            initialize_client(&device)
        else {
            return false;
        };
        if unsafe { client.Start() }.is_err() {
            return false;
        }
        unsafe { client.Stop() }.is_ok()
    }

    pub fn check_permission() -> bool {
        probe()
    }

    pub fn request_permission() -> bool {
        probe()
    }

    #[cfg(test)]
    mod tests {
        use super::{capture_layout, is_transient_startup_error, pcm_has_signal};
        use wasapi::{SampleType, WaveFormat};

        #[test]
        fn detects_nonzero_pcm_signal() {
            assert!(!pcm_has_signal(&[]));
            assert!(!pcm_has_signal(&[0, 0, 0, 0]));
            assert!(pcm_has_signal(&[0, 0, 1, 0]));
            assert!(pcm_has_signal(&[0, 0, 0, 0x80]));
        }

        #[test]
        fn accepts_native_bluetooth_hands_free_mix_format() {
            let format = WaveFormat::new(32, 32, &SampleType::Float, 16_000, 1, None);
            assert_eq!(capture_layout(&format).unwrap(), (4, 1, 16_000));
        }

        #[test]
        fn preserves_native_channel_and_sample_rate_layout() {
            let format = WaveFormat::new(32, 32, &SampleType::Float, 48_000, 2, None);
            assert_eq!(capture_layout(&format).unwrap(), (8, 2, 48_000));
        }

        #[test]
        fn rejects_integer_or_invalid_native_mix_formats() {
            let integer = WaveFormat::new(16, 16, &SampleType::Int, 16_000, 1, None);
            assert!(capture_layout(&integer).is_err());
            let zero_rate = WaveFormat::new(32, 32, &SampleType::Float, 0, 1, None);
            assert!(capture_layout(&zero_rate).is_err());
        }

        #[test]
        fn retries_only_windows_device_invalidation_errors() {
            assert!(is_transient_startup_error(
                "query Windows microphone packet: 0x88890004"
            ));
            assert!(!is_transient_startup_error(
                "initialize Windows microphone capture: access denied"
            ));
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::MicrophoneInputDevice;
    use crate::audio_util::{
        AudioObjectID, Resampler, base64_encode, downmix_to_mono, get_property,
    };
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
    use objc2_core_audio::{
        AudioDeviceCreateIOProcIDWithBlock, AudioDeviceDestroyIOProcID, AudioDeviceIOProcID,
        AudioDeviceStart, AudioDeviceStop, kAudioDevicePropertyStreamFormat,
        kAudioHardwarePropertyDefaultInputDevice, kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyScopeInput, kAudioObjectSystemObject,
    };
    use objc2_core_audio_types::{
        AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp, kAudioFormatFlagIsBigEndian,
        kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked, kAudioFormatLinearPCM,
        kLinearPCMFormatFlagIsNonInterleaved,
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

    pub fn list() -> Result<Vec<MicrophoneInputDevice>, String> {
        Ok(Vec::new())
    }

    pub fn start(app: tauri::AppHandle, _device_id: Option<String>) -> Result<(), String> {
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

            *guard = Some(CaptureState {
                device_id: input_id,
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
                        audio_type, &*handler,
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
/// IO proc on macOS (not Voice-Processing I/O) and WASAPI shared mode on
/// Windows. `device_id` is a native Windows endpoint ID; `None` uses the
/// current system default.
#[tauri::command]
pub fn start_microphone_capture(
    app: tauri::AppHandle,
    device_id: Option<String>,
) -> Result<(), String> {
    imp::start(app, validate_device_id(device_id)?)
}

/// List active native Windows capture endpoints. Other platforms return an
/// empty list because their Settings UI does not expose endpoint selection.
#[tauri::command]
pub async fn list_microphone_input_devices() -> Result<Vec<MicrophoneInputDevice>, String> {
    tokio::task::spawn_blocking(imp::list)
        .await
        .map_err(|error| format!("microphone enumeration task failed: {error}"))?
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

#[cfg(test)]
mod common_tests {
    use super::*;

    #[test]
    fn endpoint_id_validation_accepts_native_ids_and_default() {
        assert_eq!(validate_device_id(None).unwrap(), None);
        assert_eq!(
            validate_device_id(Some("{0.0.1.00000000}.{endpoint}".into())).unwrap(),
            Some("{0.0.1.00000000}.{endpoint}".into())
        );
    }

    #[test]
    fn endpoint_id_validation_rejects_empty_oversized_and_nul() {
        assert!(validate_device_id(Some(String::new())).is_err());
        assert!(validate_device_id(Some("x".repeat(MAX_DEVICE_ID_LEN + 1))).is_err());
        assert!(validate_device_id(Some("bad\0id".into())).is_err());
    }
}
