# Delete local meeting notes (issue #359)

## Problem

Local-backend recordings have no user-facing way to delete a meeting note.
The gap is real: `useMeetingApi.ts` only exposes `deleteMeetingRecordingClip`,
which is Ariso-only (`DELETE /meeting-notes/...`) and removes one recording
clip, not a whole local note; on the Rust side `MeetingDetailView.vue:909`'s
own comment says it outright — "oats has no whole-recording delete" — which
is why the per-clip delete button (`showPerClipDelete`) only ever appears for
Ariso meetings with 2+ real clips.

The cascade primitive already exists, unused: `vault::delete_recording_artifacts`
(`vault.rs:317`) removes a recording's vault note + audio attachment and is
unit-tested (`vault.rs:698`), but its only caller today is
`retry_notes_core` (`transcribe.rs:655`), which uses it to clear a stale note
before regenerating — not to delete a recording. The 2026-07-03 vault design
spec anticipated exactly this feature and deliberately left it undone:

> **Cascade delete (forward requirement).** Deleting a recording inside oats
> must remove its vault note + audio attachment as well as the private
> `~/.ariso` folder. **Note:** the app has no local "delete recording" command
> today... `vault::delete_recording_artifacts` is provided so that whoever
> adds local deletion later wires in the cascade.

So this is that follow-up. Users running the local backend have no button to
remove a note they no longer want, and the only way to get rid of one today
is deleting files by hand in `~/.ariso` and `~/.ariso/vault` — a filesystem
operation most users won't discover or trust themselves to do correctly.

## Goal

In local mode, a meeting note's detail view gets a discoverable **Delete**
action that, after an explicit confirmation, permanently removes the note,
its transcript, and its audio from the local library — and the meeting
disappears from the visible list immediately, staying gone after navigating
away, refreshing, or relaunching the app.

Reviewable as: open a finished local recording, click Delete, confirm, watch
it vanish from the sidebar and the detail pane close; relaunch the app and
confirm it's still gone; on disk, confirm `~/.ariso/recordings/<id>/` and the
vault note + attachment under `~/.ariso/vault/` are gone. Canceling the
confirmation, or a simulated backend failure, leaves everything untouched and
shows an inline error.

## Non-goals

- **Ariso (cloud) meeting deletion.** Out of scope per the issue; cloud notes
  have no delete endpoint wired up anywhere in this app, and building one is
  a separate feature with its own moderation/undo questions. `ArisoBackend`
  gets a `deleteMeeting` that throws, mirroring how `LocalBackend` already
  throws on the Ariso-only `deleteMeetingClip` (`useBackend.ts:612`) — a
  defensive backstop, since the UI never calls it for a non-local meeting.
- **Soft delete / trash / undo.** The vault design already settled this: a
  deleted vault note "is respected by construction" because oats has no
  spontaneous regeneration path. This feature is a hard, cascading delete —
  consistent with that decision, not a new one.
- **Per-clip delete changes.** `showPerClipDelete` / `RecordingDeleteConfirmDialog`'s
  existing Ariso multi-clip flow is untouched.
