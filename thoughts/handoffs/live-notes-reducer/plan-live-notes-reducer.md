---
date: 2026-07-22T00:00:00-04:00
type: plan
status: complete
plan_file: thoughts/shared/plans/PLAN-live-notes-reducer.md
---

# Handoff: Local Live Notes Reducer Plan

## Summary

Created and revised a concrete implementation plan for replacing local/offline full-note regeneration with a simple reducer-style note update path over transcript deltas. The revised plan requires one shared Rust implementation across macOS and Windows and limits each `ariso-stt` sidecar to the same thin local-LLM completion surface.

## Plan Created

- `thoughts/shared/plans/PLAN-live-notes-reducer.md`

## Core Technical Decisions

- Keep notes plain Markdown with stable headings: Summary, Key Points, Decisions, Action Items.
- Add minimal reducer metadata to `RecordingMeta`: `notes_cursor`, `notes_source_hash`, and `notes_job_id`.
- Add `NotesStatus::Updating` and allow failed latest updates to surface even when `has_note` is true.
- Use full note generation only for the first note or when no prior note can be read.
- Use reducer generation for appends: previous note + transcript delta + optional bounded recent transcript window.
- Do not delete or blank the last good note on retry, update failure, blank model output, or stale job discard.
- Do not add a sidecar-level `reduce-notes` implementation. Put full/reducer policy, prompts, validation, cursoring, and stale guards in shared Rust.
- Add the same minimal completion command to both platform sidecars; native code loads the local LLM and returns generated text, but owns no meeting-note behavior.
- Treat existing STT, diarization, transcript normalization, model acquisition, and first-note behavior as compatibility surfaces. Changes in `ariso-stt` are additive command dispatch and generic completion only.

## Important Code Findings

- `src-tauri/src/transcribe.rs`
  - Current `run_notes` shells out to `ariso-stt notes --transcript ...` and parses JSON/raw Markdown.
  - `process_notes` reads the full transcript, rejects blank output, writes the note, applies generated title, and records `notes_written`.
  - Fresh finalize writes transcript artifacts and spawns notes after marking status `Done`.
  - Append transcribes only the new clip and stitches segments, but then still spawns full notes regeneration.
  - `retry_notes_core` removes existing notes before retrying, which should change for this feature.
- `src-tauri/src/storage.rs`
  - `RecordingMeta` already has `notes_error`, `notes_written`, and title metadata, but no cursor or active-job state.
  - `write_atomic` is available and should continue to be used for note/transcript writes.
- `src-tauri/src/commands.rs`
  - `local_recording_status` currently derives note status from `has_note` and `notes_error`; this makes old notes mask update progress/failure.
  - `read_recording_file` already prefers vault notes and falls back to legacy `ari-note.md`.
- `src-tauri/ariso-stt/macos/Sources/ariso-stt/main.swift`
  - The macOS notes path is full-transcript one-shot and should gain only a thin completion/prompt-file entrypoint, not reducer policy.
- `src-tauri/ariso-stt/windows/src/notes.rs`
  - The Windows notes path already has useful bounded chunking/reduction patterns. Those ideas should be moved into shared Rust if needed, not kept as a second reducer implementation.
- `src/composables/useLocalRecordingProgress.ts`
  - `deriveStage` currently treats any existing note as ready, so it needs an updating/failed-with-note distinction.
- `src/views/MeetingDetailView.vue`
  - Transcript and AI notes are already separate tabs; the main UX change is status handling while preserving readable content.

## Risks To Watch

- Reducer output may drop prior decisions/action items; test persistence directly.
- Detached notes jobs may race append jobs; use `notes_job_id` and source hashes before writes.
- Old notes may hide failed updates unless the status derivation changes first.
- macOS and Windows sidecar contracts may drift unless the completion contract stays tiny and documented.
- A Windows loopback model transport is acceptable, but neither adapter may make external network requests during local inference.
- If a prior note is missing while a cursor remains, fall back to full generation and reset cursor on success.

## Recommended First Implementation Step

Start with metadata/status and frontend stage changes. Then extract Rust note orchestration and build the reducer prompt/validation there before touching `ariso-stt`; the sidecar change should be limited to the narrowest completion adapter Rust needs.

## Verification Expected After Implementation

```bash
npm test

cargo test --manifest-path src-tauri/ariso-stt/windows/Cargo.toml

cd src-tauri/ariso-stt/macos && swift build -c release

DYLD_LIBRARY_PATH="/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift-5.5/macosx" \
  cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1
```

If the macOS sidecar changes, rebuild it with Xcode and copy the resulting binary/resources into `src-tauri/ariso-stt/binaries`.
