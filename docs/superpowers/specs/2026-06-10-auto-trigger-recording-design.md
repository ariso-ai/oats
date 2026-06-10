# Auto-Trigger Recording — Design

**Date:** 2026-06-10
**Branch:** `feat/auto-trigger-recording`
**Status:** Approved (pending spec review)

## Summary

Automatically start a meeting recording when another application (Zoom,
FaceTime, Google Meet, etc.) begins using the microphone, and automatically end
it when that application releases the microphone — independent of any calendar
entry. When a current calendar meeting can be matched, the recording is attached
to it; otherwise the user is asked to confirm the capture.

The recording itself reuses the existing recorder pipeline (`useRecorder` +
`RecorderPanel.vue` in the `waveform` window) and finalize/upload flow
unchanged. The new work is a native microphone-usage monitor and the
orchestration that connects it to that pipeline.

## Goals

- Start recording within a few seconds of a meeting app taking the mic.
- Stop recording shortly after the meeting app releases the mic.
- Attach to a "happening now" calendar meeting when one exists (Ariso backend).
- Never silently capture ad-hoc mic usage without a calendar match — ask first.
- Never interfere with a manual recording already in progress.
- Be a user-controllable setting (default ON), and degrade gracefully on
  hardware/OS that can't support reliable detection.

## Non-Goals

- Windows/Linux support (the app is macOS-only).
- Detecting *which* meeting app is in use, or app-specific behavior.
- Recording purely local mic blips (Siri, voice memos, "test your mic").
- Changing the recording/encoding/upload pipeline.

## Key Decisions

| Decision | Choice |
|---|---|
| Detection mechanism | Per-process CoreAudio (`kAudioProcessPropertyIsRunningInput`), macOS 14.4+ |
| Self-recording loop | PID-snapshot at trigger; stop only when the *triggering* PIDs release |
| Opt-in | Settings toggle, **default ON** |
| Calendar association | Best-effort match to a "current" meeting; else confirm prompt |
| Confirm-prompt timeout (~60 s, no response) | **Discard & stop** |
| Local backend (no calendar) | Same confirm prompt as an unmatched Ariso recording |
| UI surface | Reuse the existing recorder pill + tray menu; fully controllable |
| OS gating | Monitor active on macOS 14.4+ only; app minimum stays macOS 13 |

## Architecture

### Component overview

```
┌──────────────────────── Rust process (never suspended) ────────────────────┐
│                                                                             │
│  mic_monitor.rs                          commands.rs / tray.rs              │
│  ┌─────────────────────────┐             ┌──────────────────────────────┐  │
│  │ CoreAudio poll loop      │  open win   │ open_waveform_window(auto=1) │  │
│  │  - enumerate proc objs   ├────────────▶│ RecordingState (shared)      │  │
│  │  - debounce on/off       │  emit stop  │ set_tray_recording           │  │
│  │  - PID snapshot          │◀────────────┤                              │  │
│  └─────────────────────────┘             └──────────────────────────────┘  │
│            │ emits: auto-record://stop                                       │
└────────────┼────────────────────────────────────────────────────────────────┘
             ▼
   waveform webview  ── RecorderPanel.vue (auto=1)
       - start recording immediately
       - match calendar meeting (Ariso) OR show confirm overlay
       - on auto-record://stop → finalize + upload (existing flow)
```

### New / changed files

- **`src-tauri/src/mic_monitor.rs`** (new) — CoreAudio FFI + monitor task +
  state machine + the `RecordingState` shared flag and start/stop commands.
- **`src-tauri/src/main.rs`** — register module, manage `RecordingState`, spawn
  the monitor on startup (gated on setting + OS version), register new commands.
- **`src-tauri/src/commands.rs`** — `open_waveform_window` gains an `auto` flag
  that adds `&auto=1` to the URL; `RecordingState` is set/cleared alongside the
  tray menu transitions.
- **`src/views/RecorderPanel.vue`** — read `auto=1`; start immediately; run the
  match-or-confirm flow; render the confirm overlay; listen for
  `auto-record://stop`.
