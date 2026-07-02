# Local (offline) multi-recording support in oats — design

**Date:** 2026-07-02
**Status:** Approved (brainstorming) → ready for implementation plan
**Scope:** PR 2 of 2. Cloud (Ariso) multi-recording shipped in PR 1
(`docs/superpowers/specs/2026-06-30-cloud-multi-recording-oats-design.md`,
merged as #184). This spec covers the **local (offline, on-device) backend
only**.

## 1. Problem

Recording the same meeting more than once — record, stop (break / accidental
stop), record again shortly after — should extend the existing local recording
rather than create a second, disconnected one. The local backend has no calendar
and no concept of a meeting spanning sessions today: every finalize creates a new
recording directory keyed by `sanitize_iso_to_id(created_at)`, with its own
`recording.mp3`, `transcript.md`, and `ari-note.md`.

Unlike the cloud backend (which keeps each upload as a separate first-class
*clip* with a stacked-player UI), the local backend **merges** a resumed session
into the existing recording: one audio file, one continuous transcript, notes
regenerated. No clip UI.

## 2. Current state (local pipeline)

- **Finalize:** `local_finalize_recording(audio, title, created_at, duration_seconds)`
  → `finalize_core` (`src-tauri/src/transcribe.rs`): create dir, write
  `recording.mp3`, write `meta.json` (`status: Transcribing`), run STT, render
  `transcript.md`, mark `Done`, spawn detached notes.
- **STT:** `ariso-stt --audio <path> --models <path> --format json` →
  `TranscriptResult { language, participants: [{id,label}], segments: [{speaker,text,start,end}] }`
  (on-device diarization). `render_markdown` produces `transcript.md` (YAML
  frontmatter + speaker blocks).
- **Notes:** detached `ariso-stt notes --transcript transcript.md --models <path>`
  → `ari-note.md` (best-effort, 300 s timeout).
- **On disk** (`~/.ariso/recordings/<id>/`): `meta.json`, `recording.mp3`,
  `transcript.md`, `ari-note.md`, `user-note.md`, `user-note-title.txt`.
- **No association / append / time-window** exists. The `new Blob([prev, new])`
  concatenation in `WaveformView` is Ariso-only (failed-upload retry).

## 3. Goals / non-goals

**Goals**
- After a local recording finishes, a new local recording that starts within
  **5 minutes** of it appends to it: extend the audio, stitch the transcript,
  regenerate notes — one merged recording, no new directory.
- Keep transcription incremental: transcribe only the new clip.
- Keep the merged recording crash-safe and the append decision testable.

**Non-goals**
- Speaker/diarization reconciliation across clips (ids are kept distinct, not
  merged).
- Any clip UI (stacked players / per-clip transcript / per-clip delete) — that
  is the cloud path only.
- A configurable window (fixed at 5 minutes) or a manual "merge/split
  recordings" UI.
- Cloud backend (shipped in PR 1).

## 4. Design

### 4.1 On-disk model (`src-tauri/src/storage.rs`)

Add one sidecar per recording: **`segments.json`** — the structured transcript,
`{ language, participants: [{id,label}], segments: [{speaker,text,start,end}] }`.
`transcript.md` becomes a pure render of `segments.json` + `meta.json`. `meta.json`
keeps its current shape; `duration_seconds` and `participants` accumulate across
clips. No new `RecordingMeta` fields; no clip concept on disk (one merged
recording).

New/changed storage functions:
- `write_segments(dir, &SegmentsFile)` / `read_segments(dir) -> Option<SegmentsFile>`
  (atomic write).
- `render_markdown` already renders from segments + meta; the finalize path now
  always persists `segments.json` alongside `transcript.md`.

### 4.2 Append decision — `most_recent_appendable(root, new_start) -> Option<RecordingMeta>`

List recordings newest-first (existing `list_recordings`) and consider only the
newest. It is the append target iff:
- `status == Done`, **and**
- `0 ≤ (new_start − prev_end) ≤ 5 min`, where `prev_end = created_at +
  duration_seconds` (as timestamps) and `new_start` is the incoming clip's
  `created_at`.

Chained appends work because appends do not create new directories: the target
stays newest and its `prev_end` grows with each clip. Any other case (empty
list, newest is `Transcribing`/`Failed`, gap > 5 min, negative delta) → normal
new recording.

### 4.3 Finalize flow (`src-tauri/src/transcribe.rs`, refactor `finalize_core`)

- **New recording** (no target): today's behavior **plus** persist
  `segments.json`, render `transcript.md` from it.
- **Append** (target `T`):
  1. Transcribe only the new clip's audio (`run_transcribe`).
  2. Offset the new segments' `start`/`end` by `T.duration_seconds`; offset the
     new speaker ids by `max_existing_speaker_id + 1`; add the offset speakers as
     new participants (kept distinct — never falsely merged).
  3. Append the offset segments to `T/segments.json`.
  4. Append the clip's mp3 bytes to `T/recording.mp3`.
  5. Update `T/meta.json`: `duration_seconds += new_duration`, merged
     `participants`, `status = Done` (language unchanged).
  6. Re-render `T/transcript.md` from the updated `segments.json` + `meta.json`.
  7. Regenerate notes (detached).
  8. Return `FinalizeResult { id: T.id, … }`.

All writes use the existing `write_atomic` (segments, transcript, and the
extended mp3) so a crash mid-append cannot corrupt the recording.

### 4.4 Frontend (`src/`) — minimal

Rust owns the decision, so the frontend is nearly untouched:
- `LocalBackend.finalizeRecording` already returns `{ id, … }`; the id may now be
  an existing recording's. `WaveformView` shows success and closes; the Library
  re-lists and shows the extended recording.
- `getMeetingAudio` / `getMeetingTranscript` read the now-longer
  `recording.mp3` / `transcript.md` unchanged.
- No clip UI. One transient cosmetic: during the 2nd session the "recording"
  red-dot uses that session's `startAt`-derived id, which won't match the target
  recording id until finalize re-lists the Library. Acceptable.

## 5. Data flow (append case)

1. Session 2 stops → `WaveformView` → `local.finalizeRecording(newBytes, title, createdAt2, dur2)`.
2. Rust: newest recording `T` is `Done` and ended ≤ 5 min ago → append path.
3. Transcribe `newBytes` → offset segments/speakers → append to `segments.json`
   → append bytes to `recording.mp3` → update `meta.json` → re-render
   `transcript.md` → regenerate notes.
4. Return `T.id` → frontend success → Library shows `T` extended.

## 6. Error handling / edge cases

- **New-clip STT fails during an append:** do not touch `T`. Fall back to writing
  the clip as its own separate `Failed` recording (audio retained, retryable) —
  identical to today's failure behavior.
- **Newest is `Transcribing`** (prior clip still processing): not appendable →
  new recording (rare race; safe and simple).
- **Notes regen race:** a superseded detached notes job could finish after the
  append's job. Guard with a per-recording notes token so a stale job's output is
  discarded (last-writer-by-token wins).
- **Channel-count mismatch across clips** (mono then mic+system stereo): mp3
  concatenation can glitch playback. Rare within 5 min; documented limitation.
- **Speaker ids not reconciled across clips:** offset to stay distinct;
  documented limitation (same as cloud).
- **Atomicity:** `segments.json`, `transcript.md`, and the extended
  `recording.mp3` are each written via `write_atomic`.

## 7. Testing

- **Rust unit tests** (sidecar mocked via `ARISO_STT_BIN` → a fake script,
  matching the existing `transcribe.rs` test pattern; run with the documented
  `DYLD_LIBRARY_PATH` + `--test-threads=1` workaround):
  - `most_recent_appendable`: within/outside the 5-min window; `Done` vs
    `Transcribing`/`Failed`; empty list; negative delta.
  - Segment offsetting: timestamps shifted by prior duration; speaker ids offset;
    participants merged.
  - Re-render: `transcript.md` from a multi-clip `segments.json` — monotonic
    timestamps, frontmatter total duration + merged participants.
  - Append flow end-to-end: existing recording + new clip → `segments.json`
    grows, `meta.duration_seconds` sums, `transcript.md` re-rendered, returned id
    == target.
  - STT-failure fallback → separate `Failed` recording, target untouched.
- **Frontend:** minimal — a `useBackend` test that `finalizeRecording` surfaces
  the returned (possibly existing) id.

## 8. Out of scope

Speaker reconciliation across clips; cloud backend (PR 1); manual merge/split UI;
a configurable append window.
