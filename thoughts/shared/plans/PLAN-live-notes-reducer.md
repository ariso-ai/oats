# Plan: Local Live Notes Reducer

## Goal

Move local/offline AI notes from "regenerate the whole note from the full transcript" toward a bounded reducer path:

```text
reduce(previous_note_markdown, transcript_delta_markdown, options?) -> updated_note_markdown
```

The first implementation should stay deliberately simple:

- Keep the canonical note as plain Markdown.
- Preserve stable output headings.
- Track only the minimal reducer metadata needed to know which transcript segments are reflected in the latest successful note.
- Keep summarization separate from STT and test it directly from transcript fixtures.
- Keep the last good note visible while background updates run or fail.
- Keep reducer policy, prompt construction, validation, cursoring, and retry behavior in shared Rust, not in platform sidecars.
- Minimize `ariso-stt` changes to a thin local-LLM completion surface when the existing `notes` command is not enough.

This plan is scoped to local/offline notes. It does not introduce true streaming STT checkpoints during an active recording; it improves the note-generation path after local transcript segments are persisted by fresh finalization, append, or retry.

## Cross-Platform and Change-Scope Constraints

This feature must ship through one product-logic implementation on both macOS and Windows:

- Shared Tauri Rust is the only implementation of reducer policy, prompts, batching, cursor/hash/job state, validation, retry semantics, and durable writes.
- Vue/TypeScript owns presentation and polling only. It does not implement note reduction or platform branches.
- The macOS Swift and Windows Rust sidecars each expose the same bounded `llm-complete` transport contract because model loading is necessarily platform-specific.
- No reducer prompt, Markdown schema, transcript cursor, retry policy, or persistence rule may be copied into either sidecar.
- Existing transcription and one-shot first-note behavior are compatibility surfaces. Sidecar changes are additive command dispatch and generic model completion only; do not refactor ASR, diarization, transcript normalization, model acquisition, or the existing `notes` command.
- Platform differences belong behind the completion adapter. The shared Rust caller must not branch on macOS versus Windows.

The result is not zero platform code: model adapters remain native. It is one copy of all meeting-note behavior, with two intentionally thin inference adapters.

## Current State

The local pipeline already has a clean separation point after STT:

- `src-tauri/src/transcribe.rs`
  - `local_finalize_recording` writes audio, transcribes, persists `segments.json` and `transcript.md`, marks the recording `Done`, then spawns notes generation in the background.
  - `process_notes` reads the complete transcript, calls `run_notes`, writes a vault note or legacy `ari-note.md`, applies a generated title when allowed, and records `notes_written`.
  - Appends already transcribe only the new clip, offset/stitch segments, re-render the full transcript, then spawn full notes regeneration.
  - `retry_notes_core` currently removes the old note before retrying, which is the wrong behavior for a live-update UX.
- `src-tauri/src/storage.rs`
  - `RecordingMeta` has `notes_error`, `notes_written`, `title_is_default`, and transcript/audio metadata, but no reducer cursor or active-note-job status.
  - `RecordingStatusView` exposes only `status`, `has_transcript`, `has_note`, and `notes_status`.
- `src-tauri/src/commands.rs`
  - `local_recording_status` derives notes status mostly from `has_note` and `notes_error`, so an existing note can hide an update or failure from the frontend.
  - `read_recording_file` already prefers the vault note body and falls back to `ari-note.md`.
- `src-tauri/ariso-stt/macos/Sources/ariso-stt/main.swift`
  - The `notes` command currently combines prompt policy, model inference, and title generation in a one-shot full-transcript path.
- `src-tauri/ariso-stt/windows/src/notes.rs`
  - This path already has useful bounded chunk/reduction concepts: chunk limits, model-call limits, and runtime-budget checks. Treat it as an implementation reference to port into shared Rust policy, not as a second reducer implementation.
- `src/composables/useLocalRecordingProgress.ts`
  - `deriveStage` maps `hasNote || notesStatus === 'ready'` to `ready`, which makes old notes look fully current while a newer update is running or failed.
