# Resumable Ariso Uploads from the Library — Design

**Date:** 2026-06-14
**Status:** Approved

## Problem

When recording with the Ariso (cloud) backend and the network is down, stopping
the recorder fails the upload. The encoded MP3 is buffered to
`<ariso_root>/pending-uploads/<id>.mp3` (added in the upload-recovery work), but:

1. The buffer stores **only audio bytes** — no `meetingId`, `startAt`, `endAt`,
   or `durationSeconds`. Once the recorder window closes (or the app quits), the
   metadata needed to resume the upload is gone; only an orphan `.mp3` remains.
2. Retry lives **only** in the transient floating pill (`WaveformView`). The
   Library's embedded `RecorderStrip` renders a failed upload as static
   "Upload failed" text with **no resume control**, and once the pill is gone
   there is no way to resume from the Library at all.

The user reported this as: with the network down, after stopping, the recorder
appears done ("Recording saved") and there is no option to resume uploading.

## Goals

1. Persist enough metadata alongside each buffered MP3 that an upload can be
   resumed after an app restart.
2. Surface pending/failed uploads in the Library with a resume control that
   survives restart.
3. When more than one upload is pending, **combine** them into a single
   concatenated MP3 and upload once, producing one merged meeting.

## Non-goals

- No automatic retry (on launch or in the background); resume is user-initiated.
- No per-item retry in the Library — the Library action is all-or-nothing.
- No garbage collection of orphans beyond explicit discard / successful upload.
  An `.mp3` with no readable `.json` sidecar is left for manual recovery, as today.
- No change to the local backend's finalize flow.

## Decisions (resolved during brainstorming)

- **Durability:** survive app restart (persist a metadata sidecar; scan on demand).
- **Backend in scope:** Ariso (cloud) only.
- **Placement:** a pinned "Pending uploads" group at the top of the Library sidebar.
- **Multiple pending → one merged meeting:** concatenate all pending audio
  chronologically into a single MP3; the separate recordings do not survive as
  individual meetings.
- **Trigger:** a single primary **`Upload (N)`** button (auto-combine, all-or-
  nothing). No per-item upload/retry.
- **Discard:** a secondary **`Discard all`** button. No per-item discard in the
  Library (per-item discard still exists in the pill at stop-time).
- **Merged metadata:** `startAt` = earliest, `endAt` = latest,
  `durationSeconds` = **sum** of clips. In-between gaps (time not recording) are
  dropped; the combined audio plays back-to-back.

## Design

### 1. Persistence — metadata sidecar (`storage.rs`)

Each buffered recording gets a JSON sidecar next to its audio:

```
pending-uploads/<id>.mp3      # audio bytes (unchanged)
pending-uploads/<id>.json     # PendingUploadMeta
```

`<id>` remains the sanitized `createdAt` (`startAt ?? endAt`) via
`sanitize_iso_to_id`, so a retry under the same timestamp overwrites both files.

```rust
#[serde(rename_all = "camelCase")]
struct PendingUploadMeta {
    created_at: String,            // raw ISO; the buffer key (startAt ?? endAt)
    start_at: Option<String>,      // raw ISO or null
    end_at: String,                // raw ISO
    duration_seconds: u64,
    meeting_id: Option<u64>,       // usually absent (presign never ran offline)
}
```

Storage helpers:

- `write_pending_audio(root, meta: &PendingUploadMeta, bytes) -> Result<String>`
  — writes the audio atomically, then the sidecar atomically (audio first so a
  sidecar without audio is never observed); returns the id. Same path-traversal
  guard (`validate_pending_id`) as today.
- `discard_pending_audio(root, created_at)` — removes **both** files; missing
  files are not an error (idempotent). Existing callers unchanged.
- `list_pending_uploads(root) -> Result<Vec<PendingUploadMeta>>` — scans the
  dir, parses each `.json`, includes an entry only when its sibling `.mp3`
  exists. Skips unpaired/unparseable files (orphans). Sorted by `created_at`
  ascending (chronological).
- `read_pending_audio_bytes(root, created_at) -> Result<Vec<u8>>` — internal
  helper for combine; validates the id and reads the `.mp3`.

### 2. Tauri commands (`commands.rs`)

- `buffer_pending_audio(audio: Vec<u8>, meta: PendingUploadMeta) -> Result<String>`
  — signature changes from `(audio, created_at)` to `(audio, meta)`.
- `discard_pending_audio(created_at: String) -> Result<()>` — unchanged
  signature (now deletes both files via the updated storage helper).
- `list_pending_uploads() -> Result<Vec<PendingUploadMeta>>`.
- `combine_pending_audio(created_at_keys: Vec<String>) -> Result<tauri::ipc::Response>`
  — concatenates the given keys' `.mp3` bytes **in the order provided**
  (the frontend passes them chronologically). Each key is validated and read
  via `read_pending_audio_bytes`; a missing key is an error. The running total
  is checked against `MAX_AUDIO_BYTES` (1 GB) and rejected if exceeded. Returns
  the concatenated bytes as `tauri::ipc::Response` (same pattern as
  `read_recording_audio` / `fetch_meeting_audio`).

Taking an explicit ordered key list (rather than "combine whatever's there")
makes combine + discard operate on the same snapshot: a recording stopped
offline between list-and-discard is never clobbered, and combine can return
pure bytes (metadata is derived frontend-side from the same `list()` result).