- **`src/composables/useAutoTrigger.ts`** (new) — pure-ish orchestration the
  panel calls: resolve association (match vs prompt), drive the confirm overlay
  state machine, and the timeout. Unit-tested in isolation.
- **`src/views/SettingsView.vue`** + **`src/composables/useRecordingPermissions.ts`**
  — new "Auto-record meetings" toggle persisted to `settings.json`
  (`autoRecordEnabled`), and a sync call so toggling it starts/stops the native
  monitor without a restart.

## Native detection (`mic_monitor.rs`)

### CoreAudio FFI (macOS 14.4+)

Read-only enumeration — no audio tap is created, so no extra TCC capture
permission should be required (to be verified during implementation):

- `kAudioHardwarePropertyProcessObjectList` → `[AudioObjectID]` of process
  objects.
- Per object: `kAudioProcessPropertyPID` (pid) and
  `kAudioProcessPropertyIsRunningInput` (bool).
- "External input set" = `{ pid : IsRunningInput == true } \ { our_pid }` where
  `our_pid = std::process::id()`.

Implemented as raw `extern "C"` against the CoreAudio framework, matching the
existing raw-FFI style in `audio_capture.rs` (`CGRequestScreenCaptureAccess`).
Availability is detected at runtime; if the symbols/property are unavailable or
the OS is < 14.4, the monitor reports unsupported and never runs.

### State machine

States: `Idle` → `Arming` → `Recording` → `Stopping` → `Idle`.

- **Idle:** poll every ~1 s. When the external input set becomes non-empty,
  capture `armed_at` and move to `Arming`.
- **Arming:** require the external set to stay non-empty for `START_DEBOUNCE`
  (**3 s**). If it empties first, return to `Idle` (it was a blip). On success:
  snapshot `trigger_pids` = current external set, consult `RecordingState`.
  - If a recording is already active (manual or auto) **or** the setting is
    disabled → return to `Idle` (do nothing).
  - Else → open the recorder window with `auto=1`, mark `RecordingState`
    auto-active, move to `Recording`.
- **Recording:** the meeting is "alive" while *any* PID in `trigger_pids` still
  has input running. New PIDs that appear (our own `getUserMedia` capture, a
  second app) are ignored for the stop decision. When none of `trigger_pids`
  has input running, capture `released_at`, move to `Stopping`.
- **Stopping:** require all `trigger_pids` to stay released for `STOP_DEBOUNCE`
  (**8 s**). If any re-acquires input, return to `Recording`. On success → emit
  `auto-record://stop`, move to `Idle`. `RecordingState` is cleared by the
  frontend when finalize completes (or by a window-closed fallback).

### Why the PID snapshot

Our own recorder opens the mic via `getUserMedia`, which would keep the input
device "in use." By tracking only the *triggering* PIDs for the stop decision,
our own capture (whatever PID WebKit attributes it to) never keeps the meeting
artificially "alive." The `RecordingState` guard separately prevents a manual
recording from self-triggering, regardless of PID attribution.

## Orchestration & shared state

- **`RecordingState`** (`Mutex<Recording>` in native, `app.manage`d):
  `{ active: bool, source: Manual | Auto }`. Set when a recorder window opens
  (manual via tray, or auto), cleared when finalize completes. The monitor reads
  it to suppress triggers while any recording runs.
- Manual recordings already flow through `open_waveform_window`; that helper
  sets `RecordingState{active, Manual}` so the monitor stays quiet.
- A new command `auto_recording_finished` (called by the panel after finalize,
  success or failure) clears the flag. A window-`Destroyed` handler clears it as
  a fallback so a crashed/closed window can't wedge the monitor off.

## Match-or-prompt flow (`useAutoTrigger.ts` + `RecorderPanel.vue`)

On mount with `auto=1`:

1. **Start recording immediately** via the existing path (respecting the user's
   mic/system source toggles) so the meeting's opening is never lost.