- `src/views/MeetingDetailView.vue`
  - The transcript and AI note tabs are already separate, which matches the desired UX. The status labels need enough backend state to distinguish "ready", "updating", and "update failed but old note is still visible".

## Target Design

### Platform Boundary

The reusable boundary should be:

```text
Vue/TS UI
  -> Tauri commands and polling
    -> shared Rust local_notes orchestration
      -> ariso-stt platform sidecar only for local model completion
```

Shared Rust owns:

- Full-vs-reducer policy.
- Transcript segment cursoring.
- Transcript delta and recent-window rendering.
- Full-note and reducer prompt construction.
- Markdown scaffold normalization and output validation.
- Source hashing and stale-job guards.
- Retry behavior.
- Metadata updates and atomic note writes.

The `ariso-stt` sidecars own only:

- Loading the platform-local LLM.
- Running a prompt with bounded generation parameters.
- Returning raw generated text or a narrow JSON wrapper.

This avoids writing the reducer twice. macOS and Windows both implement the same thin completion adapter and reuse the Rust reducer module unchanged.

### Markdown Scaffold

Keep the output plain Markdown with stable headings:

```markdown
# Meeting Notes

## Summary

## Key Points

## Decisions

## Action Items
```

The reducer prompt should ask the model to preserve prior valid content unless the new transcript corrects or supersedes it, merge duplicates, and update only these sections. This gives structure in the artifact without adding a rich internal note state object.

### Minimal Reducer Metadata

Extend `RecordingMeta` with reducer/job fields:

```rust
pub notes_cursor: Option<usize>,
pub notes_source_hash: Option<String>,
pub notes_job_id: Option<String>,
```

Recommended semantics:

- `notes_cursor`: number of transcript segments reflected in the latest successful note.
- `notes_source_hash`: hash of the source inputs for the latest successful note or active job. Use SHA-256 over the previous note, transcript delta, and recent context text.
- `notes_job_id`: set while a notes job is running. Clear on success, failure, stale discard, or cancellation-equivalent exit.

These fields are intentionally operational metadata, not a typed internal note model.

### Status Semantics

Extend `NotesStatus` with an updating state:

```rust
pub enum NotesStatus {
    Pending,
    Updating,
    Ready,
    Failed,
}
```

Derive status as:

- `Updating`: `notes_job_id.is_some()` and an existing note is present.
- `Pending`: `notes_job_id.is_some()` without a note, or no note/no error after transcript completion.
- `Failed`: `notes_error.is_some()` after the latest notes job failed. This should be returned even when `has_note == true`; the old note remains readable, but the latest update failed.
- `Ready`: note exists and there is no active job or notes error.

Add `notes_written` to `RecordingStatusView` so the frontend can show a lightweight "last updated" timestamp.

### Minimal Sidecar Contract

Keep the existing full-generation command for first notes during the MVP if that reduces risk:

```text
ariso-stt notes --transcript <path> --models <path>
```

Do not add a sidecar-level `reduce-notes` command. That would put product/reducer policy into every platform sidecar.

Instead, add the smallest reusable local-LLM completion surface needed by Rust:

```text
ariso-stt llm-complete \
  --prompt <path> \
  --models <path> \
  [--max-tokens 2048] \
  [--temperature 0.3] \
  [--repetition-penalty 1.15]
```

Output should be raw generated text, or this narrow shape if JSON is easier to parse consistently:

```json
{"text":"..."}
```

Rust should create the prompt file, call the sidecar with argv-based arguments, parse the response, strip code fences if needed, validate Markdown, and write the note.

The reducer prompt, built in Rust, should be plain and conservative:

- The previous note is the canonical prior state.
- The delta transcript is the only new source of truth.
- The optional recent transcript window is context for resolving references, not a license to re-summarize the whole meeting.
- Preserve decisions and action items unless explicitly contradicted.
- Do not return an empty note.
- Return note Markdown only, unless Rust deliberately requests a JSON wrapper for the completion command.

