# Start recording: detail-pane empty vs populated

**Date:** 2026-07-07
**Status:** Design approved
**Branch:** `fix/local-start-empty-detail-new`
**Relates to / revises:** `2026-07-05-recording-start-new-vs-continue-design.md` (PR #206, issue #174)

## Problem

Clicking **Start recording** when no meeting is shown in the detail pane silently
attaches the new session to a previous/now meeting:

- **Local (offline):** the backend auto-appends to the most-recent recording when it
  starts within a 5-minute window (`most_recent_appendable`). With nothing open, the user
  gets a silent continuation they did not ask for.
- **Ariso (cloud):** in the Today view with nothing deliberately open, the flow records
  the "now"/selected meeting directly (`decideRecordingAction` → `record`).

The user wants: **if the detail pane is empty, Start recording begins a fresh recording;
it must not continue/attach to a prior meeting.** When a meeting *is* shown, the existing
"continue" affordance stays.

## Behavior

The discriminator is **`selectedItem`** in `LibraryView.vue` — the meeting rendered in the
detail pane (`MeetingDetailView` renders under `v-if="selectedItem"`). "Populated" means
`selectedItem != null` (including an auto-selected-on-mount row); "empty" means
`selectedItem == null`.

| Detail pane | Local | Ariso |
|---|---|---|
| **Empty** (`selectedItem == null`) | New recording, **no** 5-min auto-append | Open meeting picker with **no** featured default |
| **Populated** (`selectedItem != null`) | Keep the 5-min auto-append (`start_recording_window({})`) | Open meeting picker with the shown meeting as the **featured default** |

**Consequence (intentional):** because the trigger is now "a meeting is shown" rather than
"a meeting was deliberately opened", PR #206's local **New/Continue dialog no longer fires**
— a populated-local Start is the silent 5-min append. PR #206's Ariso featured-picker
behavior is retained, now keyed on `selectedItem`. The dialog components
(`RecordingStartChoiceDialog.vue`, `useRecordingStartChoice.ts`) are left in place but
unused rather than deleted, to keep this change focused.

## Architecture

### Frontend

**`src/composables/decideStartRecording.ts`** — reshape the pure helper to key on the
detail pane and the backend. New input/output:

```ts
export interface StartRecordingInput {
  usesPicker: boolean;                 // Ariso = true, local = false
  detailOpen: boolean;                 // selectedItem != null
  // The meeting shown in detail, when populated (for the Ariso featured default).
  shownMeeting: { numericId: number | undefined; title: string } | null;
}

export type StartRecordingPlan =
  | { kind: 'local-new' }                                            // forceNew recording
  | { kind: 'local-continue' }                                       // 5-min auto-append (start_recording_window({}))
  | { kind: 'ariso-picker'; defaultMeetingId: number | null };
```

> **Revised during implementation:** the `open_meeting_picker` command accepts only
> `default_meeting_id`, so the plan/input carry the meeting id alone — no
> `defaultMeetingTitle`. The picker resolves the featured meeting's title from its own
> fetched list (matching the pre-existing #206 wiring). `shownMeeting` is
> `{ numericId: number | undefined }`.

Decision:
- `usesPicker` (Ariso): always `ariso-picker`. When `detailOpen`, pass
  `defaultMeetingId` from `shownMeeting`; when empty, `null`.
- local + `detailOpen` → `local-continue`.
- local + empty → `local-new`.

**`src/views/LibraryView.vue` — `startRecording()`** — replace the current
`open`/`decideRecordingAction` wiring for the Start button with:

- Compute `detailOpen = selectedItem.value != null` and `shownMeeting` from
  `selectedItem.value` (`numericMeetingId(selectedItem.value)`).
- `ariso-picker` → `invoke('open_meeting_picker', args)` where `args` includes
  `defaultMeetingId` only when non-null (empty → `{}`).
- `local-continue` → `invoke('start_recording_window', {})`; `setRecording(true)`.
- `local-new` → `invoke('start_recording_window', { forceNew: true })`; `setRecording(true)`.

`decideRecordingAction` is no longer used by the Start button (it may remain for other
callers if any; otherwise it and its test become dead — leave them, out of scope to remove).

**`src/views/WaveformView.vue`** — read a new `forceNew` route-query flag (mirrors
`localAppendId`). When set:
- resolve `effectiveLocalRecordingId` to this session's own new id (skip the
  `recordingIdForStart` append resolve), and
- pass `forceNew: true` through to `finalizeRecording`.

**`src/tauri.ts` / `src/composables/useBackend.ts`** — `local.finalizeRecording(...)` /
`LocalBackend.finalizeRecording(...)` add `forceNew?: boolean` (alongside the existing
`appendTo?`). `recordingIdForStart(createdAt, forceNew?)` gains an optional `forceNew`.

### Backend (Rust) — `forceNew` seam (mirror of #206's `local_append_id`)

- `commands.rs` `start_recording_window(...)` → add `force_new: Option<bool>`; thread into
  `open_waveform_window(...)` and append `forceNew=1` to the waveform URL query when true
  (same pattern as `local_append_id`).
- `transcribe.rs` `local_finalize_recording(...)` → add `force_new: bool`; forward to
  `finalize_core_with_target`.
- `transcribe.rs` `finalize_core_with_target(...)` → when `force_new` is true (and no
  explicit `append_to`), skip `most_recent_appendable` and call `fresh_recording_core`
  directly. `append_to = Some(..)` still wins (explicit continue); `force_new = false` +
  `append_to = None` keeps today's 5-minute auto-append (regression-safe).
- `transcribe.rs` `local_recording_id_for_start(...)` → add `force_new: bool`; when true,
  return the new recording's own id (`storage::sanitize_iso_to_id(&created_at)`) instead of
  resolving the append target, so the recorder docks to a fresh row.

Ariso needs no backend change — the empty case just opens the picker; the populated case
opens the picker with a featured default (existing `open_meeting_picker` args from #206).

## Error handling / edge cases

- **forceNew + a stale/legacy state:** `fresh_recording_core` is the normal creation path;
  nothing to validate.
- **Empty detail, Ariso, no meetings at all:** picker opens in its existing empty state
  ("Record a new meeting"), unchanged.
- **Populated but the shown meeting is a local recording under Ariso backend:** not
  possible — the active backend determines the list; `usesPicker` gates the Ariso path.

## Testing

**Frontend (Vitest, on the pure helper — the views are brittle):**
- `decideStartRecording`: the 2×2 table —
  - local + empty → `local-new`
  - local + populated → `local-continue`
  - Ariso + empty → `ariso-picker` with `defaultMeetingId: null`
  - Ariso + populated → `ariso-picker` with the shown meeting's numeric id
  - Ariso + populated but `numericId` undefined → `defaultMeetingId: null` (graceful)

**Rust (unit, in `transcribe.rs` test module, using the `ARISO_STT_BIN`/`ARISO_ROOT` stubs):**
- `finalize_core_with_target` with `force_new = true` starts a fresh recording even when a
  recent Done recording is inside the 5-minute window (no append).
- `force_new = false`, `append_to = None` still auto-appends inside the window (regression).
- `append_to = Some(id)` still appends even with `force_new = true` (explicit continue wins).
- `local_recording_id_for_start(created_at, force_new = true)` returns
  `sanitize_iso_to_id(created_at)` (the new id), not the appendable target.

**Manual E2E:** local — with the detail pane empty, Start records a new meeting (verify the
vault gets a new note, not an append); with a meeting shown, Start within 5 min still
appends. Ariso — empty Start opens the picker with no default; Start with a meeting shown
opens the picker with it featured.

## Out of scope

- No deletion of the (now-unused) local New/Continue dialog components.
- No change to the tray/pill entry points.
- No change to Ariso session/clip semantics beyond which meeting the picker features.
