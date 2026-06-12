# Ariso Upload Recovery & Audio Playback — Design

**Date:** 2026-06-12
**Status:** Approved

## Problem

When recording with the Ariso backend, the encoded MP3 exists only in the
recorder window's memory. If any step of the upload (presign, S3 PUT, confirm)
fails after the user stops recording, the pill shows a dead red "✗" with no
retry path, and the audio is lost permanently. Additionally, Ariso meetings
have no in-app audio playback, while local recordings do.

## Goals

1. Buffer the audio locally (on disk) before the Ariso upload attempt so a
   failure or crash cannot lose it.
2. Let the user retry the upload from the failed pill, or dismiss the pill
   (dismissing discards the recording deliberately).
3. Show an audio player for Ariso meetings, backed by
   `GET {API_BASE_URL}/meeting-notes/{meeting-id}/audio` (returns raw audio
   bytes; requires the session Bearer token).

## Non-goals

- No pending-upload list in the Library; retry lives only in the pill.
- No automatic retry (on launch or in the background).
- No garbage collection of orphaned buffer files; a crash leaves a plain
  playable `.mp3` on disk for manual recovery.
- No change to the local backend's finalize flow (it already persists audio
  before transcription).

## Design

### 1. Local buffering (Rust)

New directory: `<ariso_root>/pending-uploads/` (sibling of `recordings/`).

New Tauri commands in `src-tauri/src/commands.rs`, helpers in `storage.rs`:

- `buffer_pending_audio(audio: Vec<u8>, created_at: String) -> Result<String, String>` —
  sanitizes the ISO timestamp into an id via `sanitize_iso_to_id`, writes
  `pending-uploads/<id>.mp3` via the existing `write_atomic` helper, and
  returns the id. Sanitization stays Rust-only; the frontend never derives ids.
- `discard_pending_audio(id: String) -> Result<(), String>` — deletes
  `pending-uploads/<id>.mp3`; missing file is not an error (idempotent).

`discard_pending_audio` validates the id with the same path-traversal guard as
`recording_dir` (reject empty, `/`, `\\`, `:`, `..`).

Frontend bridge in `src/tauri.ts` under a small `pending` namespace.

### 2. Finalize flow (`ArisoBackend.finalizeRecording`)

Sequence (in `src/composables/useBackend.ts`):

1. `createdAt = meta.startAt ?? meta.endAt` (raw ISO timestamp).
2. `buffer_pending_audio(bytes, createdAt)` → `id` (Rust sanitizes the
   timestamp into the id). A buffering failure is logged but does NOT block
   the upload attempt (buffering is a safety net).
3. `uploadAudio(blob, …)` (existing presign → PUT → confirm).
4. On confirm success: `discard_pending_audio(id)` (best-effort; a failed
   delete is logged, not thrown).
5. Return `{ backend: 'ariso', meetingId, pendingId: id }`.

`finalizeRecording` keeps its current error behavior: any upload-step failure
still throws to the caller. The buffer file remains on disk in that case.

### 3. Retry / dismiss in the pill (`WaveformView.vue`)

State additions:

- Keep the stopped recording's `mp3Blob` and meta (`startAt`, `endAt`,
  `durationSeconds`, `meetingId`) in refs after `handleStop` so retry can
  re-run finalize without re-recording.
- Track the pending buffer id (derived/returned during finalize) for dismiss.

UI in the `uploadResult === 'failed'` template branch, alongside the red ✗:

- **Retry** button: sets `isUploading`, clears `uploadResult`, re-runs the
  same `finalizeRecording` + 120 s timeout race. Success → existing success
  path (green ✓, auto-close after 1.5 s). Failure → back to the failed state.
- **Dismiss** (✕) button: calls `discard_pending_audio(id)` (best-effort),
  broadcasts `closed`, closes the window. This is an explicit discard.

The `RecorderPhase` union and the strip's rendering of `failed` are unchanged;
the retry attempt re-broadcasts `uploading` → `success`/`failed` through the
existing watchers.

The `expand()` hover gate stays as-is (no expand during upload/result states);
the retry/dismiss buttons render directly in the failed branch.

### 4. Ariso audio player

New Tauri command `fetch_meeting_audio(meeting_id: String)`:

- GET `{api_base_url}/meeting-notes/{meeting_id}/audio` with
  `Authorization: Bearer <session token>`.
- 200 → raw bytes as `tauri::ipc::Response` (same pattern as
  `read_recording_audio`), guarded by `MAX_AUDIO_BYTES`.
- Non-200 → `Err` whose message starts with the status code (e.g. `"404: no
  audio"`) so the frontend can distinguish "no audio" from a real failure.
- `meeting_id` is validated as digits-only before interpolation into the URL.

Frontend:

- Generalize `src/views/RecordingAudioPlayer.vue`: replace the hardcoded
  `local.readRecordingAudio(props.id)` call with a required
  `load: () => Promise<ArrayBuffer>` prop. Existing local usage passes
  `() => local.readRecordingAudio(id)`; behavior is otherwise identical
  (lazy fetch on Play, Blob URL, native `<audio controls>`).
- `MeetingDetailView.vue` renders the player for Ariso meetings (non-local
  detail), passing `() => api.fetchMeetingAudio(meetingId)`. A `404` from the
  load surfaces as the player's existing "No audio" state; other errors show
  "Failed".

### Error handling summary

| Failure | Behavior |
| --- | --- |
| Buffer write fails | Log, continue with upload (in-memory blob still valid) |
| Presign / PUT / confirm fails | Pill shows ✗ + Retry + Dismiss; buffer stays on disk |
| Retry fails again | Same failed state; can retry repeatedly |
| App crashes while failed | `pending-uploads/<id>.mp3` remains for manual recovery |
| Dismiss clicked | Buffer deleted, window closes — explicit discard |
| Buffer delete fails (post-success or dismiss) | Log only; orphan file is harmless |
| Audio fetch 404 | Player shows "No audio" |
| Audio fetch other error | Player shows "Failed" |

## Testing

- **Rust:** unit tests for buffer/discard (write, idempotent delete, id
  sanitization, traversal guard) and `fetch_meeting_audio` id validation.
- **`useBackend.test.ts`:** Ariso finalize buffers before upload, discards
  after confirm success, leaves the buffer on upload failure, and does not
  fail finalize when buffering itself fails.
- **`WaveformView.test.ts`:** failed state renders Retry/Dismiss; Retry
  re-runs finalize and reaches success on second attempt; Dismiss discards
  the pending buffer and closes; phase broadcasts follow
  uploading → failed → uploading → success.
- **Player:** generalized component fetches via the `load` prop; 404 maps to
  "No audio"; detail view wires local vs Ariso loaders correctly.