### Reducer Policy

Use full generation only when there is no prior note/cursor.

Use reducer generation when:

- A prior note exists.
- `segments.json` has segments after `notes_cursor`.
- The new transcript delta is non-empty after rendering and trimming.

Inputs to the reducer:

- Prior note body, read from the vault note first and legacy `ari-note.md` second.
- Transcript delta rendered from `segments[notes_cursor..]`.
- Optional small rolling transcript window before the cursor, for example the prior 5-10 minutes or a capped number of segments.

On success:

- Atomically write the updated note.
- Update `notes_cursor` to the current segment count.
- Update `notes_source_hash`.
- Update `notes_written`.
- Clear `notes_error` and `notes_job_id`.
- Apply generated title only if `title_is_default` still allows it.

On blank output, sidecar failure, timeout, or stale source detection:

- Keep the last good note in place.
- Leave `notes_cursor` unchanged.
- Set `notes_error`.
- Clear `notes_job_id`.
- Do not replace the note with empty content.

## Implementation Tasks

### 1. Add Reducer Metadata and Status

Files:

- `src-tauri/src/storage.rs`
- `src-tauri/src/commands.rs`
- `src/tauri.ts`
- `src/composables/useLocalRecordingProgress.ts`

Steps:

1. Add optional `notes_cursor`, `notes_source_hash`, and `notes_job_id` fields to `RecordingMeta` with serde defaults for existing recordings.
2. Extend `NotesStatus` with `Updating`.
3. Add `notes_written` to `RecordingStatusView`.
4. Update `derive_notes_status` in `commands.rs` to account for active jobs and failed updates even when a prior note exists.
5. Update TypeScript status types and `deriveStage`.
6. Add a frontend stage such as `notes-updating` so ready notes can remain visible while the status chip says the update is running.

Acceptance criteria:

- Existing recordings deserialize without migration work.
- A recording with an old note and an active job reports `notesStatus: 'updating'`.
- A recording with an old note and a failed latest job reports `notesStatus: 'failed'`, while `hasNote` remains true.

### 2. Extract Local Notes Orchestration

Files:

- `src-tauri/src/transcribe.rs`
- New recommended module: `src-tauri/src/local_notes.rs`
- `src-tauri/src/main.rs`

Steps:

1. Move note-specific orchestration out of `transcribe.rs` into a local notes module:
   - Sidecar invocation.
   - Input temp-file preparation.
   - Full-note and reducer prompt construction.
   - Source hashing.
   - Stale-job guard.
   - Markdown cleanup and validation.
   - Note write and metadata updates.
2. Keep `transcribe.rs` responsible for STT, segment stitching, transcript rendering, and starting the notes job.
3. Model the job mode explicitly:

```rust
enum NoteJobMode {
    Full,
    Reduce {
        previous_note: String,
        delta_transcript: String,
        recent_transcript: Option<String>,
        target_cursor: usize,
    },
}
```

4. Reuse the existing transcript-change guard, but make the guard job-specific with `notes_job_id` and source hash checks.

Acceptance criteria:

- Fresh transcription still generates first notes.
- Append transcription starts a reducer job when a prior note and cursor exist.
- Stale note jobs cannot overwrite a newer transcript/note state.

### 3. Add Transcript Delta Rendering

Files:

- `src-tauri/src/storage.rs`
- `src-tauri/src/local_notes.rs`

Steps:

1. Add a helper that renders a transcript fragment from a slice of `TranscriptSegment`.
2. Use the same speaker/timestamp conventions as `render_markdown` so fixtures are realistic.
3. Add a bounded recent-window helper:
   - Prefer a time window if segment timestamps are reliable.
   - Fall back to a capped segment count.
4. Treat empty delta as "no notes update needed", not as an error.

Acceptance criteria:

- Delta rendering produces stable Markdown from `segments.json`.
- Appends with no new segments do not call the model.
- The reducer never needs to read the entire transcript unless running first full generation.

