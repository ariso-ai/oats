# Native microphone capture — eliminate the startup voice-processing duck

**Issue:** #159 — "Startup audio is too quiet for the first couple of seconds when Oats first starts."

## Problem

When a recording starts in a mode that includes the microphone, the **system-audio
channel is attenuated ~10–50× for the first ~2 seconds**, then jumps to normal level.
The user hears the dip live and the recording captures it, so the start of a meeting /
a speaker intro is barely audible.

### Root cause (confirmed by instrumentation)

The microphone is captured via WKWebView `getUserMedia`, which engages macOS
**Voice-Processing I/O (AUVoiceIO)**. That path *ducks* all "other audio" — including
the system output that oats taps — for ~2 s while it spins up. The Core Audio process
tap then faithfully records the ducked output.

Evidence (system-channel peak, full-scale ≈ 32768), measured on the signed bundle:

| Mode | first ~2 s | after ~2 s | audible dip |
|------|-----------|-----------|-------------|
| mic + system | ~50–380 (ducked) | ~2000–6500 | yes |
| **system only** | ~4000–8000 from frame 1 | ~2000–8000 | **no** |

With the mic off, system audio is full-volume from the first frame — proving the mic,
not the tap, is the trigger. The tap start-gap (~30 ms) and aggregate-device drift
compensation were both investigated and **ruled out** (a drift-compensation change had
no effect on the symptom and will be reverted).

The duck cannot be disabled from JavaScript: `echoCancellation/noiseSuppression/
autoGainControl: false` are already set (the only JS-level levers), but WKWebView owns
its audio session and still ducks (WebKit bug 218012). The ducking-level / bypass APIs
require owning the **native** audio session — which `getUserMedia` does not expose.

## Goal

Capture the microphone via Core Audio directly (a plain HAL input unit, **not**
Voice-Processing I/O) so macOS never ducks system audio. Recording is full-volume on
both channels from t=0 — no delay, nothing dropped, no audible dip.

Non-goals: changing the mp3 encode/interleave format, the waveform visualization, or the
recording settings UI. No browser/non-Tauri mic path (the app is macOS-only).

## Architecture (Approach A: native mic as an event stream)

The frontend stays the encoder and clock; only the mic *source* moves from WebAudio to
native Core Audio events, mirroring how system audio already works.

### 1. New Rust module `src-tauri/src/mic_capture.rs`

Symmetric with `audio_capture.rs`. Captures the **default input device**
(`kAudioHardwarePropertyDefaultInputDevice`) via an IO proc, with a plain HAL input
(no `AUVoiceIO`), so it does not duck other audio.

Per IO callback: downmix interleaved Float32 to mono → resample device-rate → **44.1 kHz**
→ Int16 little-endian → base64 → emit Tauri event **`mic-audio-data`** (same contract
shape as `system-audio-data`, but 44.1 kHz instead of 16 kHz).

Reuses `audio_capture.rs` building blocks, generalized/shared where practical:
- `downmix_to_mono`, `base64_encode`.
- `Resampler` — generalize the hardcoded 16 kHz target into a constructor parameter so
  mic can target 44.1 kHz and system can keep 16 kHz.
- Float32-LinearPCM format validation (`is_supported_tap_format` analogue) — reject any
  non-packed / non-float / non-positive-rate input format before reinterpreting bytes.
- A `CaptureState` + `static Mutex<Option<…>>` lifecycle, torn down in reverse order on
  stop.

Commands (registered in `main.rs` `invoke_handler!`):
- `start_microphone_capture(app)` / `stop_microphone_capture()`.
- `request_microphone_permission()` / `check_microphone_permission()` — via
  `AVCaptureDevice` authorization status / `requestAccess` for `.audio`.
  `NSMicrophoneUsageDescription` is already in `Info.plist`.

Off macOS, the module mirrors `audio_capture.rs`'s no-op `imp` (capture errors / returns
"not supported"); production is macOS-only.

### 2. Frontend `src/composables/useRecorder.ts`

Swap the mic source, keep encode/timing/visualization intact.

- Remove `getUserMedia`, `micStream`, `micSource`, and the WebAudio mic-source branch.
- Add `micAudioBuffer: Int16Array` ring buffer + a `mic-audio-data` listener that
  appends decoded Int16 (44.1 kHz) — exactly parallel to `systemAudioBuffer` /
  `system-audio-data`.
- For `useMic`, `invoke('start_microphone_capture')` (and `stop_microphone_capture` in
  cleanup), parallel to the existing system-audio start/stop.