- **A delete affordance on Library list rows.** Rows are plain text today
  (no hover actions, no rename-from-list either — rename also lives only in
  the detail view). Delete follows that same precedent; see
  [Open questions](#open-questions) if quick-delete-from-list turns out to be
  wanted.
- **Guarding against deleting an actively-recording meeting.** Not reachable
  today: an in-progress local recording has no list row yet (LibraryView's
  own comment: "An in-progress local recording has no list row yet"), so it
  can't be opened in `MeetingDetailView` and therefore can't hit this delete
  path. No new guard needed.
- **Bulk / multi-select delete.**

## Design

### Backend: `delete_local_recording`

One new `#[tauri::command]` in `commands.rs`, a sibling of
`rename_local_recording` (`commands.rs:2035`) and `retry_notes_core`
(`transcribe.rs:643`), reusing the same `recording_dir` / `validate_recording_id`
guard everything else in this file uses:

```rust
/// Permanently delete a local recording: its vault note + audio attachment
/// (if any) and its private `<vault>/.oats/recordings/<id>` folder (meta,
/// transcript, and — for legacy pre-vault recordings — `ari-note.md` /
/// `recording.mp3`, which live in that same folder and go with it).
#[tauri::command]
pub fn delete_local_recording(id: String) -> Result<(), String> {
    let dir = recording_dir(&id)?;
    let meta = crate::storage::read_meta(&dir)?;
    crate::vault::delete_recording_artifacts(&id, meta.audio_file.as_deref())?;
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("delete recording folder: {e}")),
    }
}
```

Registered in `main.rs`'s `generate_handler![...]` next to
`commands::rename_local_recording`. No new capability entry — like the rest
of `commands::*`, this needs no capability entry itself (only plugin-provided
commands do); it takes no path from the frontend, only a recording id
resolved server-side.

**Order matters.** `delete_recording_artifacts` runs first, while
`meta.json` (and `meta.audio_file`) still exists to name the attachment; the
private folder — which holds `meta.json` — is removed last. If the vault
step fails (e.g. a permission-denied on the attachment), the command returns
`Err` before touching the folder, so the recording is untouched and still
lists normally rather than ending up half-deleted.

`delete_recording_artifacts(id, None)` is a safe no-op for a legacy
recording (pre-vault, `audio_file: None`, no vault note under that
`oats_id`) — its `ari-note.md`/`recording.mp3` live inside `dir` and are
removed anyway by `remove_dir_all`. So this one command correctly cascades
both new (vault-based) and legacy recordings without a branch.

### Frontend

- **`src/tauri.ts`**: new `local.deleteRecording(id): Promise<void>` wrapper
  next to `renameRecording` (`tauri.ts:432`):
  ```ts
  deleteRecording(id: string): Promise<void> {
    return invoke('delete_local_recording', { id });
  },
  ```
- **`useBackend.ts`**: new `Backend` method `deleteMeeting(item: MeetingListItem): Promise<void>`,
  next to `deleteMeetingClip`. `LocalBackend.deleteMeeting` calls
  `local.deleteRecording(item.id)`. `ArisoBackend.deleteMeeting` throws (see
  [Non-goals](#non-goals)).
- **`MeetingDetailView.vue`**: a **Delete** button in `.head-actions`
  (`MeetingDetailView.vue:40-55`), gated on `detail.isLocal`, sitting before
  the Close button (Share already conditionally appears in that row, so
  Delete joins it rather than needing new layout). Clicking it opens a
  confirmation dialog; confirming calls `backend.deleteMeeting(item)` and, on
  success, emits a new `deleted` event with the meeting id, then the same
  `emit('close')` the Close button already sends (so the pane clears itself
  the same way it does today).
- **Confirm dialog**: generalize `RecordingDeleteConfirmDialog.vue` rather
  than duplicating it — add optional `title`/`body` props (defaulting to the
  existing per-clip copy) so the whole-meeting flow passes its own copy:
  `title="Delete this meeting?"`, `body="This permanently deletes the note,
  transcript, and audio for this meeting. This can't be undone."` The
  dialog's existing `confirm`/`cancel` emit contract is unchanged.
- **State**, mirroring the existing per-clip pattern
  (`showClipDeleteConfirm`/`deletingClip`/`clipDeleteError`,
  `MeetingDetailView.vue:913-919`): `showMeetingDeleteConfirm`,
  `deletingMeeting`, `meetingDeleteError`. `confirmDeleteMeeting()` guards
  against a second concurrent delete the same way `confirmDeleteClip` does,
  shows `meetingDeleteError` inline in the header area on failure, and
  otherwise emits `deleted` + `close`.
- **`LibraryView.vue`**: a new `@deleted="onMeetingDeleted"` handler beside
  the existing `@title-updated="onTitleUpdated"` wiring
  (`LibraryView.vue:227-228`), following the same shape as `onTitleUpdated`
  (`LibraryView.vue:623-629`): remove the row from `meetings.value`, drop it
  from `pinnedMeetings` if present, and call `clearSelection()` (the same
  function the Close button's `@close` already triggers, so `deleted` only
  needs to additionally prune the list — `close`'s existing handler still
  clears `selectedItem`).

## Cloud vs offline

**Local only.** The Delete button is gated on `detail.isLocal`; Ariso
meetings are untouched, and `ArisoBackend.deleteMeeting` throwing is a
backstop the UI never exercises. Nothing in this feature makes a network
call — deletion is local filesystem removal driven by an id already in
memory — so it's compatible with the offline-mode privacy guarantee by
construction.

## Error handling

- **Confirmation canceled**: dialog closes, nothing happens, note untouched.
- **Recording already gone** (double-click, or deleted by another window):
  `read_meta` fails, the command returns `Err` before any filesystem
  mutation, and the UI shows `meetingDeleteError` ("Could not delete this
  meeting. Please try again."). The list still shows whatever it showed
  before; if the recording is genuinely gone, the next `loadMeetings()`
  refresh drops it naturally.
- **Vault deletion fails** (permission denied on the attachment, disk
  issue): the command returns `Err` before removing the private folder — the
  note, transcript, and `meta.json` are all still intact, so "failure states
  preserve the note" holds exactly.
- **Folder removal fails after vault deletion already succeeded** (rare: the
  vault note/attachment are gone but `remove_dir_all` on
  `~/.ariso/recordings/<id>` fails, e.g. permission changed mid-flight): the
  command still returns `Err`, but the recording is left in an inconsistent
  state — its private folder (and stale `meta.json`) survive with no vault
  note. This is a real but narrow edge case (both paths are under the same
  `~/.ariso` tree with the same permissions in the overwhelmingly common
  case); accepted here rather than building a two-phase/transactional delete
  for it. Flagged explicitly in [Open questions](#open-questions).
- **Frontend catch**: any rejected `deleteMeeting` sets `meetingDeleteError`
  inline, the confirm dialog has already closed by then (same UX as
  `clipDeleteError` today), and the meeting stays selected and in the list —
  the user can retry.

## Testing

- **Rust (`commands.rs`)**: `delete_local_recording` removes the vault note +
  attachment and the entire `recording_dir` for a vault-based recording
  (`meta.audio_file` set); for a legacy recording (`audio_file: None`,
  `ari-note.md`/`recording.mp3` on disk, no vault entry) the same call
  removes the whole folder with the vault step a no-op; rejects a missing id
  without touching disk (reuses the existing "missing recording" shape from
  `rename_local_recording_rejects_missing_recording`); a traversal id is
  rejected by the existing `recording_dir` guard
  (`recording_dir_rejects_traversal_ids`, already tested — reused, not
  reimplemented).
- **`useBackend.test.ts`**: `LocalBackend.deleteMeeting` calls
  `local.deleteRecording` with the item's id (mirrors the existing
  `deleteMeetingClip calls deleteMeetingRecordingClip` test); `ArisoBackend.deleteMeeting`
  rejects (mirrors `deleteMeetingClip rejects as unsupported`, inverted).
- **`MeetingDetailView.test.ts`**: Delete button renders only when
  `detail.isLocal`; clicking opens the confirmation dialog with the
  meeting-specific copy; Cancel leaves the note and calls neither
  `deleteMeeting` nor `close`; Confirm calls `backend.deleteMeeting` then
  emits `deleted` with the meeting id and `close`; a rejected `deleteMeeting`
  shows `meetingDeleteError`, leaves the pane open, and emits neither event.
- **`LibraryView.test.ts`**: `onMeetingDeleted` removes the matching row from
  `meetings.value`, clears a matching `pinnedMeetings` entry if present, and
  clears the current selection.
- **Manual**: record locally, wait for it to finish, open it, Delete →
  Cancel (note still there, still selected) → Delete → Confirm (note gone
  from the sidebar, pane closes); relaunch the app and confirm it does not
  reappear; on disk, confirm `~/.ariso/recordings/<id>/` and the vault note +
  `Attachments/*.mp3` are both gone. Separately, open an Ariso meeting and
  confirm no Delete button appears (regression check for
  [Non-goals](#non-goals)).

## Open questions

- **The two failure-order edge cases above** (a `read_meta` race on an
  already-gone recording, and a `remove_dir_all` failure stranding a
  vault-less private folder) — confirmed as accepted, narrow risks rather
  than something worth a transactional delete to close.
- **List-row quick delete.** This spec keeps Delete inside the detail view
  only, matching where rename already lives and keeping list rows plain.
  Confirm that's sufficient for v1, versus also wanting a hover/right-click
  delete directly on a Library row.