### 4. Add Minimal Sidecar Completion Surface

Files:

- `src-tauri/ariso-stt/macos/Sources/ariso-stt/main.swift`
- `src-tauri/ariso-stt/windows/src/main.rs`
- `src-tauri/ariso-stt/windows/src/notes.rs`
- `src-tauri/ariso-stt/shared/README.md`

Steps:

1. Add a thin `llm-complete` command, or an equivalent prompt-file mode, that reads a Rust-authored prompt file and runs the existing notes LLM.
2. Reuse `loadNotesModel`, `ChatSession`, `GenerateParameters`, and `stripCodeFence` where possible.
3. Keep generation parameters bounded and explicit: max tokens, temperature, repetition penalty, and runtime timeout from Rust.
4. Return raw text or `{ "text": "..." }`; do not return `{title, notes}` for reducer calls.
5. Do not build reducer prompts, inspect transcript deltas, update cursors, or normalize meeting-note headings inside the sidecar.
6. Implement the same argument bounds and `{ "text": "..." }` output on Windows, using its existing packaged local-model runtime without adding reducer semantics.
7. Update the shared sidecar README to document:
   - Existing `notes` command.
   - New completion command.
   - Output contract.
   - Security invariant: direct argv invocation, no shell, and no external network access. A Windows llama.cpp loopback transport remains local to the sidecar process boundary.
8. Leave transcription flags, ASR/diarization code, existing `notes` prompts, and model installation behavior unchanged.

Acceptance criteria:

- Existing `notes` behavior remains compatible for first-note generation.
- The new completion command can run a Rust-authored prompt and return parseable text/JSON.
- The sidecar contains no reducer-specific branching or transcript-cursor logic.
- macOS and Windows accept the same completion arguments and return the same JSON shape.
- Existing platform transcription tests remain green without fixture changes caused by this feature.
- Completion failure exits non-zero or returns invalid output in a way Rust can classify as a failed update without deleting prior notes.

### 5. Wire Fresh, Append, and Retry Flows

Files:

- `src-tauri/src/transcribe.rs`
- `src-tauri/src/local_notes.rs`
- `src-tauri/src/commands.rs`

Steps:

1. Fresh finalize:
   - After transcript write, start a full notes job.
   - On success, set `notes_cursor` to the segment count.
2. Append finalize:
   - After segment stitching and transcript write, start a reducer job when possible.
   - Set `notes_job_id` before spawning.
   - Keep old note content intact during the job.
   - Call the sidecar completion surface with a Rust-built reducer prompt, not a sidecar reducer command.
3. Retry:
   - If a prior note and cursor exist, retry the reducer/update path without deleting the note.
   - If no prior note exists, run full generation.
   - Reserve destructive full regeneration for an explicit future command, not the default retry path.
4. Ensure note errors are cleared at job start only after recording the active job state. On failure, set the latest error.

Acceptance criteria:

- Retry no longer removes a readable prior note.
- Append updates use only transcript delta plus bounded context.
- First-note generation still works for recordings with no prior note.

### 6. Preserve the Live UX

Files:

- `src/composables/useLocalRecordingProgress.ts`
- `src/views/MeetingDetailView.vue`
- `src/composables/useBackend.ts`
- `src/tauri.ts`

Steps:

1. Update status labels:
   - No note yet: "Generating AI Notes".
   - Old note plus active job: "Updating AI Notes".
   - Old note plus failed latest job: "AI Notes update failed".
   - Ready: "AI Notes ready".
2. Keep the AI Notes tab enabled whenever `hasNote` is true.
3. Keep showing old note content while updates run or fail.
4. Surface `notes_written` as a small "Last updated" timestamp near the note status, if the design has a natural place for it.
5. Avoid adding live editing complexity to the AI note panel.

Acceptance criteria:

- Appending a recording does not make the note disappear.
- A failed update does not replace prior notes with an empty state.
- Transcript and notes remain separate tabs.

### 7. Add Transcript-Driven Tests

Files:

