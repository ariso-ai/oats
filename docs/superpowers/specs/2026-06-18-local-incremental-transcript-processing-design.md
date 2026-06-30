# Incremental Transcript Processing for the Local Backend — Specification

**Issue:** [#123](https://github.com/ariso-ai/oats/issues/123)
**Status:** Draft — pending review
**Date:** 2026-06-18
**Scope:** Local (on-device) backend only. Ariso/cloud backend is untouched.

---

## 1. Objective

Refresh AI meeting notes **every 5 minutes during a Local recording** instead of only
after the user stops. Each checkpoint re-transcribes the audio captured so far and updates
the AI notes **incrementally** — reusing the prior notes and folding in only the new
transcript since the last checkpoint — so notes evolve smoothly during a long meeting with
minimal latency and no wholesale regeneration.

### Done looks like

- A Local recording produces updated `ari-note.md` (and `transcript.md`) **while still
  recording**, on a ~5-minute cadence.
- Notes stay consistent across checkpoints — later checkpoints build on the previous notes
  rather than regenerating from scratch.
- During a long meeting, the user can watch the notes grow with minimal delay or duplication.
- Short meetings (< 5 min) behave exactly as today: one finalize pass, no checkpoints.

### Target users

Local/offline users recording long meetings who want useful notes before the meeting ends.

---

## 2. Background — current Local flow (what exists today)

| Concern | Today |
|---|---|
| Audio capture | Browser captures mic (+ optional system audio), encodes MP3 in-browser via lamejs (`src/composables/useRecorder.ts`). The full MP3 `Blob` is handed to Rust **only on stop**. |
| Finalize | `local_finalize_recording` → `finalize_core` (`src-tauri/src/transcribe.rs:140`) writes `recording.mp3`, runs STT over the whole file, writes `transcript.md`, marks `Done`, then spawns best-effort `process_notes`. |
| STT | Sidecar `ariso-stt --audio <path> --models <dir> --format json` (whole-file, with diarization). |
| Notes | Sidecar `ariso-stt notes --transcript <path> --models <dir>` → Gemma 3 1B → markdown on stdout → `ari-note.md`. Best-effort; failures set `meta.notes_error`, recording stays `Done`. LLM is opt-in (`llm_is_ready`). |
| Storage | `~/.ariso/recordings/<id>/` where `id = sanitize_iso_to_id(created_at)` (deterministic). Files: `recording.mp3`, `transcript.md`, `ari-note.md`, `meta.json`. Atomic write-then-rename (`src-tauri/src/storage.rs`). |
| Status / polling | `local_recording_status(id)` → `{status, hasTranscript, hasNote, notesStatus}`. Frontend polls every 2 s **after stop** via `useLocalRecordingProgress.ts`. |
| Retry | `retry_local_transcription`, `retry_local_notes` in `transcribe.rs`. |

Two facts this feature leans on:
1. The recording dir id is **deterministic** from `created_at`, so checkpoints and the final
   finalize can write to the **same** directory in place.
2. `finalize_core` already re-runs in place (retry reuses it), so "write in place, update meta"
   is an established pattern.

> Note: the 2026-06-03 design doc calls the notes file `note.md`; the shipped code uses
> **`ari-note.md`**. This spec uses the real filename.

---

## 3. Design decisions (confirmed)

| Decision | Choice | Rationale / tradeoff |
|---|---|---|
| **Transcription per checkpoint** | **Re-run STT over the full accumulated audio** (coarse 5-min micro-batching on the existing batch contract). | The Parakeet TDT model is a transducer and *could* stream, but the `ariso-stt` sidecar exposes only whole-file `--audio <path>` transcription, and its FluidAudio **diarizer needs the full utterance window** to cluster speakers — so speaker-labeled segments only exist after the whole audio is seen. Re-running full audio keeps diarization/labels consistent with no new streaming mode. Tradeoff: cost grows with meeting length (≈ O(n) per checkpoint, O(n²) over a meeting). Acceptable at a 5-min cadence; true streaming / delta-STT with cross-chunk diarization stitching is a noted follow-up. |
| **Notes per checkpoint** | **Feed prior `ari-note.md` + only the new transcript delta** to the LLM and ask it to update/extend. | Matches the issue's "reuse existing notes, append/update" goal; reduces churn and keeps the LLM prompt small and bounded. |
| **Live UX** (my recommendation) | Backend writes checkpoints to the recording dir during recording; a **compact live notes panel** in the recording view surfaces the evolving `ari-note.md`, reusing the existing status-polling infrastructure. | Lower risk than a bespoke live editor; directly delivers "see notes evolve." A richer live editor is a follow-up. |

---

## 4. Detailed design

### 4.1 Stable recording identity at record start

To let checkpoints write into the eventual recording dir, the recording's `created_at`
(and thus `id`) must be fixed **when recording starts**, not at stop.

- `WaveformView.vue` / `useRecorder.ts`: capture `created_at` (ISO UTC) and `title` at the
  moment recording starts; reuse the same `created_at` for every checkpoint and for the
  final `local_finalize_recording`. This guarantees all writes target one dir.
- If `created_at` is currently generated at stop, move it to start and thread it through.

### 4.2 Frontend checkpoint timer

- During an active Local recording, schedule a checkpoint every **5 minutes** of *elapsed
  recording time* (respecting pause — reuse the existing wall-clock anchors in
  `useRecorder.ts`; do not count paused time).
- On each tick:
  1. Snapshot the audio accumulated so far and encode it to a self-contained MP3 (flush a
     copy of the lamejs chunks without ending the live encoder, so recording continues
     uninterrupted).
  2. `invoke('local_checkpoint_recording', { audio, title, createdAt, durationSeconds })`.
  3. Skip the tick if a prior checkpoint for this recording is still in flight (coalesce —
     never queue overlapping checkpoints).
- Skip a checkpoint if no new audio has been added since the last one (e.g. fully paused).
- On stop: the existing finalize path runs (see 4.5) — the last checkpoint and finalize must
  not race (see 4.6).

### 4.3 New Tauri command + core: `local_checkpoint_recording`

`src-tauri/src/transcribe.rs`:

```text
local_checkpoint_recording(audio, title, created_at, duration_seconds) -> CheckpointResult
  └─ checkpoint_core(root, audio, title, created_at, duration_seconds)
```

`checkpoint_core` (new), reusing existing helpers:
1. Derive `id`/`dir` from `created_at`; create dir if absent.
2. Write the snapshot to `recording.mp3` (atomic; this is the latest full audio).
3. Ensure/refresh `meta.json`: status `Transcribing` while a checkpoint runs (or a new
   `status` value — see 4.4), update `duration_seconds`, set `last_checkpoint_at`.
4. `run_transcribe(full_audio, models)` → render markdown → write `transcript.md` in place.
5. **Incremental notes** (only if `llm_is_ready`): compute the transcript delta since the
   last checkpoint (4.4), then run notes in update mode (4.7), writing `ari-note.md`.
   Best-effort — a notes failure sets `meta.notes_error` but the checkpoint still succeeds
   with an updated transcript.
6. Update meta: `status = Done`-equivalent-for-checkpoint, persist the new checkpoint marker.

Runs on a background task; returns quickly. Best-effort throughout — a checkpoint failure
never aborts the live recording (frontend logs and continues; the next checkpoint retries).

### 4.4 `RecordingMeta` additions (`src-tauri/src/storage.rs`)

Add optional, backward-compatible fields (`#[serde(default, skip_serializing_if = "Option::is_none")]`):

- `last_checkpoint_at: Option<String>` — ISO timestamp of the last successful checkpoint.
- `notes_checkpoint_end_seconds: Option<f64>` — transcript end-time (segment `end`) that
  the current `ari-note.md` already incorporates. Used to slice the **delta** for the next
  notes update: segments with `start >= notes_checkpoint_end_seconds` are "new."
- (Optional) `checkpoint_count: u32` for diagnostics.

Delta computation re-runs STT on full audio (segments may shift slightly for past audio);
slicing by timestamp is pragmatic and avoids double-counting. Document this tradeoff inline.

Consider a lightweight `RecordingStatus::Checkpointing` variant **or** reuse `Transcribing`
during a checkpoint; reuse is simpler and the post-stop poller already treats `transcribing`
as "in flight." Decide during implementation; if reusing, ensure live-vs-finalized states
remain distinguishable to the UI.

### 4.5 Final finalize unification

On stop, `finalize_core` should produce the **final** transcript/notes consistently with the
checkpoints — i.e. reuse the last checkpoint's `ari-note.md` as the prior and apply the final
delta, rather than regenerating notes from scratch. Refactor so `finalize_core` and
`checkpoint_core` share the incremental-notes path (4.7). If no checkpoints occurred
(short meeting, or LLM not installed), finalize falls back to today's from-scratch notes.

### 4.6 Concurrency & safety

- **Per-recording mutex** keyed by `id` (e.g. an async lock map) so at most one
  checkpoint/finalize touches a recording dir at a time. The final finalize acquires the
  same lock; an in-flight checkpoint completes (or is awaited) before finalize proceeds, so
  the final pass always sees the latest state.
- All file writes stay atomic (write-tmp + rename), as today, so a concurrent reader/poller
  never sees a partial `transcript.md` / `ari-note.md`.
- Reuse the existing `NOTES_TIMEOUT` (300 s) bound on the notes sidecar per checkpoint.

### 4.7 Sidecar `notes` — incremental update mode

`src-tauri/ariso-stt`: extend the `notes` subcommand with an optional previous-notes input:

```
ariso-stt notes --transcript <delta-or-full.md> --models <dir> [--previous-notes <ari-note.md>]
```

- **Without `--previous-notes`** (today's behavior): generate notes from the full transcript.
- **With `--previous-notes`**: prompt Gemma to *update* the supplied notes by integrating the
  new transcript excerpt — keep existing structure (summary / discussion / decisions /
  action items), avoid duplicating prior points, append/refine only. Keep the existing
  repetition-penalty setting. Emit the **full updated** markdown to stdout (Rust overwrites
  `ari-note.md` atomically). Same stdout/stderr/exit-code contract as today.

Rust side: a `run_notes` variant (or extra arg) that passes `--previous-notes` and writes the
delta transcript to a temp file. `process_notes` becomes checkpoint-aware (prior note +
delta) while preserving the empty-output and error handling already in `transcribe.rs:107`.

### 4.8 Live notes UI (recommended, minimal)

- Generalize `useLocalRecordingProgress.ts` to poll `local.recordingStatus(id)` **during**
  recording (not just after stop), keyed on the start-time `id`.
- In the recording view (`WaveformView.vue`), add a compact, read-only **"AI notes
  (updating live)"** panel that shows the latest `ari-note.md` and a subtle "updated HH:MM"
  indicator after each checkpoint. Reading note content reuses the existing meeting-detail
  read path (or a small `local_read_note(id)` command if no read path is reachable
  mid-recording).
- Gating: show the panel only when the LLM is installed (`llm_is_ready`); otherwise show
  nothing extra (transcript still checkpoints silently). No notes panel for the Ariso backend.

---

## 5. Commands (build / test / run)

| Purpose | Command |
|---|---|
| Frontend typecheck/build | `npm run build` (vite) |
| Frontend unit tests | `npm test` (vitest) |
| Rust tests | `cargo test --manifest-path src-tauri/Cargo.toml` — on this Mac run with `DYLD_LIBRARY_PATH` set and `-- --test-threads=1` (see memory: *Rust test env workaround*). |
| Run the app | Tauri dev / bundle (see memory: *Driving app via MCP*, *macOS permission testing*). |

Fresh worktrees need `src-tauri/binaries` copied, `npm ci`, and a `vite:build` before they
compile (see memory: *Fresh worktree build setup*).

---

## 6. Files to change / add

**Rust (`src-tauri/src/`)**
- `transcribe.rs` — new `local_checkpoint_recording` command + `checkpoint_core`; refactor
  shared incremental-notes path; make `process_notes` checkpoint-aware; per-id lock.
- `storage.rs` — `RecordingMeta` fields (`last_checkpoint_at`, `notes_checkpoint_end_seconds`,
  optional `checkpoint_count`); helper to slice transcript delta by `end` seconds.
- `commands.rs` — register the new command; optional `local_read_note` for live content.
- `lib.rs`/`main.rs` — add the command to the Tauri `invoke_handler`.

**Sidecar (`src-tauri/ariso-stt/`)**
- `notes` subcommand: optional `--previous-notes` and an update-mode prompt.

**Frontend (`src/`)**
- `tauri.ts` — `local.checkpointRecording(...)` (+ `local.readNote` if added).
- `composables/useRecorder.ts` — fix `created_at` at start; expose an MP3 snapshot/flush.
- `views/WaveformView.vue` — 5-min checkpoint timer; live notes panel.
- `composables/useLocalRecordingProgress.ts` — allow polling during recording.

**Docs**
- Update the README sidecar-contract section for the `--previous-notes` notes option.

---

## 7. Code style

- Match surrounding code: Rust with `tokio` async, `Result<T, String>` command signatures,
  atomic write-then-rename, doc-comments explaining *why* (as in `transcribe.rs`).
- New `RecordingMeta` fields are additive and `#[serde(default)]` so existing `meta.json`
  files keep deserializing.
- Frontend: Vue 3 + TypeScript composables, `invoke` via `src/tauri.ts` (no ad-hoc invokes
  in components).
- Best-effort discipline: notes/checkpoint failures log + record in meta, never crash a
  recording.

---

## 8. Testing strategy

Follow the existing `ARISO_STT_BIN` stub-script pattern (no real models in CI):

- `checkpoint_core` happy path: full-audio STT stub + notes stub → `transcript.md` and
  `ari-note.md` written, `last_checkpoint_at` / `notes_checkpoint_end_seconds` set.
- Incremental notes: a checkpoint with an existing `ari-note.md` invokes the notes stub with
  `--previous-notes` and only the delta transcript; assert the stub received both.
- Best-effort: notes stub exits 1 → checkpoint still updates `transcript.md`, sets
  `notes_error`, recording not failed.
- LLM-not-ready: checkpoint updates transcript, skips notes, no `notes_error`.
- Concurrency: two checkpoints (or checkpoint + finalize) for one id don't corrupt the dir
  (serialized by the per-id lock); final finalize observes the latest checkpoint state.
- Short meeting: no checkpoint fires; finalize behaves exactly as today (regression-guard
  existing `finalize_*` tests stay green).
- Frontend (vitest): timer fires checkpoints on cadence, coalesces overlaps, skips when no
  new audio / when paused; live-notes polling starts during recording and stops after.
- Swift Gemma update-mode generation validated manually (real model), as today.

---

## 9. Boundaries

**Always**
- Keep notes/checkpoints best-effort: never fail or interrupt a live recording because a
  checkpoint or notes pass failed.
- Write all files atomically; keep the recording dir id deterministic from `created_at`.
- Keep `RecordingMeta` changes backward-compatible (`serde(default)`).
- Scope strictly to the Local backend.

**Ask first**
- Changing the 5-minute cadence, or making it configurable/user-facing.
- Introducing a new `RecordingStatus` variant vs. reusing `Transcribing`.
- Any change to the Ariso/cloud backend or shared finalize behavior beyond the refactor.
- Switching the notes model or altering the base notes prompt structure.

**Never**
- Re-architect STT into a streaming pipeline (delta-STT is an explicit follow-up, not now).
- Discard a good transcript because notes failed.
- Add root-level docs or testing-plan markdown beyond what the issue requests (per the
  issue's implementation guidelines). *(This SPEC.md was explicitly requested.)*
- Block recording on LLM readiness — notes remain opt-in.

---

## 10. Acceptance criteria

1. During a Local recording longer than ~5 min, `transcript.md` and (if LLM installed)
   `ari-note.md` in the recording dir are updated roughly every 5 minutes, before stop.
2. Each notes update reuses the previous `ari-note.md` and folds in new transcript content
   without regenerating from scratch or duplicating prior points.
3. The recording view shows the evolving notes with an "updated" indicator (LLM installed).
4. Stopping produces final notes consistent with the last checkpoint (no jarring rewrite).
5. A checkpoint or notes failure leaves the recording intact and recording continues.
6. Short recordings and recordings without the LLM behave as they do today.
7. Existing Rust/frontend tests stay green; new tests above pass.

---

## 11. Risks & follow-ups

- **Cost on long meetings:** full-audio STT each checkpoint is O(n²) overall. Mitigation:
  5-min cadence; follow-up = delta-STT with cross-chunk diarization stitching.
- **Delta slicing accuracy:** re-running STT can shift earlier segment timings; timestamp
  slicing is pragmatic but may rarely drop/duplicate a boundary sentence. Acceptable; revisit
  if note quality suffers.
- **Small-model update drift:** Gemma 3 1B in update mode may restructure notes; the
  repetition penalty and an explicit "preserve structure, append only" prompt mitigate this.
- **Follow-ups:** configurable cadence; richer live notes UI; streaming generation progress;
  delta-STT.
