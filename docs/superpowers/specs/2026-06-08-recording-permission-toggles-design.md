# Recording Permission Toggles — Design

**Date:** 2026-06-08
**Status:** Approved (pending spec review)
**Branch:** `settings-permission-toggles`

## Summary

Replace the single **Recording mode** dropdown in the Settings window with two
independent toggles — **Microphone** and **System Audio** — each of which
triggers the corresponding macOS permission prompt when switched on, and
deep-links to System Settings when the permission has already been decided
(denied / no prompt possible).

The two toggles are **fully independent**: any combination is allowed in
Settings (mic only, system only, both, or neither). Supporting system-audio-only
recording (mic off) requires a new code path in the recorder, which today always
captures the microphone.

## Current State

- **`src/views/SettingsView.vue`** — an "Audio" section with a single
  `<select v-model="recordingMode">` dropdown (`mic` / `mic_and_system`).
  Persisted via the Tauri store (`settings.json`, key `recordingMode`) in a
  `watch(recordingMode, …)`. Loaded back in `onMounted`.
- **`src/composables/useRecorder.ts`** — `startRecording(mode)`. **Always** opens
  the mic via `getUserMedia`. A `ScriptProcessorNode` on the mic source is the
  audio clock (`onaudioprocess`) that pulls PCM and (for `mic_and_system`) drains
  the system-audio ring buffer to a stereo MP3 encoder. An `AnalyserNode` on the
  mic source feeds the waveform via `getAnalyser()`.
- **`src/views/WaveformView.vue`** — `startRecording()` reads `recordingMode` from
  the store, maps it to `'mic' | 'mic_and_system'`, and calls
  `recorder.startRecording(mode)`.
- **`src-tauri/src/audio_capture.rs`** — `start_system_audio_capture` /
  `stop_system_audio_capture` Tauri commands using ScreenCaptureKit. Screen
  Recording permission is currently triggered *implicitly* by ScreenCaptureKit;
  there is no explicit request/check command.
- **Permissions today** — mic is prompted implicitly by `getUserMedia`; screen
  recording is prompted implicitly by ScreenCaptureKit at record time.
- **`src/composables/useMeetingNotifications.ts`** — the model pattern for a
  toggle that requests an OS permission and deep-links to System Settings on
  denial (`ensureNotificationPermission`, `openNotificationSettings`).
- **Platform plumbing already in place** — `Info.plist` has both
  `NSMicrophoneUsageDescription` and `NSScreenCaptureUsageDescription`;
  `tauri.conf.json` sets `macOSPrivateApi: true` (required for `getUserMedia` in
  the WKWebView); the opener plugin already deep-links to
  `x-apple.systempreferences:`. App-defined `#[tauri::command]`s need no extra
  capability ACL entry.

## Goals

1. Two independent toggles (Microphone, System Audio) in the Settings window.
2. Toggling on triggers the relevant macOS permission prompt.
3. When the permission is already decided so no prompt appears, open System
   Settings to the relevant Privacy pane.
4. Recorder supports all four combinations, including **system-audio-only**.

## Non-Goals

- No change to the transcription / upload pipeline.
- No new permission UI outside the Settings window.
- No Windows/Linux behavior change (deep-links are macOS-only, as with
  notifications).

## Design

### 1. Settings UI — `src/views/SettingsView.vue`

Rename the "Audio" section to **"Recording"**. Replace the single dropdown with
two toggle rows, reusing the `.toggle` / `.toggle-track` / `.toggle-thumb` switch
styles already defined in the file (used by the Notifications toggle):

```
Recording
┌────────────────────────────────────────┐
│ Microphone                      [ o——]  │
│   <status: granted / not granted>       │
│ System Audio                    [ o——]  │
│   <status: granted / not granted>       │
└────────────────────────────────────────┘
```

- Each toggle is bound to a ref (`micEnabled`, `systemAudioEnabled`) and an
  `@change` handler (`onToggleMic`, `onToggleSystemAudio`).