- Rust tests near `src-tauri/src/local_notes.rs`, `storage.rs`, or existing local transcription tests.
- `src/composables/useLocalRecordingProgress.test.ts`
- `src/views/MeetingDetailView.test.ts`
- `src/composables/useBackend.test.ts`

Rust test cases:

1. Existing metadata without reducer fields deserializes with `None` defaults.
2. `derive_notes_status` returns `Updating` for active job plus existing note.
3. `derive_notes_status` returns `Failed` for latest failed update plus existing note.
4. Full first-note job writes a note and sets cursor to segment count.
5. Reducer job receives only new segments after `notes_cursor`.
6. Reducer success updates note, cursor, hash, and `notes_written`.
7. Reducer blank output keeps previous note and cursor unchanged.
8. Stale reducer output cannot overwrite a newer transcript/note state.

Frontend test cases:

1. `deriveStage` distinguishes ready, updating, pending, failed-with-note, and failed-without-note.
2. `MeetingDetailView` keeps the AI Notes tab enabled and existing note visible during updating.
3. Retry UI remains available after a failed update.
4. `LocalBackend.getMeetingDetail` reads local note and transcript bodies for local fixtures.

Verification commands:

```bash
npm test

DYLD_LIBRARY_PATH="/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift-5.5/macosx" \
  cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1
```

Also run the Windows sidecar tests and compile the macOS sidecar after changing either adapter:

```bash
cargo test --manifest-path src-tauri/ariso-stt/windows/Cargo.toml

cd src-tauri/ariso-stt/macos
swift build -c release
```

For a distributable macOS sidecar, rebuild and copy the binary/resources:

```bash
cd src-tauri/ariso-stt/macos
xcodebuild build \
  -scheme ariso-stt \
  -configuration Release \
  -destination 'generic/platform=macOS' \
  -derivedDataPath .xcode \
  -skipMacroValidation
cp .xcode/Build/Products/Release/ariso-stt ../../binaries/ariso-stt-aarch64-apple-darwin
cp -R .xcode/Build/Products/Release/mlx-swift_Cmlx.bundle ../../binaries/
```

## Pre-Mortem Risks

### High: The reducer drops old decisions or action items

Small local models may overwrite or compress away important prior content. Mitigate with a conservative prompt, stable headings, action/decision persistence tests, and keeping full regeneration as an explicit fallback rather than the default update path.

### High: Stale background jobs overwrite newer notes

Appends already run under an append lock, but note jobs are detached. Mitigate with `notes_job_id`, source hashing, and a final pre-write metadata/source check.

### Medium: Existing notes hide update failures

The current status derivation treats `has_note` as ready. Mitigate by adding `Updating` and allowing `Failed` to surface even when a previous note remains readable.

### Medium: Sidecar command drift across platforms

macOS and Windows sidecars already differ. Mitigate by keeping sidecars thin: one completion contract documented once, contract tests on both adapters, and all reducer policy in shared Rust.

### Medium: Cursor semantics become wrong after manual note deletion or metadata edits

If a note disappears but cursor remains, the reducer has no valid prior state. Mitigate by falling back to full generation when the prior note cannot be read, and reset cursor on successful full generation.

### Medium: Full first-note generation still struggles on very long meetings

The reducer improves appends, but the first note can still be long. Mitigate by porting the Windows chunk/reduction budget to macOS full-note generation as part of the sidecar work if long fixtures fail.

## Out of Scope

- Live audio checkpointing during an active recording.
- Cloud backend summary behavior.
- A rich typed intermediate note-state DSL.
- Reducer or prompt policy duplicated inside platform sidecars.
- User-editable AI note merging.
- Full public dataset benchmark automation.

## Suggested Build Order

1. Metadata/status changes with frontend stage tests.
2. Rust local notes extraction and transcript-delta helpers.
3. Minimal sidecar completion command or prompt-file mode.
4. Wire append/retry to reducer.
5. UX polish for updating/failed-with-note states.
6. Regression fixtures and full verification.