MP3 frame concatenation: all clips are produced by the same in-app encoder
configuration, so concatenating the byte streams yields a stream players and the
transcription pipeline decode cleanly. Any leading encoder tag is tolerated as
inter-frame data. (No re-encode; YAGNI.)

### 3. Frontend bridge + finalize (`tauri.ts`, `useBackend.ts`)

`pending` namespace in `tauri.ts`:

```ts
export interface PendingUploadMeta {
  createdAt: string;
  startAt: string | null;
  endAt: string;
  durationSeconds: number;
  meetingId?: number;
}
export const pending = {
  bufferAudio(audio: number[], meta: PendingUploadMeta): Promise<string> { … },
  discardAudio(createdAt: string): Promise<void> { … },
  list(): Promise<PendingUploadMeta[]> { … },
  combine(createdAtKeys: string[]): Promise<ArrayBuffer> { … },
};
```

`ArisoBackend.finalizeRecording` builds the `PendingUploadMeta` from the
`RecordingMeta` it already has and passes it to `bufferAudio`. Failure behavior
is unchanged: any upload-step error still throws; the buffer + sidecar remain on
disk.

### 4. Resume flow (combine + upload)

Lives in a small composable (e.g. `usePendingUploads.ts`) so the Library view
and tests share one implementation:

1. `items = await pending.list()` (already chronological).
2. `keys = items.map(i => i.createdAt)`.
3. `merged = { startAt: items[0].startAt ?? items[0].createdAt,
   endAt: items.at(-1).endAt,
   durationSeconds: sum(items.durationSeconds) }`. `meetingId` is omitted →
   the upload creates one new meeting.
4. `buf = await pending.combine(keys)` → `new Blob([buf], { type: 'audio/mpeg' })`.
5. `await useMeetingApi().uploadAudio(blob, merged)` — reuses the existing
   presign → PUT → confirm path unchanged.
6. On success: `await Promise.all(keys.map(k => pending.discardAudio(k)))`,
   then refresh the list. On failure: leave everything in place, surface an error.

`Discard all`: snapshot `keys` from `list()`, then discard each.

### 5. Library UI — `PendingUploads.vue`

A new component rendered at the top of the sidebar list in `LibraryView.vue`,
above the meeting sections. Hidden entirely when the list is empty.

- Group label "Pending uploads".
- Read-only rows: derived title `Recording HH:MM` (from `startAt ?? createdAt`,
  via the existing local formatting) + duration. No per-row controls.
- Primary **`Upload (N)`** button: runs the resume flow. While running it shows a
  spinner and is disabled; on failure it shows an error tick and re-enables.
- Secondary **`Discard all`** button: runs the discard-all flow (with a confirm,
  matching the pill's deliberate-discard semantics).

`LibraryView` loads the pending list on mount, and refreshes it after a resume,
after a discard-all, and when the `RecorderStrip` broadcasts a `closed` event
(so a just-failed upload appears without a manual reload).

### 6. Honest status (regression guard)

The reported symptom was the recorder appearing "saved" when the upload had
failed. The pill (`WaveformView`) and strip (`RecorderStrip`) already render the
`failed` phase as "Upload failed"; this design keeps that and adds a regression
test asserting a finalize failure never yields `phase: 'success'` /
"Recording saved". With the durable sidebar section, a failed upload is also
always visible and resumable in the Library regardless of the pill's transient
state.

### Error handling summary

| Failure | Behavior |
| --- | --- |
| Buffer write (audio or sidecar) fails | Log, continue with upload (in-memory blob still valid) |
| Upload fails (offline) | Pill shows ✗ + Retry/Dismiss; buffer + sidecar persist; item appears in Library's Pending uploads |
| Combine: a key missing | Error surfaced as upload failure; nothing discarded |
| Combine: total > `MAX_AUDIO_BYTES` | Error surfaced as upload failure; nothing discarded |
| Resume upload fails | Items left in place; section shows error; user can try again |
| Resume upload succeeds | Combined keys discarded; one new meeting created; list refreshed |
| `Discard all` | All listed buffers + sidecars deleted (explicit discard) |
| Orphan `.mp3` with no `.json` | Skipped by `list_pending_uploads`; left for manual recovery |
| Pending item had a `meetingId` (rare) | Combined upload still creates a new meeting; the pre-created server meeting is left empty |

## Testing

- **Rust (`storage.rs` / `commands.rs`):**
  - `write_pending_audio` writes both `.mp3` and `.json`; `discard` removes both;
    discard is idempotent; traversal ids rejected.
  - `list_pending_uploads` pairs files, sorts chronologically, skips orphan
    `.mp3` and unparseable `.json`.
  - `combine_pending_audio` concatenates in the requested order, errors on a
    missing key, and rejects when the combined size exceeds `MAX_AUDIO_BYTES`.
- **`useBackend.test.ts`:** Ariso finalize buffers audio + sidecar meta before
  upload, discards on success, leaves both on failure, and does not fail
  finalize when buffering itself fails.
- **`usePendingUploads` / `PendingUploads.vue`:** section hidden when empty;
  shows count and rows; `Upload (N)` combines (chronological keys), uploads with
  merged meta (earliest start / latest end / summed duration), discards the
  combined keys on success and leaves them on failure; `Discard all` clears.
- **Honest-status regression:** a finalize failure broadcasts `failed`, never
  `success` / "Recording saved".
