# Meeting-switch prompt (replaces the meeting-end prompt)

**Date:** 2026-07-21
**Status:** Approved

## Problem

Today, when a recording attached to a calendar meeting runs past its scheduled
end — or the next calendar meeting starts — oats shows a **meeting-end prompt**
("Meeting ended" / "Next meeting started", Stop / Keep recording). Stopping is a
dead end: the user must then notice the next call and start a fresh recording
(via the mic monitor's re-arm + a second prompt).

Desired behavior:

1. **No meeting-end prompt at all.** A recording with no following meeting runs
   until the user stops it (the silence prompt still covers forgotten
   recordings).
2. Only **"meeting started"-style prompts** exist. When a recording is ongoing
   and the **next calendar meeting starts**, show a "Meeting started" card;
   accepting it stops the current recording (finalize + upload as normal) and
   **immediately** starts recording the new meeting, auto-attached — no picker.

## Decisions (confirmed with user)

- **Trigger:** calendar only — reuse the existing next-meeting-start detection
  (`findNextMeetingStart` over the scheduled-meetings list). No mic-based
  trigger. Ariso backend + attached meeting only, as today.
- **On accept:** auto-attach the new recording to the meeting that just
  started. No meeting picker.
- **Settings:** delete the `meetingEndReminderEnabled` setting and its
  Settings toggle entirely. The switch prompt is not gated by a setting.

## Design

### What is removed

- **Scheduled-end trigger and all its machinery:** `MEETING_END_GRACE_MS`,
  `MEETING_END_REPROMPT_MS`, `MEETING_END_MAX_PROMPTS`,
  `shouldPromptMeetingEnd`'s end-time branch, `findMeetingEndAt`.
- **Settings:** `useMeetingEndReminder.ts`, the "Meeting stop reminder" toggle
  in `SettingsView.vue`, the `meetingEndReminderEnabled` store key.
- **The "Stop / Keep" card semantics.** The window/view is repurposed (below),
  not kept alongside.
- **Frontend stop path:** `handleMeetingEndStop`/`handleMeetingEndKeep` and the
  `request_mic_monitor_rearm` invoke in `WaveformView.vue`. The Rust
  `request_mic_monitor_rearm` command and the mic-monitor `rearm` flag lose
  their only production caller and are deleted too (the switch flow keeps
  recording, so no re-arm is needed for back-to-back calls).

### What is kept unchanged

- The mic-driven **meeting-started prompt** (`MeetingPromptView.vue`, oneshot
  `resolve_meeting_prompt` flow) for "call started while not recording."
- The **silence prompt**, untouched.

### The switch prompt

Reuses the WaveformView-driven prompt-window pattern the meeting-end prompt
already uses (recorder window invokes a Rust command to open a borderless card;
card resolves via events back to the recorder window). The meeting-end files are
**renamed/rewritten** into switch-prompt equivalents:

| Old | New |
|---|---|
| `MeetingEndPromptView.vue` | `MeetingSwitchPromptView.vue` |
| `/meeting-end-prompt` route | `/meeting-switch-prompt` |
| `parseMeetingEndPromptParams` | `parseMeetingSwitchPromptParams` |
| `meetingEndWatch.ts` | `meetingSwitchWatch.ts` |
| Rust `show/dismiss/resolve/resize_meeting_end_prompt` | `show/dismiss/resolve/resize_meeting_switch_prompt` |
| window label `meeting-end-prompt` | `meeting-switch-prompt` |
| events `meeting-end-prompt://stop\|keep` | `meeting-switch-prompt://switch\|keep` |

**Card copy:** title **"Meeting started"**, subtitle = the next meeting's title
(hidden when absent), primary split button **"Take notes"** (= switch), chevron
menu reveals **"Keep recording"**; corner ✕ = keep. 30-second cosmetic
countdown, same visual as today. Timeout or dismiss = keep recording; the card
does **not** re-prompt for the same next meeting.

### Detection (in `WaveformView.vue` + `meetingSwitchWatch.ts`)

- `resolveMeetingEnd()` becomes `resolveNextMeeting()`: Ariso + attached
  meeting only; fetches the scheduled-meetings window (−2h…+24h) and computes
  the **next meeting** = earliest meeting whose `start_at` is strictly after
  the attached meeting's `start_at` (parallel meetings sharing the start are
  skipped — existing semantics). `findNextMeetingStart` is extended to also
  return the next meeting's `id` (needed for attachment) alongside `startAt`
  and `title`.
- A 1s timer (replacing `meetingEndTimer`) fires the prompt when
  `now >= nextStartAt`, the recorder is **not paused**, and the prompt hasn't
  already been shown **for that next-meeting id** (a `Set`/last-id guard —
  once per candidate, so after a dismissed meeting B ends and C becomes next,
  C can still prompt).
- No grace period: the next meeting's start **is** the transition point.
- Paused recordings never prompt; the pending state is simply re-evaluated
  each tick, so resuming mid-meeting-B still prompts (id not yet consumed).

### Accept → switch (in-window, no window teardown)