- Each row has a status line beneath it, reusing the existing
  `notif-status` / `notif-status--ok` / `notif-status--err` classes, driven by
  per-toggle status refs (`micStatus`, `systemAudioStatus`: `'' | 'granted' |
  'denied'`).
- Remove `recordingMode` ref, its `<select>`, and its `watch`.

### 2. Persistence

Source of truth becomes two booleans in the Tauri store (`settings.json`):

| Key                         | Type    | Default |
| --------------------------- | ------- | ------- |
| `recordMicEnabled`          | boolean | `true`  |
| `recordSystemAudioEnabled`  | boolean | `true`  |

Defaults preserve today's behavior (legacy default was `mic_and_system`).

**One-time migration** (in the load helper): if both new keys are absent but the
legacy `recordingMode` key exists, derive and persist:

- `recordingMode === 'mic'` → `{ mic: true, system: false }`
- `recordingMode === 'mic_and_system'` → `{ mic: true, system: true }`

The legacy `recordingMode` key is then no longer read by any consumer.

### 3. Permission flow — new `src/composables/useRecordingPermissions.ts`

Mirrors `useMeetingNotifications.ts`. Exposes:

- `ensureMicPermission(): Promise<boolean>` — call
  `navigator.mediaDevices.getUserMedia({ audio: true })`, immediately stop all
  tracks, return `true`; on throw (`NotAllowedError` etc.) return `false`.
- `ensureSystemAudioPermission(): Promise<boolean>` — `invoke`s the new Rust
  command `request_screen_capture_permission` (below) and returns its boolean.
- `openMicSettings(): Promise<void>` — macOS only; `openUrl(
  'x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone')`.
- `openSystemAudioSettings(): Promise<void>` — macOS only; `openUrl(
  'x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture')`.
- Store helpers: `isMicEnabled()`, `isSystemAudioEnabled()`,
  `setMicEnabled(bool)`, `setSystemAudioEnabled(bool)`, plus the migration read.

**Toggle handler shape** (identical structure for both toggles, following the
existing `onToggleMeetingNotifications`):

```
optimistically flip the ref
if turning ON:
  granted = await ensure<X>Permission()
  status = granted ? 'granted' : 'denied'
  if (!granted) await open<X>Settings()   // best-effort, must not throw out
else:
  status = ''
try: await set<X>Enabled(value)
catch: revert the ref                      // persist failed → undo optimistic flip
```

Permission/settings calls are wrapped so a rejection never aborts the handler
before persistence (same rationale as the notifications handler).

### 4. Rust — `src-tauri/src/audio_capture.rs` + `src-tauri/src/main.rs`

Add CoreGraphics FFI for the Screen Recording permission:

```rust
#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGRequestScreenCaptureAccess() -> bool;   // prompts the first time
    fn CGPreflightScreenCaptureAccess() -> bool;  // checks, never prompts
}

#[tauri::command]
pub fn request_screen_capture_permission() -> bool {
    #[cfg(target_os = "macos")]
    { unsafe { CGRequestScreenCaptureAccess() } }
    #[cfg(not(target_os = "macos"))]
    { true }
}

#[tauri::command]
pub fn check_screen_capture_permission() -> bool {
    #[cfg(target_os = "macos")]
    { unsafe { CGPreflightScreenCaptureAccess() } }
    #[cfg(not(target_os = "macos"))]
    { true }
}
```

`CGRequestScreenCaptureAccess()` surfaces the prompt only the first time; once
the user has decided (denied), it returns `false` without a prompt — which is
exactly when the composable opens System Settings. Register both commands in the
`generate_handler!` list in `main.rs`. `check_screen_capture_permission` is used
on Settings mount to populate the initial status line without prompting.

### 5. Recorder — `src/composables/useRecorder.ts`

Extend the mode type to `'mic' | 'system' | 'mic_and_system'`.

