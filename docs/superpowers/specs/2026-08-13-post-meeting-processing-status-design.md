# Post-meeting processing status (issue #315)

## Problem

Once a recording ends, the only feedback the user gets is a green checkmark
on the floating recorder pill (`WaveformView.vue`'s `uploadResult === 'success'`
branch) that auto-closes after ~1.5s. After that the pill is gone and nothing
in the Library says what's happening to the meeting. Concretely:

- **Cloud (Ariso).** `MeetingDetailView.vue` fetches a meeting's notes exactly
  once per selection (the `load()` watcher at line ~898, keyed on
  `props.item?.id`/`timestamp`/etc. — it never re-fires while a meeting stays
  selected). While the transcript/notes haven't landed yet, `availableTabs`
  is empty and the pane shows a flat `"No notes available for this meeting
  yet."` (line 176) — identical to what a meeting with genuinely no content
  ever will show. There is no polling, no chip, and the sidebar row
  (`LibraryView.vue`'s `meeting-item`, lines 138-152) shows only the title and
  a time/duration sub-line — nothing about upload or processing state. A user
  has no way to tell "still processing" from "something went wrong" from
  "this meeting will never have notes."
- **Local (offline).** The backend pipeline already models this correctly —
  `RecordingStatus::Recording → Transcribing → Done` and
  `NotesStatus::Pending → Ready/Failed` (`src-tauri/src/storage.rs`,
  `src-tauri/src/transcribe.rs`) — and the frontend already polls it
  (`useLocalRecordingProgress.ts`, 2s interval) to drive a status chip
  (`MeetingDetailView.vue`'s `showStatusChip`/`statusLabel`, "Generating
  Transcript" / "Generating AI Notes"). But that chip is gated to
  `detail.value?.isLocal` and only renders once the meeting is open in the
  detail panel — the sidebar row gives no indication a meeting is still being
  transcribed, even though the row data (`MeetingListItem.status`,
  `RecordingSummary['status']` in `src/tauri.ts`) already carries the
  information needed to show it.

So the acceptance criteria in the issue — visible uploaded/processing state,
distinct from uploading and from completed/failed, accurate through the
transitions — is unmet for cloud entirely, and only half-built (detail-panel
only) for local.

## Goal

From the moment a recording finishes uploading (cloud) or capture stops
(local) until its transcript/notes exist, both the Library sidebar row and
the open detail panel show an explicit "uploaded, processing" state that is
visually distinct from:

- upload-in-progress (already shown by the recorder pill/strip's spinner),
- ready (existing content renders normally, unchanged), and
- upload failed (the existing failed pill / `PendingUploads.vue` error,
  unchanged).

This applies to meetings recorded in the current app session. Recovering the
indicator for a meeting still processing from a *previous* session is called
out below as an explicit limitation, not solved here.

## Non-goals

- No backend/API changes. Ariso exposes no distinct "processing" status for
  audio-uploaded meetings (`ScheduledMeeting.status` / `MeetingNotes.status`
  only carry the calendar-call lifecycle: `created`/`joined`/`done`/
  `cancelled`, per `src/composables/meetingStatus.ts`). The frontend infers
  processing from content absence instead.
- No detection of cloud-side "processing failed" (transcription/notes
  generation erroring out after a successful upload). There is no signal for
  this today — the audio-confirm endpoint (`POST
  /desktop/meetings/{id}/audio/confirm`) just returns 202 and nothing further
  is pushed to the client. Only the local pipeline can report a genuine
  processing failure (`RecordingStatus::Failed` / `NotesStatus::Failed`,
  already surfaced). If cloud processing silently fails server-side, this
  design has no way to tell that apart from "still processing" — flagged
  under Open questions.
- No persistence of processing state across an app restart. oats is a
  persistent menu-bar app that's rarely force-quit mid-processing; if it does
  happen, the meeting falls back to today's plain "No notes available yet."
  state until content exists, with no chip. Making this durable would need
  either a backend status field or a disk-persisted marker with a staleness
  timeout — real scope, deliberately deferred (see Open questions).
- No live background polling for local rows other than the one currently
  selected in the detail panel. A backgrounded local recording's row updates
  on the next natural list reload (window focus, mount, backend switch, the
  selected meeting's own poll completing) rather than a dedicated poll — the
  local multi-recording case where two meetings transcribe in the background
  concurrently is a minor UX imperfection, not a correctness bug.
- No change to the Ari-bot call-lifecycle chips (`ariJoinChip`, "will join" /
  "has joined") — those are a different pipeline and already behave
  correctly.

## Design

### Readiness check (shared)

Add a small predicate next to `MeetingNotes` in `src/composables/useMeetingApi.ts`:

```ts
export function isMeetingNotesReady(n: Pick<MeetingNotes, 'hasTranscript' | 'summary'>): boolean {
  return !!n.hasTranscript || !!n.summary;
}
```

`hasTranscript` and `summary` are exactly the fields `ArisoBackend
.getMeetingDetail` (`src/composables/useBackend.ts`) already reads off the
same `/meeting-notes/:id` payload to decide whether the Transcript/AI-Notes
tabs exist (`notesPresent`/`availableTabs` in `MeetingDetailView.vue`). One
truthy value means the pipeline has produced *something* — good enough to
call it "no longer processing" without needing to reach full done-ness on
every field (assessment/coaching lag notes and shouldn't hold up the chip).

### Cloud tracking — new `src/composables/useMeetingProcessing.ts`

A module-scoped (singleton) reactive store, session-only, holding the set of
Ariso meeting ids the app knows it just uploaded and hasn't confirmed ready:

- `markUploaded(meetingId: string): void` — add to the tracked set.
- `isProcessing(meetingId: string): boolean` — reactive membership check.
- Internally: while the set is non-empty, poll every 5s (heavier round-trip
  than local's on-disk check, and less latency-sensitive — this is a
  background confirmation, not a progress bar) via `getMeetingNotes(id)`
  from `useMeetingApi.ts`. A ready id is removed from the set; a request
  failure leaves it tracked (keep showing "processing" through a transient
  network blip, mirroring `useLocalRecordingProgress`'s existing retry
  behavior). No age cap — the set is session-scoped and empties itself as
  ids resolve or, worst case, when the app restarts.
- Exposes a reactive `version` counter (or equivalent) that bumps whenever
  the set shrinks, so callers can react to "something just became ready"
  without re-deriving membership on every poll tick.

Two call sites mark a meeting uploaded:

1. `LibraryView.vue`'s `onRecorderPhase` — in the existing `phase ===
   'success'` branch (line ~756), when `activeBackend.value?.id ===
   'ariso'` and `recordingMeetingId.value` is set (it's still populated at
   this point — see the existing comment above `onRecorderPhase` about
   `RecorderStrip` clearing it *after* this callback), call
   `markUploaded(recordingMeetingId.value)`.
2. `PendingUploads.vue`'s `onUpload` (retry of buffered recordings) — today
   `combineAndUpload` (`src/composables/usePendingUploads.ts`) uploads each
   group and discards its buffer but discards the returned `meetingId` too.
   Change `uploadGroup`/`combineAndUpload` to return the uploaded meeting
   ids, and have `onUpload` call `markUploaded` for each before emitting
   `uploaded`.

Local recordings need no equivalent — their processing state already lives
on disk and is durable across restarts; there's nothing to "mark."

### List row UI (`LibraryView.vue`)

Add a `rowProcessingLabel(m: MeetingListItem): string | null` used by the
`meeting-item` template (around line 151, alongside the existing `mi-sub`):

- Local: `m.status === 'recording' || m.status === 'transcribing'` → mid
  transcription; `m.status === 'done' && m.files?.hasTranscript &&
  !m.files?.hasNote` → notes still generating. Both read fields
  `MeetingListItem` already carries (`recordingToListItem` in
  `useBackend.ts`) — no new plumbing needed for local.
- Cloud: `isProcessing(m.id)` from `useMeetingProcessing`.

When either is true, render a small inline indicator in place of the normal
time/duration sub-line — a small spinner (reusing the `.spinner`/
`.spinner--sm` pattern already used in `MeetingDetailView.vue` and
`PendingUploads.vue`) plus the text `Processing…`. Keep it terse; the row is
narrow (~250px sidebar, per `PendingUploads.vue`'s own comment on why it
wraps to two lines).

Trigger a row refresh when state changes:

- Cloud: watch `useMeetingProcessing`'s `version` counter in `LibraryView.vue`
  and call `loadMeetings()` when it bumps, so a row whose meeting just became
  ready picks up its real sub-line on the next tick. The set is normally 0-1
  items, so this is cheap.
- Local: `MeetingDetailView.vue` already watches `progress.stage` transitions
  for the selected meeting (`showStatusChip`'s dependency). Add a new
  `content-ready` emit fired the moment that stage crosses into a terminal
  state (`ready`/`transcript-failed`/`notes-failed` from `idle`/
  `transcribing`/`notes-pending`), and have `LibraryView.vue` call
  `loadMeetings()` on it — mirroring the existing `title-updated` emit
  pattern between the two components.

### Detail panel UI (`MeetingDetailView.vue`)

Generalize the existing chip instead of adding a parallel one:

- `showStatusChip` currently gates on `!!detail.value?.isLocal`. Add a cloud
  branch: `!detail.value?.isLocal && processingMeetings.isProcessing(detail
  .value.id)`. (Guarded by the id being in the tracked set at all — an old
  meeting nobody uploaded this session falls through to today's unchanged
  "No notes available for this meeting yet." empty state.)
- `statusLabel`: add a cloud case, e.g. `"Uploaded — processing transcript &
  notes…"`. Cloud has one combined stage (no separate transcript/notes
  phases like local), so no need to mirror `LocalProgressStage`'s split.
- `statusGenerating` stays true for the cloud case (spinner, no Retry
  button) — there is no cloud "failed" stage to offer a retry for (see
  Non-goals).
- Once `isMeetingNotesReady` flips true (picked up by the composable's poll),
  the chip disappears and `availableTabs`/tab content render normally on the
  next `loadMeetings()`-triggered reload — no separate code path needed,
  since content presence already drives `availableTabs` today.

## Cloud vs offline

They diverge in mechanism, not in the UI contract:

- **Cloud** has no backend-exposed processing signal, so "processing" is
  purely a client-side inference: track meetings this session just uploaded,
  poll `getMeetingNotes` until content shows up. It cannot distinguish a
  slow-but-healthy pipeline from a silently failed one, and it forgets
  everything on app restart.
- **Local** already has a real, disk-backed state machine
  (`RecordingStatus`/`NotesStatus`) that survives restarts and *can*
  distinguish failure from in-progress. This design only extends where that
  existing state is surfaced (the sidebar row), it doesn't change the
  pipeline itself.

Both end up showing the same shape of indicator (spinner + "Processing…") in
the row, and a chip in the detail panel — but local's chip keeps its existing
specific copy and Retry action; cloud's is generic and has no Retry.

## Error handling

| Situation | Behavior |
| --- | --- |
| Cloud: upload itself fails | Unchanged — existing failed pill / `PendingUploads` error path. Never reaches `markUploaded`. |
| Cloud: `getMeetingNotes` poll request fails (network blip) | Meeting stays in the tracked set; chip/row keep showing "Processing…"; poll retries next tick. |
| Cloud: processing never completes server-side | No distinct state — indicator persists indefinitely for the session (see Non-goals/Open questions). Not worse than today's silent "No notes yet.", but not a real fix either. |
| Local: transcription fails | Unchanged — `RecordingStatus::Failed` already drives the "Transcript failed" chip with Retry; now also reflected in the row via `rowProcessingLabel` returning `null` once `m.status === 'failed'` (row falls back to its normal sub-line; the failure is visible once the meeting is opened, same as today). |
| Local: notes generation fails | Unchanged — `NotesStatus::Failed` already drives "AI Notes failed" with Retry. Row shows "Processing…" only up to that point (`hasNote` stays false, but `status` is `'done'`, not `'failed'` — see below). |
| App restarted mid-processing | Cloud: tracked set is empty; meeting shows the old ambiguous empty state until it happens to have content. Local: unaffected, disk state is authoritative regardless of restarts. |

One subtlety worth flagging: local's row heuristic
(`m.status === 'done' && hasTranscript && !hasNote`) can't tell "notes
pending" from "notes failed" without the per-recording status view (only the
detail panel's poll knows `notesStatus`). A local notes failure will show
"Processing…" in the row until the user opens the meeting and sees "AI Notes
failed" in the chip — an acceptable simplification for a list row, not
worth carrying `notesStatus` onto every list item for.

## Testing

- `useMeetingApi.test.ts` — `isMeetingNotesReady`: true on `hasTranscript`,
  true on non-empty `summary`, false when both are absent.
- New `useMeetingProcessing.test.ts` — `markUploaded` adds to the set;
  `isProcessing` reflects membership; a poll tick that finds
  `isMeetingNotesReady` removes the id and bumps `version`; a poll tick that
  throws leaves the id tracked; multiple tracked ids poll independently.
- `usePendingUploads.test.ts` — `combineAndUpload` returns the uploaded
  meeting ids (existing success/failure cases extended, not replaced).
- `LibraryView.test.ts` — cloud row shows "Processing…" after
  `onRecorderPhase('success')` with a tracked meeting id, and clears once
  the tracked poll (mocked) reports ready; local row shows "Processing…" for
  `status: 'transcribing'` and for `status: 'done'` with
  `hasTranscript && !hasNote`; `content-ready` emit from the detail view
  triggers `loadMeetings()`.
- `MeetingDetailView.test.ts` — cloud chip renders "Uploaded — processing
  transcript & notes…" with a spinner and no Retry button while tracked and
  not yet ready; chip absent for an untracked meeting with no content
  (unchanged empty state); chip absent once content exists; local chip
  behavior unchanged (existing tests keep passing).
- Manual check against both backends (per `oats-debugging`): record a short
  cloud meeting, confirm the pill's checkmark hands off to the row/chip
  "Processing…" and that it clears once notes land (few seconds to a minute
  typically); record a short local meeting and confirm the sidebar row now
  shows "Processing…" while the detail panel's existing chip is up, and both
  clear together.

## Open questions

- Should cloud processing state survive an app restart? Doing so needs a
  disk-persisted marker (meeting id + upload timestamp) with a staleness
  timeout to avoid showing "Processing…" forever if something really did
  fail server-side — real added complexity this spec leaves out. Worth
  doing only if quitting oats mid-processing turns out to be common enough
  to matter.
- Is a 5s poll interval right for the cloud case, or should it back off over
  time (e.g. 5s for the first minute, slower after)? Most meetings should
  finish processing well within a minute; a fixed 5s poll for a meeting that
  takes 20 minutes is 240 wasted requests. Not a scaling concern at today's
  usage, but worth a cheap backoff if it's easy.
- Exact copy for the row indicator and the cloud detail chip ("Processing…"
  vs "Uploaded, processing…" vs something else) — flagged for review rather
  than settled here.
