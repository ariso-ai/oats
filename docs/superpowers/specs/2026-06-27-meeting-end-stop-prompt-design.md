# Meeting-End Stop Prompt (back-to-back call split)

**Date:** 2026-06-27
**Branch:** `feat/issue-157`
**Issue:** #157
**Status:** Approved design, pending implementation plan

## Problem

When a user goes straight from one call into the next, oats keeps recording
across both meetings, merging two separate calls into one transcript / note set.

The only automatic stop signal today is the mic monitor: it snapshots the
triggering PIDs when it enters its `Recording` phase
(`mic_monitor.rs:77-79`) and emits `auto-record://stop` only once **all** of
those PIDs release the mic for 8 s (`Stopping → Idle`, `mic_monitor.rs:96-108`),
which `WaveformView` turns into `handleStop` (`WaveformView.vue:600`).

In a true back-to-back transition the *same* app (e.g. the Zoom PID) holds the
mic continuously from call A into call B. The trigger PID never releases, so:

1. the machine never leaves `Recording`, no stop fires, and call A's recording
   bleeds into call B; and
2. even if a stop *did* fire, the machine cannot **re-arm** for call B, because
   re-arming requires a PID release that never happens.

The silence prompt (10 min of no captured sound → prompt → auto-stop) does not
help here: call B has live audio, so there is no silence to detect.

The mic-drop signal therefore structurally cannot fire on a same-app
back-to-back transition. We need a **calendar-driven** signal.

## Goal

While a recording is attached to a calendar meeting, detect when that meeting's
**scheduled end has passed** and surface a **visible, consentful** prompt
offering *Stop* / *Keep recording*. On *Stop*, end the current recording **and
re-arm the mic monitor** so the next call produces a fresh, separately-attached
session.

The prompt is a non-destructive **offer**: scheduled `end_at` is a noisy signal
(real meetings routinely run over), so ignoring it keeps recording.

## Non-Goals

- No detection for **local** or **unattached** recordings — they have no
  calendar end. The existing silence / mic-drop backstops still apply to them.
- No auto-stop on ignore (unlike the silence prompt). Ignoring the prompt keeps
  recording.
- No new user-facing setting to configure thresholds (hard-coded constants for
  now, matching the silence prompt's approach).
- No change to pause semantics — a paused recording is never prompted.
- No VAD / content-based meeting-end detection — purely calendar `end_at`.

## Behavioral Summary (decided)

- **Trigger:** the attached meeting's scheduled `end_at` passes (plus a small
  grace). Fires regardless of whether a next meeting exists.
- **Scope:** Ariso backend **and** the recording is attached to a meeting
  (`effectiveMeetingId != null`) **and** that meeting has an `end_at`.
- **Ignore / timeout default:** keep recording (the card dismisses, recording
  continues).
- **Re-prompt:** show once at `end_at + GRACE`; if kept/ignored and still
  recording, show **once more** after `REPROMPT_INTERVAL`; then never again for
  this recording (max 2 prompts).
- **On Stop:** `handleStop()` then re-arm the mic monitor so call B flows
  through the normal start-prompt path and attaches to the now-current meeting.

## Architecture

Mirrors the silence-stop prompt (`2026-06-17-silence-stop-prompt-design.md`):
the **frontend owns the state machine and all timing** (it owns the recording
lifecycle — `handleStop`, pause state, `effectiveMeetingId` — and is
unit-testable via pure functions); **Rust only renders the borderless prompt
window and forwards the user's click**.

Rejected alternatives:

- **Backend-owned detection** (Rust polls `/meetings`, compares `end_at`,
  emits the prompt). Rejected: it would duplicate the recording-lifecycle state
  the frontend already owns (stop, pause, attachment, re-arm timing) and is
  harder to unit-test.
- **Reuse the silence-prompt window** for both. Rejected: conflates two
  independent triggers that could collide on a single shared window.

## Frontend

### `meetingEndWatch.ts` (new pure helpers)

Sibling of `silenceWatch.ts`.

```ts
export const MEETING_END_GRACE_MS = 2 * 60_000;     // 2 min past end → 1st prompt
export const MEETING_END_PROMPT_TIMEOUT_MS = 30_000; // card stays up 30 s, then keep
export const MEETING_END_REPROMPT_MS = 5 * 60_000;  // 5 min later → 2nd prompt
export const MEETING_END_MAX_PROMPTS = 2;

// Returns true when the meeting-end prompt should be shown on this tick.
// endAt is the attached meeting's scheduled end (epoch ms), or null when the
// recording is unattached / non-Ariso / has no end (then this always returns
// false). lastPromptAt is the time the previous prompt was shown (epoch ms), or
// null if none shown yet.
export function shouldPromptMeetingEnd(
  endAt: number | null,
  now: number,
  paused: boolean,
  promptsShown: number,
  lastPromptAt: number | null,
): boolean {
  if (paused || endAt === null) return false;
  if (promptsShown >= MEETING_END_MAX_PROMPTS) return false;
  if (promptsShown === 0) return now >= endAt + MEETING_END_GRACE_MS;
  // promptsShown === 1
  return lastPromptAt !== null && now >= lastPromptAt + MEETING_END_REPROMPT_MS;
}
```

