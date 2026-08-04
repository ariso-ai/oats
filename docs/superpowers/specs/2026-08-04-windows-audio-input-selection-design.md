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
- If that saved ID is missing, the select keeps `<label> (unavailable)` visible and an
  inline message explains that new recordings will temporarily use System default.
  Reconnecting the same device restores it automatically; choosing another option
  replaces the saved preference.

Teams, Zoom, Riverside, Audacity, and Windows Sound settings all converge on a named
microphone dropdown. Input testing/meters are useful but beyond the smallest interface
needed for #299.

The temporary fallback keeps the recorder usable from tray/auto-record launchers, which
have no pre-recording error surface. It does not silently discard the saved preference,
so Settings still communicates the problem and reconnection restores user intent.

## Implementation

`src/composables/useAudioInputDevices.ts` owns enumeration normalization, plugin-store
persistence, availability resolution, and `devicechange` subscription. The Settings
view activates it only when native platform capabilities report Windows.

At each Windows recording start, `useRecorder` resolves the saved preference against a
fresh enumeration. An available explicit choice adds
`deviceId: { exact: savedId }` to the existing audio constraints. System default or a
missing saved input omits `deviceId`. macOS never calls this resolver.

No new Tauri command, capability, native permission, or network path is introduced.
Device IDs are treated as opaque preferences and are neither logged nor passed across
the invoke boundary.

## Focused verification

- Unit-test filtering/default behavior, persistence, available and unavailable saved
  choices, and device-change subscription cleanup.
- Settings tests cover Windows-only rendering, selection persistence, and unavailable
  messaging.
- Recorder tests cover exact selected-device constraints, default/unavailable fallback,
  and unchanged macOS native capture.
- On Windows, smoke-test with two inputs or disconnect/reconnect while Settings is open,
  then start a new recording and confirm the selected track/device.
