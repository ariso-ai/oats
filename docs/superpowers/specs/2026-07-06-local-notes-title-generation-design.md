# Local AI-notes title generation (offline mode)

**Issue:** [#208](https://github.com/ariso-ai/oats/issues/208)
**Date:** 2026-07-06
**Backend scope:** Local / offline only. Cloud (Ariso) mode is untouched.

## Problem

In offline mode a local recording is created with a default, timestamp-based title
(`timestampTitle()` in `src/composables/useBackend.ts`, e.g. `"Mon Jul 6 @ 959AM"`). After
AI notes are generated on-device, that placeholder title is still there — the vault note
reads as unfinished even though we now have enough content to name it well.

We want the note title to be regenerated to something relevant **when AI notes are
generated**, but only when the title is still the untouched default. If the user has
renamed the recording, their title must be preserved.

## Goal

When local AI notes finish generating:

- If the recording's title is still the pristine auto-generated default → replace it with a
  concise, relevant title produced on-device.
- If the user has renamed the recording → leave the title exactly as-is.
- Regenerate **at most once** — only while the title is the untouched default. Once we
  replace it (or the user renames), the title is no longer "default" and is never rewritten
  automatically again.

Everything runs on-device; nothing leaves the Mac (offline privacy guarantee intact).

## Approach

The local `ariso-stt` sidecar (`src-tauri/ariso-stt`, a Swift package built into
`src-tauri/binaries/ariso-stt-aarch64-apple-darwin`) owns the on-device LLM. Its `notes`
subcommand loads a Gemma-3-1b model and emits meeting notes. We extend that same subcommand
to **also** produce a title, reusing the already-loaded model, and the Rust app applies the
title only when the current title is the default.

## Design

### 1. Sidecar (`ariso-stt`, Swift) — `notes` emits a title alongside the notes

`Sources/ariso-stt/main.swift`.

Today the `notes` branch writes raw Markdown to stdout. Change it to emit a small JSON
object so the notes body and the title stay cleanly separated (rather than smuggling the
title into the Markdown, which is written verbatim into the vault note):

```json
{ "title": "Vault storage backend testing", "notes": "## Summary\n…" }
```

Implementation:

- After `generateNotes()` returns the notes Markdown, run a **second, cheap LLM turn on the
  same already-loaded `ModelContainer`** (no second `loadNotesModel` — the load is the
  expensive part) to produce a title from the *generated notes* (already distilled, tighter
  than the raw transcript).
- Title prompt constraints: a short, specific title; plain text only; Title Case;
  roughly ≤ 40 characters; no surrounding quotes, no Markdown, no trailing punctuation, no
  `"Meeting Notes:"`-style prefixes; use only facts present in the notes.
- Sanitize the model output in Swift: strip code fences / quotes / newlines, collapse
  internal whitespace, trim. If the result is empty after sanitizing, emit `"title": ""`.
- Encode `{ title, notes }` with `JSONEncoder` and write to stdout. `notes` is the existing
  fence-stripped Markdown; `title` is the sanitized string (possibly empty).

The `transcribe` (default) path and its JSON output are unchanged.

### 2. Rust — parse the new `notes` contract, tolerantly

`src-tauri/src/transcribe.rs`.

- Introduce `NotesOutput { title: Option<String>, notes: String }`.
- `run_notes` returns `NotesOutput` instead of `String`.
- **Tolerant parse:** attempt to decode stdout as the `{title, notes}` JSON object first; if
  that fails, fall back to `NotesOutput { title: None, notes: <raw stdout trimmed> }`. This
  keeps behavior safe if a mismatched/older binary emits plain Markdown, and lets the many
  existing `notes` test stubs (which `echo` plain Markdown) keep passing with `title: None`
  → no retitle. An empty/whitespace `title` from JSON normalizes to `None`.
- Existing empty-notes handling stays keyed on `notes`.

### 3. Rust — apply the title only if it is the default

`src-tauri/src/storage.rs`, `src-tauri/src/transcribe.rs`, `src-tauri/src/commands.rs`.

- Add `title_is_default: bool` to `RecordingMeta` with `#[serde(default)]` (defaults to
  `false`). Pre-feature recordings already on disk deserialize with `false`, so they are
  **never** auto-retitled — a safe migration.
- Set `title_is_default: true` where a fresh local recording is created
  (`fresh_recording_core`): the title there is the timestamp default passed down from
  `finalize`. (Appended clips reuse the existing recording's meta and do not reset the flag.)
- Set `title_is_default: false` in `rename_local_recording` (`commands.rs`) when the user
  renames — their title is now intentional.
- In `process_notes` (`transcribe.rs`), in the `Ok(notes)` arm, **after `vault::write_note`
  succeeds**:
  - If `meta.title_is_default` **and** `output.title` is `Some(new)` (non-empty) **and**
    `new != meta.title`:
    - Call `vault::rename_recording_artifacts(&meta.id, &meta.created_at, &audio_file, &new)`
      (the same tested path the user-rename flow uses). It renames the note file + audio
      attachment, rewrites front-matter `title:` and the embed, and returns the new
      `audio_file`.
    - On success: `meta.audio_file = Some(returned)`, `meta.title = new`,
      `meta.title_is_default = false`.
    - On error: log and keep the default title (the note itself is already written). This
      retitle step is best-effort and must never turn a successful notes run into a failure.
  - Then persist `meta` (this is the same `write_meta` that already stamps `notes_written`).

Ordering rationale: `write_note` first (so notes success never depends on retitling), then
rename. This reuses the existing, tested `rename_recording_artifacts` (with its collision
handling and rollback) instead of inventing a new write ordering. The extra small note write
it implies is negligible.

The `retry_notes` / `retry_local_notes` paths flow through `process_notes`, so a retry on a
recording whose title is still the default will also (re)generate a title — consistent with
the "only while default" rule.

### 4. Frontend

No change required. Local recording progress is polled
(`src/composables/useLocalRecordingProgress.ts` → `MeetingDetailView.vue`); when notes reach
`ready` the view re-reads the recording and shows the new title. Verified during
implementation.

## Build / packaging

- Editing `src-tauri/ariso-stt/Sources/**` invalidates the CI sidecar cache
  (`.github/workflows/release.yaml` keys on `Sources/**` + `Package.resolved`), so the
  release build rebuilds the binary automatically.
- Locally, rebuild and stage the binary to test end-to-end:
  ```bash
  cd src-tauri/ariso-stt
  xcodebuild build -scheme ariso-stt -configuration Release \
    -destination 'generic/platform=macOS' -derivedDataPath .xcode -skipMacroValidation
  cp .xcode/Build/Products/Release/ariso-stt ../binaries/ariso-stt-aarch64-apple-darwin
  cp -R .xcode/Build/Products/Release/mlx-swift_Cmlx.bundle ../binaries/
  ```

## Testing

### Rust (automated)

- `run_notes`: parses the `{title, notes}` JSON; tolerant fallback to `{None, raw}` for
  non-JSON stdout; empty/whitespace title → `None`.
- `process_notes` via the `ARISO_STT_BIN` stub emitting `{title, notes}`:
  - `title_is_default = true` → note written, `meta.title` updated to the generated title,
    `title_is_default` cleared, and the vault note file renamed to the new-title basename.
  - `title_is_default = false` → note written, title **unchanged**, file **not** renamed.
  - Empty `title` (or `None`) → note written, title unchanged.
  - Retitle failure is non-fatal: notes still count as written.
- `title_is_default` lifecycle: set on `fresh_recording_core`; cleared by
  `rename_local_recording`.

Run the suite with the repo's macOS workaround:
```bash
DYLD_LIBRARY_PATH="/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift-5.5/macosx" \
  cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1
```

### Sidecar (manual)

The Swift package has no test target (consistent with the repo). Verify by rebuilding the
binary and running `notes` on a sample transcript, confirming stdout is
`{ "title": …, "notes": … }` with a sensible, sanitized title.

### End-to-end (manual)

Record a short meeting in offline mode; after notes generate, confirm the recording's title
changes from the timestamp default to a generated title and the vault `.md` file is renamed.
Rename a recording first, then (re)generate notes, and confirm the user's title is preserved.

## Non-goals

- No title generation for cloud (Ariso) mode.
- No re-titling of recordings the user has already renamed.
- No repeated re-titling once a generated (or user) title is in place.
- No new user-facing setting or UI to toggle this behavior.
