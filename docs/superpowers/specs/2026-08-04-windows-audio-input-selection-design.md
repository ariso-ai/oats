# Windows audio-input selection — design

**Issue:** #299
**Date:** 2026-08-04

## Current behavior

Windows microphone capture was originally owned by WebView2. That made explicit
Bluetooth selection unreliable because the browser's endpoint IDs and visibility can
change independently of Windows' native capture endpoints. The final implementation
uses WASAPI shared-mode capture and enumerates the same native endpoints it opens.

macOS uses the native HAL microphone path and is intentionally outside this change.

## UX choice

Add one Windows-only **Input device** select beneath the existing Microphone toggle in
Settings → Recording:

- `System default` is always the first option and preserves today's behavior.
- Available `audioinput` devices follow by label.
- Settings refreshes on mount, focus, and a lightweight native-device polling watcher
  while the Settings window is mounted.
- A specific selection persists as its opaque device ID plus its last visible label.
- If Windows rotates a saved ID after a reconnect, one unique exact-label match repairs
  the saved ID before capture.
- A legacy WebView2 `default`/`communications` preference may carry a synthetic label
  prefix. Removing only that known prefix permits a one-time unique-label migration to
  the real native endpoint; ambiguous matches remain unavailable.
- If the saved device cannot be identified unambiguously, the select keeps
  `<label> (unavailable)` visible and asks the user to reconnect it or choose another
  input before recording.

Teams, Zoom, Riverside, Audacity, and Windows Sound settings all converge on a named
microphone dropdown. Input testing/meters are useful but beyond the smallest interface
needed for #299.

An explicit choice never silently falls back to System default. Windows can route that
default to an unrelated webcam or laptop microphone, which would make the UI claim one
device while recording another. `System default` remains the durable, unconstrained
choice for users who want Windows to manage Bluetooth profile and endpoint switching.

## Implementation

`src-tauri/src/mic_capture.rs` owns active Windows capture-endpoint enumeration and a
shared-mode WASAPI capture worker. It emits the same base64 Int16 mono 44.1 kHz
`mic-audio-data` event as the macOS HAL backend, so the frontend mixer remains
platform-independent. `src/composables/useAudioInputDevices.ts` owns plugin-store
persistence, availability resolution, and periodic refresh subscription. The Settings
view activates it only when native platform capabilities report Windows.

At each Windows recording start, `useRecorder` resolves the saved preference against a
fresh native enumeration. An available explicit choice is passed as a bounded opaque
endpoint ID to `start_microphone_capture`; a uniquely matching label repairs a rotated
ID. A missing or ambiguous explicit choice fails with an actionable unavailable-device
error. System default passes no ID and WASAPI resolves the current default capture
endpoint at recording start. macOS continues using its existing HAL default-input path.

One read-only Tauri command lists active native microphone endpoints; the existing start
command gains an optional, length-bounded endpoint ID. No capability permission,
filesystem path, or network path is introduced. Device IDs remain local opaque
preferences and are never logged.

## Focused verification

- Unit-test native enumeration/default behavior, persistence, available and unavailable
  saved choices, and polling-subscription cleanup.
- Settings tests cover Windows-only rendering, selection persistence, and unavailable
  messaging.
- Recorder tests cover passing exact native endpoint IDs, explicit unavailable-device
  failure without fallback, and unchanged macOS native capture.
- On Windows, smoke-test with two inputs or disconnect/reconnect while Settings is open,
  then start a new recording and confirm the selected track/device.
