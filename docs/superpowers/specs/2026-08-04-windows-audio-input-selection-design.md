# Windows audio-input selection — design

**Issue:** #299
**Date:** 2026-08-04

## Current behavior

Windows microphone capture is owned by the WebView. Every new recording calls
`navigator.mediaDevices.getUserMedia()` without a `deviceId`, so WebView2 asks Windows
for the current default input. Oats does not enumerate inputs, expose the chosen input,
persist a preference, or observe `devicechange`. A device connected after launch can
only affect Oats indirectly if Windows makes it the default before the next recording.
An active recording remains bound to the stream it opened.

macOS uses the native HAL microphone path and is intentionally outside this change.

## UX choice

Add one Windows-only **Input device** select beneath the existing Microphone toggle in
Settings → Recording:

- `System default` is always the first option and preserves today's behavior.
- Available `audioinput` devices follow by label.
- Settings refreshes on mount, focus, and the browser `devicechange` event.
- A specific selection persists as its opaque device ID plus its last visible label.
- If WebView2 rotates a saved ID after a reconnect, one unique exact-label match repairs
  the saved ID before capture.
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

`src/composables/useAudioInputDevices.ts` owns enumeration normalization, plugin-store
persistence, availability resolution, and `devicechange` subscription. The Settings
view activates it only when native platform capabilities report Windows.

At each Windows recording start, `useRecorder` resolves the saved preference against a
fresh enumeration. An available explicit choice adds
`deviceId: { exact: savedId }` to the existing audio constraints. A uniquely matching
label repairs a rotated ID; a missing or ambiguous explicit choice fails with an
actionable unavailable-device error. Only System default omits `deviceId`. macOS never
calls this resolver.

No new Tauri command, capability, native permission, or network path is introduced.
Device IDs are treated as opaque preferences and are neither logged nor passed across
the invoke boundary.

## Focused verification

- Unit-test filtering/default behavior, persistence, available and unavailable saved
  choices, and device-change subscription cleanup.
- Settings tests cover Windows-only rendering, selection persistence, and unavailable
  messaging.
- Recorder tests cover exact selected-device constraints, explicit unavailable-device
  failure without fallback, and unchanged macOS native capture.
- On Windows, smoke-test with two inputs or disconnect/reconnect while Settings is open,
  then start a new recording and confirm the selected track/device.