### `WaveformView.vue` wiring

- **Resolve `end_at`.** After the recording attaches to a meeting (the existing
  `meetingId` query param, or `resolveAuto()` for auto recordings), fetch the
  meeting via `useMeetingApi().getMeeting(effectiveMeetingId)` — already used by
  `resolveSilenceSubtitle()` — and store `meetingEndAt` (epoch ms) and the title
  (subtitle). `meetingEndAt` stays `null` for local / unattached / Ariso
  meetings without an `end_at`, which disables the watch.

- **Dedicated 1 s interval** `meetingEndTimer`, separate from `silenceTimer`
  (this watch is **not** gated by the silence-detection setting). Module-level
  state: `meetingEndPromptShownAt: number | null` (null when idle),
  `meetingEndPromptsShown: number` (0…2), `meetingEndLastPromptAt: number | null`.

  Guard (all ticks): skip while `isUploading`, `uploadResult` set, or not
  recording (same guards as the silence loop).

  - **idle (`meetingEndPromptShownAt === null`)**: if
    `shouldPromptMeetingEnd(meetingEndAt, now, paused, meetingEndPromptsShown,
    meetingEndLastPromptAt)` →
    `invoke('show_meeting_end_prompt', subtitle ? { subtitle } : {})`;
    set `meetingEndPromptShownAt = meetingEndLastPromptAt = now`;
    `meetingEndPromptsShown++`.
  - **prompted**: on pause → `dismiss_meeting_end_prompt`, back to idle (keeps
    `promptsShown`); on timeout
    (`now - meetingEndPromptShownAt >= MEETING_END_PROMPT_TIMEOUT_MS`) →
    `dismiss_meeting_end_prompt`, back to idle. (Timeout = keep recording, the
    ignore default.)

- **Events** (registered in `onMounted`, torn down in `onUnmounted`, alongside
  the `tray://*` / `auto-record://stop` / `silence-prompt://*` listeners):
  - `meeting-end-prompt://keep` → set `meetingEndPromptShownAt = null` (back to
    idle); the existing re-prompt logic naturally fires the 2nd prompt after
    `REPROMPT_INTERVAL`.
  - `meeting-end-prompt://stop` → `await handleStop()` then
    `invoke('request_mic_monitor_rearm')`.

- **Cleanup:** `handleStop` / `discardRecording` / `onUnmounted` must clear
  `meetingEndTimer` and, if `meetingEndPromptShownAt !== null`, call
  `dismiss_meeting_end_prompt` — mirroring the silence prompt's teardown so no
  orphaned card lingers.

## Re-arm mechanism (the "fresh session for call B" half)

After a manual / programmatic stop the mic-monitor `Machine` is stuck in
`Recording` — that phase only exits on a trigger-PID release
(`mic_monitor.rs:86-95`), which never happens in a continuous-mic back-to-back
transition. So stopping call A alone would leave the monitor unable to start
call B.

Add a reset path:

- `MicMonitorManager` gains an `AtomicBool` `rearm` flag.
- New Tauri command `request_mic_monitor_rearm(app)` sets the flag.
- `run_loop` checks the flag at the top of each tick; when set, it does
  `machine = Machine::new()` and clears the flag (before computing `external` /
  `recording_active` for that tick).

With the recorder now stopped (`recording_active == false`) and the app still
holding the mic, the reset machine advances `Idle → Arming → Recording` over the
normal 3 s `START_DEBOUNCE_MS` and fires its **normal start prompt**. That path
already resolves the *current* meeting (now call B) for the title and, on
acceptance, opens a fresh recorder that `resolveAuto()` attaches to B — yielding
a separate, correctly-attached session.

For a **manual** attached recording the monitor was already `Idle`, so the reset
is a harmless no-op; the stop simply ends the recording.

## Rust — `meeting_notifications.rs`

Mirror the silence-prompt plumbing (`meeting_notifications.rs:992-1106`):

- **Window:** label `meeting-end-prompt`, route
  `/#/meeting-end-prompt?seconds=<PROMPT_TIMEOUT_secs>&subtitle=<title>`, built
  by a `meeting_end_prompt_url(seconds, subtitle)` helper that URL-encodes like
  `silence_prompt_url`. Same borderless top-right chrome (no decorations,
  transparent, always-on-top, never focused, `skip_taskbar`), reusing the
  shared `MEETING_PROMPT_W/H` / expanded-height constants and top-right docking.
