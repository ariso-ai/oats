# Incremental local transcript & notes checkpointing (issue #123)

## Problem

For the Local (offline) backend, transcription and AI notes both run exactly
once, and only after the user stops recording. `WaveformView.vue`'s
`stopRecording()` is the sole entry point into the whole pipeline —
`backend.value.finalizeRecording(stoppedBlob, meta)` (`WaveformView.vue:597`)
sends the *entire* recorded audio to Rust, which transcribes it in one
blocking pass (`run_transcribe`, `transcribe.rs`), writes `transcript.md`,
flips `RecordingStatus` to `Done`, then spawns notes generation
(`process_notes`, `transcribe.rs:247`) as a detached, best-effort task that
runs the on-device Gemma 3 1B model over the full transcript from scratch.

During the recording itself, nothing happens: the only timer running is a
cosmetic 1-second UI clock (`useRecorder.ts:376`) that updates the on-screen
duration. `RecordingMeta` already carries a `notes_written: Option<String>`
field whose doc comment says it was recorded "for future use... an
auto-regeneration guard" (`storage.rs:114-118`) — this feature is the thing
that comment was anticipating.

For a long meeting (an hour or more) this means:

- The user gets zero transcript or notes until they stop recording, however
  long the meeting runs.
- If oats or the machine crashes mid-recording, the entire transcript and
  notes are lost — there is no partial output on disk (only the raw
  in-progress audio survives, and only in the frontend's memory until
  `finalizeRecording` is called).
- A naive "just re-run notes generation every 5 minutes on the whole
  transcript so far" doesn't scale: Gemma 3 1B is a small on-device model
  with a limited context window, and it already needs a repetition penalty to
  avoid degenerating on a single normal-length transcript
  (`2026-06-03-local-meeting-notes-design.md`). Re-summarizing a growing
  transcript from scratch every 5 minutes means the prompt gets larger and
  slower every cycle, and eventually exceeds what the model can reliably
  process — exactly the "long recording" case this issue is asking to
  support.

## Goal

While a Local-backend recording is in progress, oats periodically (every 5
minutes of recording) transcribes the audio captured since the last
checkpoint and updates that recording's notes incrementally — without
stopping the recording, and without regenerating the whole transcript's
notes from scratch each time. Concretely:

- Opening the in-progress recording in the Library (it already appears there
  in `Recording` status) shows a transcript and AI notes that grow roughly
  every 5 minutes, not just after the meeting ends.
- Notes stay internally consistent across checkpoints: checkpoint *N* updates
  what checkpoint *N-1* produced rather than starting over, so notes quality
  and generation latency don't degrade as the meeting gets longer.
- Stopping the recording still produces a complete, correct final transcript
  and notes — checkpointing is a latency improvement, not a change to the
  end state.

## Non-goals

- **Cloud (Ariso) backend.** Out of scope — see [Cloud vs
  offline](#cloud-vs-offline).
- **Configurable checkpoint interval.** Fixed at 5 minutes, matching the
  house pattern of fixed windows elsewhere in the local pipeline (e.g.
  `APPEND_WINDOW_SECONDS = 300` for cross-session append, `storage.rs:8`).
- **Cross-checkpoint speaker reconciliation.** Each checkpoint's clip is
  transcribed independently and its speaker ids are offset to stay distinct,
  exactly like today's append-window merge (`offset_segments` /
  `offset_participants`) — speaker identity is not reconciled across
  checkpoints, same accepted limitation as multi-recording append.
