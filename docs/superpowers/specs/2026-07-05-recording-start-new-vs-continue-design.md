# Start recording: New vs. Continue the open meeting

**Date:** 2026-07-05
**Status:** Design approved, pending spec review
**Issue:** #174

## Problem

When a meeting is rendered in the detail pane and the user clicks **Start
recording**, the app immediately starts a recording with no way to say "this is a
continuation of the meeting I'm looking at." The user wants an explicit choice at
that moment: **start a new recording** or **continue recording the same meeting**.

The backends differ in how "continue" works, so the UX differs per backend:

- **Ariso (cloud):** attaching a new session to an existing meeting id already
  works (`start_recording_window({ meetingId })`); each session becomes another
  `audioClip` on the meeting. The gap is a UI to deliberately pick the open meeting.
- **Local (offline):** the backend only *auto*-appends a new recording to the
  previous one when it starts within a 5-minute wall-clock window
  (`most_recent_appendable`, `APPEND_WINDOW_SECONDS = 5*60`). There is no way for
  the user to deliberately continue an older local recording. This needs new
  backend work.

## Behavior

The trigger is: **a meeting is open in the detail pane AND the user clicks Start
recording.** When no meeting is open, behavior is unchanged from today.

### Local backend
Show a two-button dialog:

- **Continue "‹meeting title›"** (default) — the new recording session appends to
  that existing local recording, regardless of how much time has passed.
