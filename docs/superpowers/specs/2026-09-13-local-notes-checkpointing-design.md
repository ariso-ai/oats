# Incremental local transcript & notes checkpointing (issue #123)

## Problem

For the Local (offline) backend, transcription and AI notes both run exactly
once, and only after the user stops recording. `WaveformView.vue`'s
`handleStop()` → `runFinalize()` is the sole entry point into the whole
pipeline — `backend.value.finalizeRecording(stoppedBlob, meta)`
(`WaveformView.vue:597`) sends the *entire* recorded audio to Rust, which
transcribes it in one blocking pass (`run_transcribe`, `transcribe.rs`), writes
`transcript.md`, flips `RecordingStatus` to `Done`, then spawns notes generation
(`process_notes`, `transcribe.rs:247`) as a detached, best-effort task that runs
the on-device Gemma 3 1B model over the full transcript from scratch.

During the recording itself, nothing happens: the only timer running is a
cosmetic 1-second UI clock (`useRecorder.ts:376`). `RecordingMeta` already
carries a `notes_written: Option<String>` field whose doc comment says it was
recorded "for future use... an auto-regeneration guard" (`storage.rs:114-118`).

For a long meeting (an hour or more) this means:

- The user gets zero transcript or notes until they stop recording, however
  long the meeting runs.