- Use the **silent graph for all modes**: `processor → analyser → gain(0) → destination`
  (today's system-only graph), which keeps the `ScriptProcessor` firing as the ~93 ms
  encode clock with no real input.
- Per `onaudioprocess` frame: drain `micAudioBuffer` for `frame` samples (44.1 kHz, 1:1)
  and `systemAudioBuffer` via the existing 16 k→44.1 k path; interleave (ch0 mic,
  ch1 system) and lamejs-encode exactly as today. Underflow at the very start zero-fills
  (a few ms; **not** ducked).
- Write the drained mic PCM into the processor `outputBuffer`
  (`e.outputBuffer.getChannelData(0)[i] = micInt16[i] / 0x8000`) — exactly as the
  system-only path writes system PCM today (lines 243–244). With the graph
  `processor → analyser → gain(0) → destination`, the `AnalyserNode` reads its input
  (= the processor output), so the FFT runs over the mic samples just like the old live
  mic node (`micSource.connect(analyserNode)`) did. `gain(0)` keeps playback silent.

### Waveform visualization (unchanged)

The waveform keeps using a WebAudio `AnalyserNode` + FFT; only the PCM feeding it changes
(written from the Rust-sourced mic buffer instead of a `getUserMedia` node). Both
consumers are untouched:
- `useWaveform.start(analyser)` — its rAF loop calls `analyser.getByteFrequencyData()`
  and renders the recorder-window bars.
- `frameLevels` — computed from `analyserNode.getByteFrequencyData()` in the audio
  callback and broadcast to the library strip via `recorder://state`.

No changes to `useWaveform.ts` or `WaveformView.vue`; no FFT in Rust.

### 3. Frontend permission + wrappers

- `useRecordingPermissions.ts`: `ensureMicPermission()` calls the native command
  (`request_microphone_permission`) instead of opening a `getUserMedia` stream;
  `checkMicPermission()` added analogous to `checkSystemAudioPermission()`.
- `tauri.ts`: typed wrappers for the four new commands.

## Data flow

```
start_microphone_capture ─┐                         ┌─ mic-audio-data ─▶ micAudioBuffer ─┐
                          ├─ Rust Core Audio IO ────┤                                     ├─ ScriptProcessor
start_system_audio_capture┘                         └─ system-audio-data ▶ systemAudioBuffer┘     (clock, ~93ms)
                                                                                                    │
                                              interleave ch0=mic ch1=system → lamejs mp3 → chunks ──┘
                                                                                       (blob on stop)
```

## Error handling

- Mic capture start failure (no device / permission denied / unsupported format) surfaces
  the same way the current `getUserMedia` failure does: `startRecording` throws →
  `WaveformView.startRecording` runs `rollbackAndClose()`.
- Permission denied is handled by the existing recording-permission flow before capture.
- Ring-buffer underflow at the very start zero-fills the missing samples (bounded to the
  native start latency, ~tens of ms, and not ducked).

## Testing & verification

- **Spike first:** a minimal native default-input capture confirming, on the signed
  bundle, that system audio is full-level from frame 1 with the mic on (re-run the
  measurement: `sys` peaks ~thousands at ≤120 ms, no ~2 s ramp, no audible dip).
- `useRecorder.test.ts`: replace the `getUserMedia`/mic-source mocks with `mic-audio-data`
  event injection + a `start_microphone_capture` invoke mock; assert mic samples are
  drained, interleaved, and encoded, and that `frameLevels` still update.
- Rust: tests mirroring `audio_capture.rs` — input-format validation accept/reject cases
  and `Resampler` behavior at the 44.1 kHz target (including the no-ramp-from-silence
  prime).
- Cargo suite via the macOS workaround (`DYLD_LIBRARY_PATH=… cargo test … --test-threads=1`).

## Cleanup (part of this work)

- **Revert** the drift-compensation change in `audio_capture.rs`
  (`kAudioSubTapDriftCompensationKey` back to `true`) — it was refuted as a fix.
- **Remove all instrumentation**: the warm-up `eprintln`/file-write and its `started_at`
  /`warmup_logged` plumbing in `audio_capture.rs`, the temporary `debug_log` command (and
  its `main.rs` registration), and the frontend ramp-logging in `useRecorder.ts`.

## Out of scope (noted, not done now)

- Removing the WaveformView "keep the window visible so `getUserMedia` resolves" hacks
  (native capture has no such requirement) — a safe follow-up simplification.
- Handling default-input-device changes mid-recording.
- Any browser/non-Tauri microphone fallback.