- `'mic'` — unchanged (mono, `getUserMedia` only).
- `'mic_and_system'` — unchanged (stereo: ch0 mic, ch1 system).
- `'system'` — **new path (Option A)**: do **not** call `getUserMedia`.
  - Create the `AudioContext` and a `ScriptProcessorNode(4096, 1, 1)`.
  - Start ScreenCaptureKit (`start_system_audio_capture`) and the
    `system-audio-data` listener exactly as the system branch does today,
    filling the existing `systemAudioBuffer` ring buffer.
  - Connect the graph as `processor → analyser → gain(0) → destination`. A
    `ScriptProcessorNode` connected to `destination` fires `onaudioprocess`
    even with no input; `gain(0)` keeps it silent (no echo) while keeping the
    graph pulling.
  - In `onaudioprocess`: `drainSystemAudio(frameSize)` → encode **mono** to the
    MP3 encoder, **and** write those same samples into
    `e.outputBuffer.getChannelData(0)` so the existing `AnalyserNode` (now tapped
    off the processor output) visualizes the system audio.
  - Use a mono encoder (`new lamejs.Mp3Encoder(1, 44100, 128)`).
  - `getAnalyser()` returns this analyser — **`WaveformView.vue` needs no
    change** to its analyser handling.
  - `cleanup()` already tears down the system-audio capture, listener, ring
    buffer, processor, analyser, and context; extend it to also disconnect the
    new gain node.

### 6. Record-time "neither" case — `src/views/WaveformView.vue`

`startRecording()` reads the two booleans and derives the mode:

| mic   | system | mode               |
| ----- | ------ | ------------------ |
| true  | false  | `'mic'`            |
| false | true   | `'system'`         |
| true  | true   | `'mic_and_system'` |
| false | false  | *abort* (see below)|

If both are off, do **not** record silence. Abort gracefully using the existing
failure path already present in `startRecording` (reset the tray to idle and
close the recording window), so the user is never left with an empty recording.

## Data Flow

```
Settings toggle ON
  → ensure<X>Permission()  → OS prompt (first time) → granted/denied
      └ denied → open<X>Settings() (deep-link to Privacy pane)
  → set<X>Enabled(true) persisted to settings.json
        │
        ▼
Tray "record"
  → WaveformView.startRecording() reads recordMicEnabled / recordSystemAudioEnabled
  → derives mode → recorder.startRecording(mode)
        ├ mic            : getUserMedia → mono encode
        ├ system         : ScreenCaptureKit → mono encode (Option A graph)
        ├ mic_and_system : getUserMedia + ScreenCaptureKit → stereo encode
        └ neither        : abort (tray idle, close window)
```

## Error Handling

- **Permission request throws/rejects** — treated as "denied"; the handler logs
  a warning, sets status to `'denied'`, attempts the Settings deep-link, and
  still persists the toggle state. A rejection never leaves the optimistic flip
  stranded.
- **Persist failure** — the optimistic ref flip is reverted.
- **`getUserMedia` / ScreenCaptureKit fails at record time** — existing
  `useRecorder` cleanup runs and `WaveformView` rolls the tray back to idle and
  closes the window (unchanged behavior).
- **Non-macOS** — permission helpers resolve `true` / are no-ops; deep-links are
  skipped (guarded by the existing `navigator.userAgent.includes('Mac')` check).

## Testing

Follow the existing `src/views/SettingsView.download.test.ts` pattern (Vitest,
mocked Tauri APIs):

- **Migration / mode-derivation** (pure functions, easiest to isolate):
  - legacy `mic` → `{mic:true, system:false}`; `mic_and_system` →
    `{mic:true, system:true}`; absent → defaults `{true, true}`.
  - boolean pair → recorder mode table, including `neither → abort`.
- **Toggle handlers** (mock the permission composable):
  - turn ON + granted → status `granted`, persisted `true`.
  - turn ON + denied → status `denied`, `open<X>Settings` called, persisted
    `true`.
  - persist throws → ref reverted.
  - turn OFF → status cleared, persisted `false`.
- Rust FFI is not unit-tested (thin CoreGraphics wrapper); verified manually.

## Open Questions

None outstanding. (System-only capture confirmed as **Option A**; both-off in
Settings is allowed but aborts at record time.)