- If oats or the machine crashes mid-recording, *everything* is lost: the audio
  lives only in the recorder window's memory (`mp3Chunks`, `useRecorder.ts`)
  until Stop, and the stub `meta.json` written at capture start (#355) is left
  stuck in `Recording` status with nothing behind it.
- Re-running today's notes pass every 5 minutes doesn't scale. The context
  window is *not* the binding constraint — the one-shot pass already handles
  an hour-long transcript on both platforms (macOS feeds the whole transcript to
  Gemma 3 1B's 32K-token context in one `ChatSession`; an hour of speech is
  roughly 12K tokens. Windows map-reduces it within a 4096-token llama.cpp
  context, `ariso-stt/windows/src/notes.rs`). The problem is cost: each
  from-scratch pass grows with the meeting, so repeating it every 5 minutes is
  quadratic in total, and it competes with the live call for the same CPU/GPU.
  The issue also asks, explicitly, that notes "remain consistent across multiple
  checkpoints instead of being regenerated from scratch each time."

## Goal

While a Local-backend recording is in progress, oats every 5 minutes of
recorded audio persists and transcribes the audio captured since the last
checkpoint and updates that recording's notes incrementally — without stopping
the recording, and without regenerating the notes from scratch each time.
Concretely:

- Opening the in-progress recording in the Library (it already appears there in
  `Recording` status, via the #355 stub) shows a transcript and AI notes that
  grow roughly every 5 minutes.
- Across checkpoints, notes are *merged*: checkpoint *N* updates what checkpoint
  *N-1* produced rather than starting over.
- Audio captured up to the last checkpoint survives a crash.
- **The end state is unchanged.** Checkpoint output is a *preview*. Stopping the
  recording runs today's finalize path, unmodified, over the full audio, so the
  final transcript (one diarization pass, consistent speaker ids) and final
  notes are exactly what oats produces today for the same audio.

## Non-goals

- **Cloud (Ariso) backend.** Out of scope — see [Cloud vs
  offline](#cloud-vs-offline).
- **Configurable checkpoint interval.** Fixed at 5 minutes of recorded audio,
  matching the house pattern of fixed windows elsewhere in the local pipeline
  (e.g. `APPEND_WINDOW_SECONDS = 300`, `storage.rs:8`).
- **Faster stop.** Stop latency stays what it is today (full-audio STT). This
  feature's value is visibility *during* the meeting and crash safety, not stop
  latency — see [Why a final full pass](#why-a-final-full-pass).
- **Cross-checkpoint speaker reconciliation in previews.** Each checkpoint's
  chunk is diarized independently and its speaker ids are offset to stay
  distinct (`next_speaker_offset` / `offset_segments`), so a two-person meeting
  previews as "Speaker 1…Speaker 24" after an hour. Previews accept this; the
  final full pass removes it.
- **Checkpointing sessions that append into an existing recording.** A session
  that docks to a prior `Done` recording — auto-append within the 5-minute
  window (`storage::resolve_local_recording_id`), an explicit "Continue this
  meeting" (`localAppendId`), or resuming a failed stop with a held blob — is not
  checkpointed in v1 and behaves exactly as today. See
  [Open questions](#open-questions).
- **A "just refreshed" UI affordance.** Content updates in place; no new visual
  language.
- **Resuming a recording after an oats relaunch.** Out of scope. A minimal
  startup reconcile (§7) *is* in scope, because checkpoints leave real data
  behind that must stay reachable.
- **Reworking the notes prompt's structure** beyond the new merge mode.

## Design

### Overview

```
record (WaveformView + useRecorder) ── every 300 s of recorded audio ──┐
      │                                                                 ▼
      │ (keeps encoding)                     local_checkpoint_recording(id, chunk, startByte)
      │                                        [per-recording lock]
      │                                        persist chunk → vault audio (append)
      │                                        transcribe chunk → offset → segments.json
      │                                        → re-render transcript.md
      │                                        → spawn preview notes (detached, merge mode)
      ▼
handleStop() ── full blob (unchanged) ──► local_finalize_recording (unchanged contract)
                                           [per-recording lock; aborts preview notes]
                                           fresh_recording_core over the FULL audio:
                                           authoritative transcript + notes, as today
```

Two tiers:

1. **Preview tier (new, during recording).** Incremental: each checkpoint
   transcribes only its chunk and merges it into the preview transcript and
   notes. Reuses the append machinery (`offset_segments`,
   `offset_participants`, `next_speaker_offset`, audio concatenation) inside a
   single still-open recording.
2. **Final tier (existing, at stop).** `local_finalize_recording` is kept, with
   its contract unchanged. The frontend still sends the full blob; the fresh path
   rewrites the recording in place (it already handles a pre-existing
   `meta.json` and reuses `meta.audio_file`, `transcribe.rs:390-419`) and
   replaces every preview artifact.

#### Why a final full pass

Stitching checkpoint transcripts together as the final result would regress
long meetings versus today: per-chunk speaker ids (above), words split at every
5-minute boundary, and any failed chunk permanently missing from the transcript.
Re-transcribing the full audio at stop costs exactly today's stop latency and
guarantees today's quality. It also keeps the proven finalize path (including
its retry / resume-after-failed-stop flows, which rely on finalize being an
idempotent in-place rewrite) completely untouched.

### 1. Frontend: the checkpoint loop (`src/views/WaveformView.vue`, `src/composables/useRecorder.ts`)

`useRecorder` stays backend-agnostic. It gains two small accessors over its
existing `mp3Chunks` buffer:

- `encodedByteLength(): number`, the total bytes emitted so far.
- `sliceFrom(startByte): { bytes: Uint8Array; endByte: number }`, the bytes
  from `startByte` up to the latest *whole* `mp3Chunks` entry. Slices are cut
  only at entry boundaries (whole encoder outputs), never at an arbitrary byte.

The loop itself lives in `WaveformView.vue`, which owns the resolved recording
id:

- **Eligibility** (all must hold): local backend; the resolved id is this
  session's own new id (`effectiveLocalRecordingId ===
  localRecordingIdFromStart(startAt)`, so no `localAppendId` and no auto-append
  target); no held `stoppedBlob` (not a resume of a failed stop); the stub write
  succeeded. For auto recordings the stub is deferred to `MIN_AUTO_DURATION_S`
  (15 s), well before the first checkpoint.
- **Trigger:** the existing `recorder.durationSeconds` watcher
  (`WaveformView.vue:397`), not `setTimeout`/`setInterval`. That clock is driven
  by the audio callback, so it keeps ticking when the hidden recorder webview's
  JS timers are throttled (`useRecorder.ts:271`). It also excludes pauses, so
  checkpoints fire every 300 s of *recorded audio*, not wall time.
- **Concurrency:** at most one checkpoint in flight. If the next 300 s mark
  passes while one is in flight, the next checkpoint fires as soon as it
  resolves, carrying the larger chunk.
- **Offset bookkeeping:** `lastCheckpointByte` advances only to the
  `ingestedBytes` value Rust returns. On an IPC or Rust error it stays put, so
  the next checkpoint resends from the same offset. Rust skips any prefix it
  already ingested (§2), which makes a resend safe.
- **Transport:** raw IPC body plus the `x-oats-meta` header, exactly like
  `local.finalizeRecording` (`tauri.ts:393`), with meta `{ id, createdAt,
  title, startByte }`. A JSON `number[]` is not acceptable: `local_finalize_recording`
  moved off it because serializing 48 MB took ~30 s (`transcribe.rs:784-787`).
  A 5-minute chunk (~4.8 MB) would block the webview main thread for seconds,
  and that thread runs the `ScriptProcessor` audio encoding. The recording
  itself would drop audio.
- **Stop:** `handleStop()` stops scheduling checkpoints and then proceeds
  *exactly as today* — full blob, `runFinalize()`. It does not await an
  in-flight checkpoint; Rust orders the two with the per-recording lock (§2).

### 2. Backend: `local_checkpoint_recording` (`src-tauri/src/transcribe.rs`)

New raw-body command, registered next to `local_finalize_recording`. The
existing `APPEND_LOCKS` table becomes a general `RECORDING_LOCKS`
(`get_recording_lock(id)`). It is taken by checkpoints, by the finalize fresh
and append paths, and by `rename_local_recording` (which becomes `async`: a
sync Tauri command runs on the main thread and must not block on a lock held
across STT).

`checkpoint_core(root, id, created_at, title, start_byte, chunk)`, under the
lock:

1. **Validate and load.** `validate_recording_id`. Read `meta.json`; if it is
   missing (stub write failed), create the stub exactly as
   `local_begin_recording` does (shared helper). If `status != Recording`, the
   recording was already finalized or is an append target. Return `Ok` with
   `stale: true` and write nothing. A late checkpoint is harmless.
2. **Idempotency.** `meta.preview.bytes_ingested` records how much of the
   stream is already persisted. A chunk ending at or before it is a duplicate:
   return `Ok` unchanged. A chunk that overlaps it has its already-ingested
   prefix skipped. (The boundary is an encoder-output boundary on both sides, so
   the remainder starts on an MP3 frame.) A chunk starting *after* it is a gap
   and is rejected; that can only be a frontend bug.
3. **Persist audio first.** On the first checkpoint, derive the vault
   attachment name exactly as `fresh_recording_core` does (`unique_basename` of
   `note_basename(created_at, title, id)`) and store it in `meta.audio_file`
   (the stub has `None`, `commands.rs:2354`). Append the chunk with
   `append_recording_core`'s read → extend → `write_atomic` pattern
   (`vault::write_audio` only replaces). Compute the chunk's duration from its
   MP3 frame headers: count MPEG-1 Layer III frames × 1152 samples / 44.1 kHz.
   This needs a small `storage::mp3_duration` header walk, not a new dependency,
   and is exact where the frontend's whole-second clock would drift by up to
   1 s per chunk. Update `preview.bytes_ingested`, `preview.duration_ms`, and
   `meta.duration_seconds`, then `write_meta`. Audio is now crash-safe whatever
   happens next.
4. **Transcribe the chunk** from a temp file (like `append-clip.mp3`) with
   `run_transcribe`. On success:
   - offset its segments by the chunk's start time (`preview.duration_ms`
     before step 3) and by `next_speaker_offset`;
   - append them to `segments.json`;
   - re-render `transcript.md` (`storage::render_markdown`, unchanged);
   - bump `preview.checkpoints`.

   On failure, log it and set `preview.last_error`. The audio stays persisted
   and the time offset has still advanced, so later chunks keep correct
   timestamps. The preview transcript simply has a gap for that window, and the
   final full pass covers it.
5. **Return** `CheckpointResult { ingestedBytes, transcriptUpdated, stale }`.
6. **After releasing the lock**, spawn the preview notes task (§3) unless one
   is already running for this recording. That backpressure is safe: the
   notes cursor didn't move, so the next run covers both deltas.

The final tier gains two small changes to `finalize_core_with_target`'s fresh
path when the target dir has a `Recording` meta with preview state:

- it takes the recording lock, so it waits for an in-flight checkpoint (a few
  seconds of STT at most);
- it aborts the preview notes task. `JoinHandle::abort` drops the future, and
  `run_notes` already sets `kill_on_drop(true)`, so the sidecar dies with it.

`fresh_recording_core` builds a new `RecordingMeta`, which drops `preview`
state automatically.

### 3. Preview notes (`src-tauri/src/transcribe.rs`)

A new `process_preview_notes(dir, models, id)`, detached, never awaited by the
checkpoint call:

- **Skip conditions:** LLM not ready (`llm_is_ready` false). The delta — the
  segments after `preview.notes_cursor` — carries no speech (the same
  `transcript_has_speech` test `process_notes` uses). A silence-only chunk is
  not a failure.
- **Inputs:** the delta segments, rendered with `render_markdown`'s speaker
  formatting into a temp `preview-delta.md`. The previous notes come from
  `vault::read_note(id)`.
  - No previous note (first checkpoint, or every earlier preview failed): run
    today's `notes --transcript transcript.md` over the transcript so far.
  - Otherwise, run `notes --previous-notes <tmp> --transcript preview-delta.md`
    (merge mode, §4).
- **Commit:** take the recording lock, re-read `meta.json`, and commit only if
  `status == Recording` and `preview.notes_cursor` is unchanged since the run
  started. Otherwise a newer run or the final pass owns the result, so discard
  it. Write the vault note using the *fresh* meta, whose title and
  `audio_file` may have changed through a rename since the run started. Then
  set `preview.notes_cursor` and `notes_written`.
- **Never** touches `meta.notes_error` (it records the error in
  `preview.last_error`), and **never** applies the generated title. Titles are
  generated once, from the full notes, by the final pass, as today.
  Otherwise `maybe_apply_generated_title` would lock the title to the first
  5 minutes (it clears `title_is_default` on first success) and rename the vault
  audio mid-recording.

**Fixing `process_notes`'s stale-meta write (existing race, widened here).**
`process_notes` writes back the whole `meta` snapshot it captured at spawn
(`transcribe.rs:305-307`). A rename that lands while notes run is reverted,
and `meta.audio_file` ends up pointing at a file the rename moved away. Today
the window is short and post-meeting. With live renames (#355) plus checkpoints
it becomes routine. `process_notes` therefore re-reads `meta.json` under the
recording lock before writing, and applies only the fields it owns: note,
`notes_error`, `notes_written`, `notes_in_progress`, and the generated title
only if `title_is_default` is still true.

### 4. Sidecar merge mode (both platforms)

> **NOT YET ADOPTED.** This section describes the originally designed merge
> mode. Testing against the real macOS MLX sidecar found that Gemma 3 1B
> cannot merge notes in place — see the resolved "Merge-prompt quality"
> finding under [Open questions](#open-questions). Task 9 (macOS merge mode)
> and Task 10 (host-side preview notes) are paused pending a decision on the
> documented fallback. The Windows sidecar's `--previous-notes` mode (Task 8)
> has landed but currently has no host caller. The prose below is kept as a
> record of what was attempted.

`notes` gains an optional `--previous-notes <path>`. When it is present,
`--transcript` is the delta, not the whole transcript. The output keeps the
`{ "title": ..., "notes": ... }` contract. Merge mode is preview-only, so it
emits `title: ""` and skips the title pass.

- **macOS** (`ariso-stt/macos/Sources/ariso-stt/main.swift`): a
  `generateMergedNotes(container:previousNotes:delta:)` next to
  `generateNotes()` (`main.swift:152`), using the same sections, rules and
  `GenerateParameters` (including the repetition penalty). The user turn is
  "Current notes: … New transcript, continuing directly after the part those
  notes cover: … Return the complete updated notes: add new key points,
  decisions and action items; keep existing content unless the new transcript
  corrects it; never duplicate an item." Budget: previous notes (≤2048 output
  tokens) plus 5 minutes of transcript (~1K tokens) is far inside the 32K
  context.
- **Windows** (`ariso-stt/windows/src/notes.rs`): the same prompt through the
  existing `PromptRunner`. The input budget is
  `input_char_budget(4096, 512)` = (4096 − 512 − 768) × 3 = 8,448 chars.
  Previous notes (≤512 tokens, ~2K chars) plus a 5-minute delta (~4.5K chars)
  fits. When a backpressured, multi-checkpoint delta doesn't fit, condense the
  delta first with the existing `chunk_summary_prompt`, then merge.

Rust's `run_notes` (`transcribe.rs:131`) takes an optional previous-notes path
and adds the flag.

### 5. Notes status stays truthful (`storage.rs`, `commands.rs`, `useLocalRecordingProgress.ts`)

Today "notes ready" is derived from whether a note exists
(`derive_notes_status`, `storage.rs:45`; `deriveStage`,
`useLocalRecordingProgress.ts:30`). Once a preview note exists, that breaks at
stop:

- the recording flips to `Done` with the preview note present, so the UI goes
  straight to terminal `ready`, stops polling, and never shows the final notes;
- `delete_local_recording`'s notes-pending guard (`commands.rs:2386-2400`)
  lets the user delete while the final notes pass runs, and `write_note` then
  re-creates the note in the vault — the exact orphan that guard exists to
  prevent.

Fix: `RecordingMeta.notes_in_progress: bool` (`#[serde(default)]`). It is set
true, before spawning `process_notes`, by the fresh path, the append path and
`retry_notes_core`. `process_notes` clears it on every exit that writes an
outcome; a superseded run leaves it to the run that superseded it.
`derive_notes_status` returns `Pending` whenever it is true, *before* the
has-note check. That covers `local_recording_status`, `list_recordings` and the
delete guard with no other changes. In the frontend, `deriveStage` checks
`notesStatus === 'pending'` before `hasNote`.

### 6. Frontend: live refresh of an open meeting (`useLocalRecordingProgress.ts`, `MeetingDetailView.vue`)

The poller already keeps ticking through `recording` (it's an in-flight stage).
Two changes:

- `RecordingStatusView` gains `previewCheckpoints: u32` and `notesWritten:
  Option<String>`. While `stage === 'recording'`, the composable bumps a
  `contentRevision` ref whenever either changes.
- `MeetingDetailView` watches `contentRevision` and re-reads **only** the
  transcript and AI note — the same per-artifact reads `load()` does for a local
  recording — preserving transcript scroll position. It deliberately does *not*
  call `load()` or emit `contentReady`. That chain refetches the list and
  reloads the whole pane (`MeetingDetailView.vue:1334`), which every 5 minutes
  would disrupt the My Note editor the user is likely typing in during the
  meeting.

**User edits during recording.** Previews overwrite the vault note every
checkpoint, and the final pass overwrites it at stop. So while `stage ===
'recording'` the AI Notes task checkboxes (`set_vault_task_done`) are disabled,
with the tooltip "Available after the recording finishes". Edits made in
Obsidian to a still-recording note are overwritten. This is consistent with
the vault invariant (`2026-07-03-local-backend-vault-design.md:173`): oats owns
the body until the recording is finalized, i.e. until `notes_written` is set by
the final pass and the append window has closed.

### 7. Startup reconcile (`main.rs` setup)

No recorder survives a relaunch, so at startup, for every local recording:

- `status == Recording` → `Failed`, with `error = "oats quit while recording —
  audio up to the last checkpoint was kept"`. `Failed` unlocks the existing
  Retry (`retry_transcription_core` re-runs the full pipeline on the vault
  audio, using `meta.duration_seconds` kept current by §2) and Delete. Without
  this, the recording is stranded: `delete_local_recording` refuses
  `Recording`, the detail pane offers no Retry for it, and the poller spins
  forever. A stub with no audio also becomes `Failed` (deletable).
- `status == Transcribing` → `Failed` (audio retained, Retry available). This
  is the same sweep, and it also fixes today's zombie left by a crash during
  finalize.
- `notes_in_progress == true` → cleared. No notes task survives a restart. A
  preview note left in place shows as `ready`, and Regenerate is available.

## Cloud vs offline

**Local only.** The Ariso cloud backend runs transcription and notes
server-side; nothing in cloud mode calls the local sidecar or touches
`segments.json`/vault files, so there's no shared code path to extend.
Server-side mid-meeting notes would be a backend-team feature request. The
checkpoint loop's eligibility check (§1) keeps cloud sessions on today's path.

Everything here stays on-device: chunk transcription reuses the on-device STT
sidecar, and merge mode reuses the already-downloaded Gemma 3 1B. No new
network call is introduced, so the offline-mode privacy guarantee
(`oats-security` §10) holds by construction.

## Error handling

| Situation | Behavior |
|---|---|
| Chunk transcription fails (sidecar crash, corrupt chunk) | Audio is already persisted and the time offset has advanced; the preview transcript has a gap for that window; `preview.last_error` is logged. The final full pass covers the gap, so nothing is lost from the end state. |
| Checkpoint IPC/Rust error | The frontend keeps `lastCheckpointByte`; the next checkpoint resends from it; Rust skips any already-ingested prefix (§2). |
| Preview notes fail | The previous preview note is untouched; `preview.last_error` is set; `notes_error` is not touched, so no failure UI. The cursor didn't move, so the next checkpoint retries with the combined delta. |
| Preview notes still running at the next checkpoint | That round's notes are skipped (backpressure); the next run covers both deltas. |
| LLM not installed | Transcript checkpoints proceed; preview notes are skipped; the final pass behaves as today. |
| Stop while a checkpoint is in flight | Finalize waits on the recording lock (seconds), aborts the preview notes task, then runs as today. |
| Checkpoint arrives after finalize began | `status != Recording` → `Ok { stale: true }`, nothing written. |
| Stop while only preview notes are running | Finalize aborts the task, which kills the sidecar via `kill_on_drop`. |
| Mid-recording rename | Serialized against checkpoints by the lock; notes commits use fresh meta; previews never auto-title. |
| App/machine crash mid-recording | Audio up to the last checkpoint is in the vault. Startup reconcile marks the recording `Failed` → Retry transcription or Delete. (Today: everything is lost.) |
| Crash during the final notes pass | Startup clears `notes_in_progress`; the last preview note shows as ready; Regenerate is available. |
| Recording shorter than 5 minutes | No checkpoint fires — identical to today. |
| Append / "Continue this meeting" / resume-a-failed-stop session | Not eligible (§1) — identical to today. |

## Testing

### Rust (`cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1`)

- `checkpoint_core`:
  - the first checkpoint creates the vault attachment and sets `audio_file`;
  - it creates the stub if `meta.json` is missing;
  - segments are offset by the frame-derived duration, and speaker ids by
    `next_speaker_offset`;
  - `transcript.md` is re-rendered;
  - `status` stays `Recording`.
- Idempotency: a duplicate `start_byte` is a no-op; an overlapping chunk
  appends only its new suffix; a gap is rejected.
- A failed chunk transcription still persists audio and advances
  `duration_ms`; `segments.json` is unchanged; `status` stays `Recording`.
- A checkpoint against a `Done` or `Transcribing` recording returns
  `stale: true` and writes nothing.
- `storage::mp3_duration` on lamejs-encoded fixtures (mono and stereo, 128 kbps).
- Preview notes:
  - no previous note → `--previous-notes` omitted; a previous note → passed
    along with the delta;
  - commit discarded when the cursor moved or `status` changed;
  - `notes_error` and the title are never touched;
  - skipped when the LLM isn't ready or the delta has no speech.
- Finalize after checkpoints produces `segments.json`/`transcript.md`
  byte-identical to finalizing the same full audio with no checkpoints, drops
  `preview`, and aborts a running preview notes task.
- `notes_in_progress`: `derive_notes_status` returns `Pending` with a note
  present; `delete_local_recording` refuses; `process_notes` clears it.
- `process_notes` commit after a concurrent rename keeps the renamed
  title/`audio_file`.
- Startup reconcile: `Recording`/`Transcribing` → `Failed`;
  `notes_in_progress` cleared.

### Sidecar

- Windows (`ariso-stt/windows`, existing `PromptRunner` test seam): merge-mode
  prompt construction; an over-budget delta is condensed first.
- macOS (manual, no test target): run `notes --previous-notes <prior>.md
  --transcript <delta>.md` on a hand-built two-part transcript. Confirm the
  output merges without duplicating prior items or degenerating (echoing the
  transcript, looping).

### Frontend (Vitest)

- `WaveformView`:
  - a checkpoint fires when `durationSeconds` crosses 300, not on wall time
    (pauses don't count);
  - only the bytes since `lastCheckpointByte` are sent, over the raw-body
    transport;
  - at most one is in flight;
  - the offset advances only to the returned `ingestedBytes`;
  - no checkpoint for cloud, append targets, `localAppendId`, or a held
    `stoppedBlob`;
  - `handleStop` still sends the full blob via `finalizeRecording`.
- `useRecorder`: `sliceFrom` cuts at `mp3Chunks` entry boundaries.
- `useLocalRecordingProgress`: `contentRevision` bumps on a
  `previewCheckpoints`/`notesWritten` change while recording, and not
  otherwise; `deriveStage` returns `notes-pending` for `notesStatus:
  'pending'` even with `hasNote`.
- `MeetingDetailView`: a revision bump re-reads the transcript/note without
  calling `load()` or emitting `contentReady`; task checkboxes are disabled
  while recording.

### Manual

- Record a ~15-minute local meeting (three checkpoints). Open it while it
  records and confirm the transcript and AI notes grow every ~5 minutes without
  duplicated items. Stop, and confirm the final transcript has consistent
  speaker labels and the final notes/title land, with the status chip showing
  "Generating AI Notes" until they do.
- Repeat with the notes LLM not installed.
- Repeat on a base 8 GB Mac during a real video call and listen to the
  recording for dropouts around each checkpoint (STT + Gemma model loads
  mid-call).
- `kill -9` oats mid-recording after one checkpoint, relaunch, and confirm the
  recording shows as failed with its audio, and that Retry produces a full
  transcript.

## Open questions

- **Merge-prompt quality — RESOLVED, failed.** Tested against the real macOS
  MLX sidecar (Gemma 3 1B), not just reasoned about: every call whose input
  contained *both* the current notes and the new transcript returned the
  current notes byte-identically. This held across 9 prompt variants,
  spanning system-vs-user-message framing, both orderings of notes and
  transcript, chain-of-thought, two-turn decomposition, fact-routing, and a
  diff-only format. Every delta-only call (no previous notes in the input)
  produced good notes. So merge-in-place is not viable with this model as
  currently prompted, on either platform (Windows uses the same Gemma 3 1B).
  §4 ("Sidecar merge mode") is therefore not yet adopted; Task 9 (macOS merge
  mode) and Task 10 (host-side preview notes) are paused. The fallback stays
  incremental without a merge prompt: summarize each checkpoint's delta once
  (cache the summaries in the recording dir), and re-run only the reduce step
  over the cached summaries. That reuses the Windows sidecar's existing
  `chunk_summary_prompt` / `summary_reduction_prompt` pipeline. This is the
  path forward, pending a decision on when to pick it up. See
  `.superpowers/sdd/2026-09-15-local-notes-checkpointing/task-9-prompt-experiment.md`
  for the experiment log (gitignored, not tracked in this repo).
- **Final notes replace the preview.** The final pass regenerates notes from
  the full, consistently diarized transcript, so the notes a user watched evolve
  can read differently after stop. The alternative — one last merge pass on top
  of the preview — would keep preview speaker labels ("Speaker 13") that don't
  exist in the final transcript. Leaning toward the full regeneration as
  authoritative; it is also exactly today's behavior.
- **Append sessions.** Excluded in v1 (§1). Supporting them would mean
  checkpointing into a `Done` target, i.e. temporarily flipping it back to
  `Recording` and teaching `append_recording_core` to fold in the preview state.
  Worth doing only if back-to-back sessions turn out to be common for long
  meetings.
- **Load during calls.** Each checkpoint loads the STT (Parakeet + diarizer)
  and notes (Gemma) models while the user is on a call. If the 8 GB manual test
  shows dropouts or heavy fan/battery use, consider skipping preview notes on
  battery or lengthening the interval adaptively.
- **UI treatment for a just-refreshed checkpoint.** Silent in-place update (as
  designed) vs. an "updated Xm ago" indicator — a product decision.