- **New recording** — a fresh ad-hoc recording (identical to today's behavior).
- Dismiss / cancel — do nothing.

### Ariso backend
Open the existing meeting picker (`/#/meeting-picker`), but with the open meeting
featured as the pre-selected default — **even if it is a past meeting not in
today's scheduled list** (decision confirmed during brainstorming). The picker
still lists other today meetings ("View all") and offers "Record a new meeting".
Selecting the featured meeting calls the existing
`start_recording_window({ meetingId })`, attaching a new session (audio clip) to
that meeting.

## Architecture

### Frontend

**New: `src/views/RecordingStartChoiceDialog.vue`**
Modeled on `AriJoinConfirmDialog.vue`. Props `{ open: boolean; meetingTitle: string }`,
emits `continue` / `new` / `cancel`. Same overlay/card/`role="dialog"` conventions
as the existing confirm dialogs. Default/focused button is **Continue**.

**New: `src/composables/useRecordingStartChoice.ts`**
Promise driver mirroring `useAriJoinConfirm.ts`, but three-way instead of boolean:

```ts
open: Ref<boolean>
meetingTitle: Ref<string>
requestChoice(title: string): Promise<'continue' | 'new' | null>  // null = cancelled
choose(kind: 'continue' | 'new'): void
cancel(): void
```

**New: `src/composables/decideStartRecording.ts` (pure helper)**
Keep the branch decision out of the giant `LibraryView.vue` (its Vitest coverage is
brittle). A pure function that, given `{ backendUsesPicker: boolean; openMeeting: {...} | null }`,
returns one of:

- `{ kind: 'default' }` — no meeting open: fall through to today's
  `decideRecordingAction` path unchanged.
- `{ kind: 'ariso-picker'; defaultMeetingId; defaultMeetingTitle }`
- `{ kind: 'local-choice'; meetingTitle; localRecordingId }`

**`src/views/LibraryView.vue` — `startRecording()`**
Wire the helper:

- `default` → existing behavior (`decideRecordingAction` → picker / attach / ad-hoc).
- `ariso-picker` → `invoke('open_meeting_picker', { defaultMeetingId, defaultMeetingTitle })`.
- `local-choice` → `await recordingStartChoice.requestChoice(title)`:
  - `'continue'` → `invoke('start_recording_window', { localAppendId: localRecordingId })`
  - `'new'` → `invoke('start_recording_window', {})` (ad-hoc, unchanged)
  - `null` → do nothing.

The "open meeting" is the `item` currently passed to `MeetingDetailView`. For local
meetings, `item.id` **is** the on-disk local recording id (the string
`most_recent_appendable`/`append_recording_core` use). For Ariso meetings we pass
the numeric meeting id and the title. *(Implementation must confirm the Ariso
numeric meeting-id field available on the open detail item.)*

**`src/views/MeetingPickerView.vue`**
On mount, read `defaultMeetingId` / `defaultMeetingTitle` from the route query
(`useRoute().query` / `window.location.hash`). When present, feature that meeting as
the default choice — synthesizing a featured entry from the passed id+title if it is
not in the fetched today list — instead of the client-side `pickDefaultMeeting`
heuristic. Selecting it uses the existing `start_recording_window({ meetingId })`.

**`src/views/WaveformView.vue`**
Read a new `localAppendId` route-query param. When present:
- set `effectiveLocalRecordingId` to it directly (so the recorder strip docks to the
  right library row from the start; skip the `recordingIdForStart` resolve), and
- pass it through to `finalizeRecording` as the append target.

### Backend (Rust)

**Local force-append — inject the target at finalize time.**
The finalize path is entirely `created_at`-driven today; that is the correct seam.

- `commands.rs` `start_recording_window(app, meeting_id: Option<i64>)` →
  add `local_append_id: Option<String>`; thread into
  `open_waveform_window(app, meeting_id, local_append_id, auto)` and append it to the
  waveform URL query (same pattern as `meetingId`). `local_append_id` is a **string**
  local recording id, orthogonal to the numeric Ariso `meeting_id`.
- `tauri.ts` `local.finalizeRecording(...)` and `useBackend.ts`
  `LocalBackend.finalizeRecording(...)` — add `appendTo?: string`.
- `transcribe.rs` `local_finalize_recording(audio, title, created_at, duration_seconds)`
  → add `append_to: Option<String>`; forward to `finalize_core`.
- `transcribe.rs` `finalize_core(...)` — when `append_to` is `Some(id)`, skip
  `most_recent_appendable` and call `append_recording_core(root, &id, ...)` directly.
  `append_recording_core` already re-validates the target is still `Done` with audio
  and falls back to `fresh_recording_core` otherwise, so a stale/invalid id degrades
  safely. When `None`, behavior is unchanged (existing 5-minute auto-append).

**Ariso picker default.**
- `commands.rs` `open_meeting_picker(app)` → add
  `default_meeting_id: Option<i64>, default_meeting_title: Option<String>`; thread
  into `open_meeting_picker_window` and append to the `/#/meeting-picker` URL query
  (use the existing query-building pattern, e.g. `waveform_url` helper). The tray
  entry point (`tray.rs`) keeps calling with `None` and is unaffected.

## Error handling / edge cases

- **Stale local id (recording deleted or mid-processing):** handled by
  `append_recording_core`'s existing re-validation → falls back to a fresh recording
  rather than erroring.
- **Cancel / dismiss:** no recording starts; no state changes.
- **No meeting open:** dialog/picker-default logic is skipped entirely; existing
  behavior preserved.
- **Ariso default not in today's list:** picker synthesizes the featured entry from
  the passed id+title; no extra network fetch required.

## Testing

**Rust (unit, in `transcribe.rs`/`storage.rs` test modules):**
- `finalize_core` with `append_to = Some(existing_done_id)` appends to that id even
  when the 5-minute window would not (start far outside the window).
- `finalize_core` with `append_to = Some(invalid_id)` falls back to a fresh recording.
- `finalize_core` with `append_to = None` preserves current auto-append behavior
  (regression guard).

**Frontend (Vitest, on the small units — not the brittle views):**
- `decideStartRecording` returns the right branch for each
  (backend, open-meeting) combination.
- `useRecordingStartChoice.requestChoice` resolves to
  `continue` / `new` / `null` for the three outcomes.
- `RecordingStartChoiceDialog` emits `continue` / `new` / `cancel` on the
  corresponding actions and renders the meeting title.

## Out of scope

- No change to the automatic 5-minute local append behavior when no meeting is open.
- No new "pick any past local meeting from a list" UI — continue applies only to the
  meeting currently open in the detail pane.
- No change to how audio clips are rendered in `MeetingDetailView`.