- **Commands:**
  - `show_meeting_end_prompt(app, subtitle: Option<String>)` — open (or replace)
    the window on the main thread.
  - `dismiss_meeting_end_prompt(app)` — close the window if up.
  - `resolve_meeting_end_prompt(app, stop: bool)` — emit
    `meeting-end-prompt://stop` (stop) or `meeting-end-prompt://keep`
    (keep / dismiss), then close the window. (Mirrors `resolve_silence_prompt`.)
  - `resize_meeting_end_prompt(app, expanded: bool)` — grow/shrink for the
    "more options" menu, like `resize_silence_prompt`.
  - `request_mic_monitor_rearm(app)` — set the re-arm flag (see above); lives in
    `mic_monitor.rs` next to `sync_auto_record`.
- Register all new commands in the `invoke_handler` in `main.rs`.

## Frontend — `MeetingEndPromptView.vue`

Clone `SilencePromptView.vue` (it already renders the borderless card, countdown
bar, and "more options" / dismiss affordance, and reads `seconds` / `subtitle`
from the route via `meetingPromptParams.ts`). Changes:

- Copy: title *"Meeting ended"*; body *"This meeting is past its scheduled end —
  still recording. Keep recording or stop?"* (when a `subtitle`/title is present,
  show it as the meeting name line, as the silence prompt does).
- Primary action **Stop** → `invoke('resolve_meeting_end_prompt', { stop: true })`.
- Secondary action **Keep recording** → `invoke('resolve_meeting_end_prompt',
  { stop: false })`.
- Countdown bar is cosmetic (drives nothing destructive); on countdown end the
  card may close itself, but the frontend tick's `PROMPT_TIMEOUT` is the source
  of truth that returns the watch to idle.
- Register the route `/#/meeting-end-prompt` in `src/main.ts` next to
  `silence-prompt`.

## Events & Commands Summary

| Direction | Name | Payload | Purpose |
|---|---|---|---|
| FE → BE (invoke) | `show_meeting_end_prompt` | `{ subtitle? }` | Display the meeting-end card |
| FE → BE (invoke) | `dismiss_meeting_end_prompt` | none | Clear the card |
| FE → BE (invoke) | `resolve_meeting_end_prompt` | `{ stop }` | From the view: user's choice |
| FE → BE (invoke) | `resize_meeting_end_prompt` | `{ expanded }` | Grow/shrink for the dismiss menu |
| FE → BE (invoke) | `request_mic_monitor_rearm` | none | Reset the mic monitor to re-arm for the next call |
| BE → FE (emit) | `meeting-end-prompt://stop` | none | User chose *Stop* |
| BE → FE (emit) | `meeting-end-prompt://keep` | none | User chose *Keep recording* |

## Testing

- **Unit (Vitest)** — `meetingEndWatch` helpers:
  - `shouldPromptMeetingEnd` fires at/after `endAt + GRACE`; not before; never
    while paused; never when `endAt === null`.
  - 2nd prompt fires only once `lastPromptAt + REPROMPT_INTERVAL` has elapsed;
    never past `MAX_PROMPTS` (returns false at `promptsShown === 2`).
- **Unit (Rust)** — `meeting_end_prompt_url` carries `seconds` and URL-encodes
  the subtitle (omitting it when empty), mirroring the `silence_prompt_url`
  tests.
- **Manual** (signed bundle `ai.ariso.desktop`, per the macOS-permission-testing
  memory; run the Rust suite with the `DYLD_LIBRARY_PATH` + `--test-threads=1`
  workaround):
  - Attach an auto recording to a short calendar meeting; let `end_at + GRACE`
    pass → prompt appears top-right.
  - **Stop** → recording stops; with the mic still held, a fresh start prompt
    appears within ~3 s and (on accept) records a new session attached to the
    now-current meeting.
  - **Keep recording** / ignore → recording continues; a second prompt appears
    ~5 min later, then no more.
  - Pausing while prompted dismisses the card.

## Risks / Edge Cases

- **Meeting legitimately runs over.** Covered: ignore = keep recording; at most
  two prompts; no auto-stop.
- **No next meeting (single meeting ends).** The prompt still offers a stop; on
  stop the monitor re-arms but finds no live mic / no new meeting and stays idle.
- **`getMeeting` fails / meeting has no `end_at`.** `meetingEndAt` stays null →
  watch disabled; degrades to today's behavior.
- **Manual attached recording.** Watch still applies; `request_mic_monitor_rearm`
  is a harmless no-op (monitor was idle).
- **Stop races another stop** (manual stop / silence stop while prompted):
  `handleStop`'s `isStopping` guard already serializes; teardown dismisses the
  card.
- **Re-arm flag set but recorder still tearing down.** The flag is level-checked
  each tick; once `recording_active` flips false the reset machine re-arms on a
  subsequent tick, so ordering between stop completion and the flag is safe.
</content>
</invoke>