The waveform window is a singleton and `handleStop` ends with the window
closing after upload — awaiting it would violate "immediately." Instead the
switch happens **inside the existing recorder window** (the same-window restart
that `resumeFailed()` already proves out):

On `meeting-switch-prompt://switch`:

1. Capture the old segment locally: `waveform.stop()`, read
   `recorder.startedAt`/`durationSeconds`, `blob = await recorder.stopRecording()`.
2. **Background-finalize the old segment.** Call
   `backend.finalizeRecording(blob, meta)` fire-and-forget with meta built from
   the **old** `effectiveMeetingId`. Deliberately does *not* go through
   `stoppedBlob`/`stoppedMeta`/`runFinalize` — those refs are the retry path
   and would concatenate the old meeting's audio into the next stop. On
   failure: log; the audio buffer is already persisted on disk before upload,
   so the Library's **Pending uploads** retry covers recovery (same guarantee
   as a timed-out finalize today).
   - Empty-blob / short-auto guard: if the blob is empty, or the recording is
     auto and shorter than `MIN_AUTO_DURATION_S`, drop the segment instead of
     uploading a stub — mirrors `handleStop`/`discardRecording`. (No on-disk
     cleanup is needed: the pending buffer is only written inside
     `finalizeRecording`, which we skip.)
3. Re-point the session: `effectiveMeetingId.value = nextMeetingId`, clear
   `localAppendId`-related meta influence (switch is Ariso-only, so local
   append/forceNew doesn't apply), reset the silence clock and switch-watch
   state, then `await recorder.startRecording()` and restart `waveform`.
   Elapsed time restarts from 0. The switched-to recording counts as **manual**
   (the user explicitly accepted), so the `MIN_AUTO_DURATION_S` stub gate no
   longer applies to its eventual stop even if the original recording was auto.
4. Sync the backend + other windows: invoke a new lightweight Rust command
   `update_recording_meeting(meeting_id)` that updates
   `RecordingState.meeting_id` (source unchanged) and re-emits
   `recording://started { meetingId }` — keeps
   `get_active_recording_meeting_id` and the Library's pinned meeting
   truthful. The existing `broadcastState()` heartbeat already carries
   `effectiveMeetingId` to the library strip.
5. The `effectiveMeetingId` watcher re-runs `resolveNextMeeting()`, so after
   switching to B the watch arms for C — back-to-back-to-back chains work.

### Keep / timeout

`meeting-switch-prompt://keep` (or 30s Rust-side timeout closing the window):
mark that next-meeting id as consumed; keep recording. Nothing else changes.

### Edge cases

- **No next meeting / non-Ariso / unattached:** watch is off; recording runs
  until manual stop (silence prompt still applies).
- **Paused:** never prompts while paused.
- **Recording B started manually mid-A-recording?** Not possible — the
  waveform window is a singleton; the switch prompt is the only in-recording
  transition.
- **Next meeting fetched before it exists:** `resolveNextMeeting` re-runs on
  the existing cadence (mount + meeting-id change); a meeting added to the
  calendar mid-recording is picked up the same way the end-watch data was —
  best-effort, acceptable.
- **Auto-join-scheduled next meeting:** the Ariso notetaker bot may already
  cover it. The switch prompt still shows (the user is *in* the room
  recording); parity with today's "Next meeting started" behavior. Out of
  scope to suppress.

### Capabilities / registration

- `src-tauri/capabilities/default.json`: replace `meeting-end-prompt` with
  `meeting-switch-prompt` in the windows list.
- `src-tauri/src/main.rs`: swap the four command registrations; register
  `update_recording_meeting`; drop `request_mic_monitor_rearm`.
- `src/main.ts`: swap the route.

## Testing

- **`meetingSwitchWatch.test.ts`** (rewrite of `meetingEndWatch.test.ts`):
  `findNextMeeting` returns `{startAt, id, title}`; skips parallel starts;
  nulls on missing/unparseable. `shouldPromptMeetingSwitch(nextStartAt, now,
  paused, alreadyPromptedForId)` — fires at start, frozen while paused, once
  per id.
- **`MeetingSwitchPromptView.test.ts`** (rewrite): renders title/subtitle/
  countdown; Take notes → `resolve_meeting_switch_prompt {switch: true}`;
  ✕/menu → `{switch: false}`.
- **`meetingPromptParams.test.ts`:** params parser renamed; defaults
  (`Meeting started` title, 30s).
- **WaveformView tests:** switch event → old segment finalized with old
  meetingId (not via retry refs), recorder restarted, `effectiveMeetingId`
  updated, `update_recording_meeting` invoked; keep event → no restart;
  short-auto old segment discarded.
- **Rust tests:** switch-prompt URL builder (mirrors existing meeting-end URL
  tests); `update_recording_meeting` state update; removal of rearm tests.
- Frontend suite via `npm test`; Rust via the documented
  `DYLD_LIBRARY_PATH=… cargo test … -- --test-threads=1`.

## Out of scope

- Any change to the mic-driven meeting-started prompt or silence prompt.
- Suppressing the switch prompt for auto-join-scheduled next meetings.
- A Settings toggle for the switch prompt.