2. **Resolve association** in parallel:
   - **Ariso:** `listScheduledMeetings(now-2h … now+2h)` → `pickDefaultMeeting`
     (which itself applies the `start-5min … start+60min` "current" window).
     - `kind === 'current'` → set `meetingId`, header shows `Recording — <title>`.
       No prompt.
     - otherwise → enter **confirm** state.
   - **Local:** always **confirm** state (no calendar).
3. **Confirm overlay** (shown over the pill while recording continues):
   *"Recording started — keep it?"* with **Keep** / **Discard**.
   - **Keep** → dismiss overlay, continue as a normal recording.
   - **Discard** → stop recorder, drop audio, close window, mark
     `auto_recording_finished`.
   - **Timeout (~60 s, no response)** → same as **Discard**.
4. **Stop:** `auto-record://stop` (from native) ends the recording and runs the
   existing finalize/upload. A manual Stop (tray or pill) also works at any time.

Auto-recordings shorter than **MIN_DURATION (15 s)** at stop time are discarded
rather than uploaded (guards against late mic-on/quick-off races).

## Settings

- New toggle **"Auto-record meetings"** in `SettingsView.vue`, persisted to
  `settings.json` key `autoRecordEnabled` (default **true** on first read).
- Toggling calls a `sync_auto_record` command that starts/stops the native
  monitor immediately (mirrors `sync_meeting_notifications`).
- On macOS < 14.4 or when the CoreAudio symbols are unavailable, the toggle is
  rendered disabled with a "Requires macOS 14.4+" caption and the monitor never
  starts.
- The feature reuses the existing mic + screen-recording permissions already
  gated by the recording-source toggles; no new OS permission UI is introduced
  unless implementation reveals the process-list read needs one (see Risks).

## Edge cases

- **Mute mid-meeting:** Zoom/Meet keep the input stream open while muted, so the
  triggering PID stays "running input" → recording continues. Correct.
- **Manual recording active:** monitor suppressed via `RecordingState`.
- **Back-to-back meetings:** after `Stopping`→`Idle`, a fresh mic-on re-arms
  normally and produces a separate recording.
- **App started mid-meeting:** monitor begins in `Idle`; an already-running mic
  is detected on the first poll and arms normally.
- **Sign-out / Ariso match fails:** match step errors are swallowed; the flow
  falls back to the confirm prompt (treated as "no match").
- **Multiple input apps at once:** `trigger_pids` holds all of them; the meeting
  is alive until all release.
- **Window/finalize crash:** `Destroyed` handler clears `RecordingState` so the
  monitor recovers.

## Testing

- **Pure unit (Vitest):**
  - `useAutoTrigger`: association resolution (current/none), confirm state
    machine (keep/discard/timeout via injected clock), min-duration discard.
  - Reuse `pickDefaultMeeting` tests for the match boundary.
- **Rust unit:** the state-machine transitions modeled as a pure function over
  `(state, external_pid_set, recording_active, now)` so debounce/snapshot logic
  is testable without CoreAudio. FFI enumeration is a thin, separately-kept
  boundary.
- **Manual verification (ariso-desktop MCP):** start a FaceTime/Zoom call →
  confirm recorder opens and arms after ~3 s; end the call → confirm it stops
  after ~8 s and finalizes; verify manual recording suppresses auto-trigger;
  verify the confirm overlay keep/discard/timeout paths; verify the toggle
  starts/stops the monitor live.

## Risks & verification items

1. **PID attribution of our own `getUserMedia` capture** — the snapshot approach
   is designed to be robust to this, but confirm during implementation that our
   capture does not retroactively land in `trigger_pids`.
2. **TCC for reading the process list / `IsRunningInput`** — verify no audio
   capture authorization is required for a read-only enumeration; if it is,
   surface it through the existing permission UX.
3. **CoreAudio symbol availability across 14.4–15.x** — confirm the property
   constants and behave consistently; keep the runtime-availability guard.
4. **Poll interval vs. responsiveness/CPU** — 1 s poll is the starting point;
   switch to a CoreAudio property listener if a poll proves too laggy or costly.
```