- **A "just refreshed" UI affordance.** Whether/how to visually indicate a
  checkpoint just landed (toast, timestamp, subtle pulse) is left to review —
  see [Open questions](#open-questions). This spec guarantees the content
  updates; it doesn't design a new visual language for it.
- **Resuming checkpoint state after an oats relaunch mid-recording.** An
  in-progress recording surviving a full app restart is an existing gap in
  the recording feature generally, not something this spec fixes.
- **Reworking the notes prompt's structure** (sections, tone, length) beyond
  what's needed to make it merge-aware. Untrusted discussion on the issue
  argued for more structured, decision/risk/action-focused notes generally —
  legitimate feedback, but a separate concern from checkpointing and not
  acted on here.

## Design

### Overview

```
record (frontend, useRecorder.ts) ──── 5-min chunk boundary ────┐
        │                                                        │
        │ (continues encoding)                    checkpoint_local_recording(id, chunk, is_final=false)
        │                                                        │
        │                                                        ▼
        │                                          transcribe chunk → offset segments/speakers
        │                                          → append to segments.json → re-render transcript.md
        │                                          → incremental notes refresh (best-effort)
        │                                                        │
        ▼                                                        ▼
stopRecording() ── final tail chunk ──► checkpoint_local_recording(id, chunk, is_final=true)
                                          (same pipeline, then status → Done, final notes pass)
```

The core idea is to reuse the machinery already built for cross-session
append (`docs/superpowers/specs/2026-07-02-local-multi-recording-oats-design.md`):
transcribing a new clip, offsetting its timestamps/speaker ids by what's
already accumulated, and appending into `segments.json`. That logic already
exists for "a new recording session starts within 5 minutes of the last
one." This feature triggers the same merge automatically, on a 5-minute
timer, *within* a single still-open recording session, instead of waiting
for the user to stop and restart.

### 1. Frontend: chunked capture (`src/composables/useRecorder.ts`, `src/views/WaveformView.vue`)

Today `useRecorder.ts` accumulates lamejs-encoded MP3 output into an in-memory
buffer for the whole session and only builds a Blob from it at `stopRecording`
time. MP3 frames are self-delimiting, so a prefix of that buffer is itself a
valid, decodable MP3 stream — this is the same property the append feature
already relies on when concatenating separate clips' raw bytes
(`local-multi-recording` §4.3 step 4).

Changes, gated to `activeBackend.value.id === 'local'` only:

- Track a `lastCheckpointByteOffset` alongside the existing MP3 byte
  accumulator.
- A new sequential (not `setInterval`) checkpoint loop: after each 5-minute
  mark, slice the bytes accumulated since `lastCheckpointByteOffset`, invoke
  the new `local.checkpointRecording(id, chunkBytes, isFinal: false)` command,
  and only schedule the next 5-minute wait once that call resolves. This
  bounds concurrency to one in-flight checkpoint per recording by
  construction — no Rust-side locking needed for the common case.
- On `stopRecording()`: cancel the pending checkpoint timer, slice the final
  remaining bytes since the last checkpoint, and call
  `local.checkpointRecording(id, chunkBytes, isFinal: true)` instead of
  today's `finalizeRecording(stoppedBlob, meta)` call. If no checkpoint ever
  fired (a recording shorter than 5 minutes), the "chunk since last
  checkpoint" is the entire recording, so this is exactly today's fresh-path
  behavior — same bytes, new entry point.
- `finalizeRecording`'s existing frontend/backend contract is retired in
  favor of `checkpointRecording(..., isFinal: true)` for the local backend;
  the cloud backend's `finalizeRecording` is untouched (see [Cloud vs
  offline](#cloud-vs-offline)).

### 2. Backend: a shared "ingest a clip" pipeline (`src-tauri/src/transcribe.rs`)

New command `checkpoint_local_recording(id: String, chunk: Vec<u8>, is_final: bool)`,
registered alongside `local_begin_recording` / `local_recording_status`. It
factors the parts of today's `fresh_recording_core` /
`append_recording_core` that don't depend on "recording has fully stopped"
into a shared `ingest_chunk_core(id, chunk_bytes) -> Result<(), String>`:

1. Load `meta.json` for `id` (must exist — created at `local_begin_recording`
   time — and have `status == Recording`).
2. `run_transcribe` the chunk (same STT call as today's per-clip transcribe).
3. `offset_segments` / `offset_participants` by what's already in
   `segments.json` (byte-for-byte the same offsetting logic
   `append_recording_core` already uses, `transcribe.rs:563`).
4. Append the offset segments into `segments.json`; append the raw chunk
   bytes into the vault audio attachment (`vault::write_audio`, extending
   rather than replacing); update `meta.duration_seconds`.
5. Re-render `transcript.md` from the updated `segments.json` + `meta.json`
   (`storage::render_markdown`, unchanged).
6. Run the incremental notes refresh (§3 below), best-effort — a failure here
   never fails the checkpoint or touches the transcript that was just
   written.

`checkpoint_local_recording` calls `ingest_chunk_core`, then:
- If `is_final == false`: leaves `status == Recording`, returns.
- If `is_final == true`: sets `status = Done`, runs one last notes refresh
  pass, and returns the same `FinalizeResult { id, ... }` shape
  `local_finalize_recording` returns today, so the frontend's
  success/close/re-list flow (`WaveformView.vue`) is unchanged.

This makes `local_finalize_recording` effectively dead code for the local
backend (superseded by `checkpoint_local_recording(..., is_final: true)`); it
is removed rather than kept as an unused parallel path, per this repo's
convention of not keeping compatibility shims around.

**In-flight guard:** because the frontend awaits each checkpoint before
scheduling the next, and `stopRecording` cancels the pending timer before
sending the final chunk, only one `checkpoint_local_recording` call is ever
in flight per recording in the expected flow. As defense in depth (a slow
final call racing a very-late-firing prior timer), `ingest_chunk_core`
rejects with a clear error if `status` isn't `Recording` when it starts,
matching the existing pattern of failing loudly on an unexpected meta state
rather than silently corrupting `segments.json`.

### 3. Storage additions (`src-tauri/src/storage.rs`)

- `RecordingMeta` gains `checkpoint_count: u32` (`#[serde(default)]`, so
  existing recordings deserialize as `0` and are never mistaken for having
  been checkpointed) — used only to decide whether a checkpoint's notes call
  is a first-generation or a merge (§3 below); not otherwise branched on.
- No change to `notes_written` — it continues to record the most recent
  successful note write, checkpoint or final, matching its existing
  semantics.

### 4. Sidecar: an incremental notes mode (`src-tauri/ariso-stt`, `main.swift`)

Today's `notes` subcommand reads the whole transcript and always generates
from scratch (`generateNotes()`, `main.swift:152`). It gains an optional
`--previous-notes <path>` argument:

- **No `--previous-notes` (first checkpoint of a recording, or a legacy
  transcript with no prior notes):** unchanged — the existing from-scratch
  prompt over the transcript-so-far.
- **`--previous-notes` present:** a different prompt — "here are the meeting
  notes so far: `<previous notes body>`. Here is a new segment of the
  transcript, continuing directly after the last one: `<delta transcript
  since the last checkpoint>`. Update the notes to incorporate anything new —
  add new discussion points, decisions, and action items; don't repeat what's
  already covered; keep the existing structure." Runs on the already-loaded
  `ModelContainer`, same as `generateTitle`'s second-turn reuse pattern
  (`local-notes-title-generation` design) — no second model load.
- Output stays the existing `{ "title": ..., "notes": "..." }` JSON contract
  (`NotesResult`, from the title-generation feature). Title generation only
  runs while `meta.title_is_default` is still true, unchanged from today.

Rust (`run_notes`, `transcribe.rs:131`) passes `--previous-notes` whenever
`checkpoint_count > 0` **and** a previous note body is available to read
(vault `read_note` for the recording's `oats_id`). If the LLM isn't installed
(`llm_is_ready` false), the notes step of a checkpoint is skipped entirely —
the transcript checkpoint still succeeds independently, matching today's
existing decoupling of STT readiness (required to record) from LLM readiness
(opt-in, notes-only).

This is a materially harder prompting task for a 1B model than one-shot
summarization, which already needs tuning (repetition penalty) to avoid
degenerating. Quality here is a real risk, not a formality — see [Open
questions](#open-questions).

### 5. Vault write timing (`src-tauri/src/vault.rs`)

The vault design's existing invariant already covers this case without
changes: *"oats writes the note body only while the recording is still
accreting; once finalized... it never rewrites the body"*
(`2026-07-03-local-backend-vault-design.md`). A recording in `Recording`
status is, by definition, still accreting — repeatedly overwriting its vault
note every 5 minutes is exactly the behavior that invariant already permits.
Once `is_final` flips `status` to `Done`, the existing "never rewrite after
done" rule takes back over unchanged (a user can still trigger `retry_notes`
manually afterward, as today).

### 6. Frontend: surfacing progressive updates (`useLocalRecordingProgress.ts`, `MeetingDetailView.vue`)

`useLocalRecordingProgress.ts` polls `local.recordingStatus(id)` every 2
seconds but only to react to terminal-state *transitions*
(`Recording→Transcribing→Done`, `NotesStatus` reaching `Ready`/`Failed`). A
recording sits in `Recording` status for the entire meeting today, so
nothing currently re-fetches transcript/note content while that status holds
steady — there was never a reason to before this feature.

Extend the poll: include `checkpoint_count` and `notes_written` in the
`recordingStatus` response, and have the composable emit a `content-updated`
signal when either value changes while `status == Recording` (mirroring the
existing `content-ready` emit pattern from
`2026-08-13-post-meeting-processing-status-design.md`). `MeetingDetailView`
re-reads transcript/notes on that signal the same way it does on reaching a
terminal stage, so an open in-progress meeting's Transcript/AI Notes tabs
update in place roughly every 5 minutes.

## Cloud vs offline

**Local only.** The Ariso cloud backend has an entirely separate
architecture — transcription and notes run server-side, driven by whatever
pipeline Ariso's backend uses for meeting bots, not by this app's
`finalizeRecording`/`ariso-stt` flow. Nothing in cloud mode calls the local
sidecar or touches `segments.json`/vault files, so there's no shared code
path to extend. If Ariso's server-side pipeline should also refresh notes
mid-meeting, that's a backend-team feature request, not a desktop-app change
— out of scope here entirely, consistent with how the issue itself frames
this as "the Local backend."

Everything in this design keeps running fully on-device: chunk transcription
reuses the existing on-device STT sidecar call, and incremental notes reuse
the already-downloaded on-device Gemma 3 1B model. No new network call is
introduced anywhere in this flow, so the offline-mode privacy guarantee
(`oats-security` §10) holds by construction.

## Error handling

| Situation | Behavior |
|---|---|
| Chunk transcription fails (sidecar crash, corrupt chunk, silence-only chunk) | That checkpoint's transcript merge is skipped; `segments.json`/`transcript.md` stay at their last good state; logged, not surfaced as a recording failure. The next checkpoint (or the final chunk) picks up from where the last successful merge left off — the un-merged audio for the failed chunk is not retried separately, so that slice of speech is missing from the final transcript (a real but rare, bounded loss, flagged as an accepted trade-off rather than solved with a retry queue). |
| Notes refresh fails at a checkpoint (model OOM, sidecar timeout) | Previous notes are left untouched — a checkpoint never overwrites good notes with an error. `meta.notes_error` is set transiently; the next checkpoint tries again from the (still-current) previous notes. |
| LLM not installed (`llm_is_ready == false`) | Transcript checkpoints proceed normally; notes checkpoints are skipped entirely (no error state) — identical to today's existing decoupling of STT vs LLM readiness. |
| `stopRecording` fires while a checkpoint is still in flight | Not expected in the normal flow — the frontend awaits each checkpoint before scheduling the next, and cancels the pending timer on stop. If it happens anyway (e.g. a very slow checkpoint overlapping a fast user stop), the final `checkpoint_local_recording(..., is_final: true)` call is queued behind it on the frontend (both go through the same sequential await chain), so they cannot race Rust-side. |
| App/machine crashes mid-checkpoint | `segments.json`/`transcript.md`/vault audio writes are atomic (`write_atomic`, unchanged), so a crash mid-write cannot corrupt them — worst case is losing exactly the in-flight chunk's transcript, recoverable by re-recording that portion. This is strictly better than today, where a crash before `finalizeRecording` loses the entire meeting's transcript and notes. |
| Recording is shorter than 5 minutes | No checkpoint ever fires; `stopRecording` sends the whole recording as one "final chunk," which is exactly today's fresh-recording behavior — no regression for short meetings. |
| Cross-session append (existing 5-min-gap merge) onto a recording that was itself checkpointed | Unaffected — append operates on the finalized (`Done`) target recording exactly as it does today; checkpointing only changes what happens *while* a single recording is still `Recording`. |

## Testing

### Rust (automated, `DYLD_LIBRARY_PATH=... cargo test --test-threads=1`)

- `ingest_chunk_core`: a chunk is transcribed, offset, and merged into
  `segments.json`; `transcript.md` is re-rendered; `status` stays `Recording`
  when `is_final == false`.
- `checkpoint_local_recording(..., is_final: true)`: same merge, then
  `status → Done`, a final notes pass runs, and the returned shape matches
  today's `FinalizeResult`.
- A recording with `checkpoint_count == 0` calling notes generation omits
  `--previous-notes`; `checkpoint_count > 0` with an existing vault note
  passes it.
- `llm_is_ready == false`: transcript checkpoint still succeeds; no notes
  call is attempted.
- A failed chunk transcription leaves `segments.json`/`transcript.md`
  unchanged and does not flip `status` away from `Recording`.
- A failed notes refresh leaves the existing vault note body untouched and
  sets `meta.notes_error` without affecting `status`.
- Regression: a short (never-checkpointed) recording finalized via
  `checkpoint_local_recording(..., is_final: true)` produces byte-identical
  output to today's `fresh_recording_core` path for the same input audio.

### Sidecar (manual — no test target, per repo convention)

Run `notes --previous-notes <prior>.md --transcript <delta>.md` against a
hand-built two-part transcript and confirm the output notes incorporate the
new segment without duplicating the prior content or degenerating (echoing
transcript text, looping) — the same quality bar the original notes feature
already had to hit, now under a harder prompt.

### Frontend (Vitest)

- `useRecorder.test.ts`: with fake timers, confirm a checkpoint invoke fires
  at the 5-minute mark with only the bytes since the last checkpoint; the
  next timer doesn't schedule until the invoke's promise resolves; the
  pending timer is cleared and a final `is_final: true` call is made on
  `stopRecording`; nothing checkpoints for the cloud backend.
- `useLocalRecordingProgress.test.ts` / `MeetingDetailView.test.ts`: a poll
  response with an incremented `checkpoint_count` while `status ==
  'recording'` triggers a `content-updated` emit and a transcript/notes
  re-read; no emit when neither value changed.

### Manual

Record a ~12–15 minute local meeting (long enough to hit two or three
checkpoints); open it in the Library while still recording and confirm the
transcript and AI notes visibly grow roughly every 5 minutes without
duplicated content. Stop recording and confirm the final transcript/notes
are complete and `status` reaches `Done`. Repeat with the notes LLM not
installed and confirm the transcript still checkpoints normally while notes
stay absent throughout.

## Open questions

- **Incremental-merge prompt quality.** This is the biggest unknown: Gemma 3
  1B already needs tuning to avoid degenerating on a single-pass summary; a
  "merge new content into existing notes" prompt is a harder task for the
  same model. Needs hands-on prompt iteration once real checkpointed
  transcripts are available; if quality doesn't hold up, the fallback is
  regenerating from a *rolling window* of the transcript (e.g. last N
  minutes plus the existing notes as context) rather than a strict two-input
  merge — worth revisiting once the first version is in hand.
- **Token/context budget.** What's the actual context-window ceiling for the
  on-device Gemma 3 1B, and does "previous notes + 5 minutes of new
  transcript" reliably fit? This should be checked empirically before
  implementation locks in the 5-minute interval as fixed.
- **UI treatment for a just-refreshed checkpoint.** Silent in-place update
  (as designed here) vs. some explicit "updated Xm ago" indicator — punted
  to [Non-goals](#non-goals), flagged here for a product decision.
- **Failed-chunk data loss.** The error-handling table accepts that a failed
  checkpoint's audio slice is not retried and is simply missing from the
  final transcript. Is that acceptable, or does this need a retry-the-failed-
  chunk-at-the-next-checkpoint mechanism? Leaning toward accepting it for v1
  given how rare a mid-recording chunk-transcription failure should be, but
  flagging since it's a real (if narrow) correctness gap versus today's
  all-or-nothing transcription.
