# Download local transcripts (issue #337)

## Problem

Once a local recording finishes transcribing, its transcript already exists as
a finished markdown file on disk: `<vault>/.oats/recordings/<id>/transcript.md`
(`vault::meta_root()` is `<vault>/.oats`, `vault.rs:47`; `storage.rs:190`).
It's rendered read-only inside the Transcript tab of `MeetingDetailView`, but
there is no user-facing way to get a copy of it out of the app.

The closest thing in the codebase is `open_recording_file`
(`src-tauri/src/commands.rs:1935`) / its frontend wrapper
`local.openRecordingFile` (`src/tauri.ts:418`), which opens that same
`transcript.md` in the OS default app — but nothing in the UI calls it
(confirmed: no `.vue` file references `openRecordingFile`). It's a backend
primitive nobody wired up, and even wired up it wouldn't be a "download": it
opens the file in place inside the vault, not a copy the user picks a
destination for. Today the only way to get transcript text out is to open the
Transcript tab and manually select/copy the rendered text.

This is the same gap the deferred `2026-08-08-meeting-export-markdown-design.md`
spec (issue #306) described for meeting notes, but scoped to transcripts only,
per this issue's explicit ask ("Download transcripts") rather than the fuller
notes+transcript export that spec designed. That spec hasn't been built yet.

## Goal

A local recording's Transcript tab gets a **Download** action that:

- Is visible only for local recordings once the tab itself is reachable
  (`detail.isLocal`).
- Is disabled — with an explanatory tooltip — until the transcript exists
  (`detail.hasTranscript`, the same flag that already gates the Transcript tab
  itself in `availableTabs`, `MeetingDetailView.vue:1161`).
- Copies the recording's existing `transcript.md` to a location the user picks
  via the native OS save dialog (`@tauri-apps/plugin-dialog`'s `save()`).

**The central design decision is that this is a file copy, not a document
build.** `transcript.md` is already a complete, well-formed markdown document
— the app writes it that way (`storage.rs:405-440`). Nothing needs composing,
so nothing here reads, transforms, or re-serializes transcript text; the bytes
the user gets are byte-identical to the bytes in the vault.

Reviewable as: open a local recording whose transcription hasn't finished —
the Download button is present but disabled with a tooltip explaining why.
Open one with a finished transcript, click Download, save to Desktop; the
resulting `.md` file is byte-identical to
`<vault>/.oats/recordings/<id>/transcript.md`.

## Non-goals

- **AI notes / personal notes.** This is transcript-only. Exporting notes
  alongside the transcript is the scope of the deferred #306 spec, not this
  one — implementing this narrower feature first doesn't block that one.
- **Ariso (cloud) meetings.** Local recordings only; see
  [Cloud vs offline](#cloud-vs-offline).
- **Other formats** (PDF, DOCX, plain `.txt` stripped of markdown). Markdown
  only, matching how the transcript is already stored.
- **Reformatting the transcript on the way out.** See
  [What the copied file contains](#what-the-copied-file-contains).
- **Bulk/multi-meeting download.**
- **Wiring up `open_recording_file`.** It solves a different problem
  (open-in-place) and isn't needed once a real save-to-disk path exists.

## What the copied file contains

`transcript.md` on disk is YAML front-matter followed by the
speaker-attributed body (`storage.rs:423-434`):

```markdown
---
title: "Weekly sync"
date: "2026-08-28T10:00:00Z"
duration: "00:32:10"
participants: ["Shawn", "Speaker 2"]
---

**Shawn** [0:00]
Let's start with the roadmap.

**Speaker 2** [0:14]
...
```

**Decision: keep the front-matter.** oats writes an Obsidian vault, so
front-matter is the app's native metadata format, and it carries title, date,
duration and participants that would otherwise be lost in a bare file. Keeping
it is also what makes the copy a copy — the promise "you get exactly what's in
your vault" is easy to keep and impossible to get subtly wrong.

Note this differs from what the Transcript tab shows: the tab renders
`stripFrontmatter(detail.transcript)` (`MeetingDetailView.vue:1109-1113`), so
the downloaded file has a metadata block the on-screen view doesn't. That's the
accepted cost of the copy.

**The alternative, for the record:** if the export must be front-matter-free
with an `# Title` heading, a copy can't produce it, and the design changes
shape — a `composeLocalTranscriptText` helper in `src/views/meetingShareText.ts`
alongside `composeLocalShareText`, fed the already-stripped
`transcriptMarkdown` computed, plus a content-agnostic
`write_markdown_export(path, contents)` command instead of the copy command
below. That's roughly three extra units of code (compose helper + its unit
tests, an extra IPC wrapper, a write command) and a wider security grant
(see [Backend](#backend-copy-the-file-dont-write-arbitrary-content)) in
exchange for a slightly prettier file. Not worth it unless the front-matter is
actually unwanted.

## Design

### UI: a Download button in the Transcript tab

Add a small header row at the top of the `activeTab === 'transcript'`
tab-pane (`MeetingDetailView.vue:287`, above the existing `card-audio` block),
shown only when `detail.isLocal`:

- A "Download transcript" button, `:disabled="!detail.hasTranscript"`,
  `:title="detail.hasTranscript ? 'Download transcript' : 'Transcript not ready yet'"` —
  the same disabled/tooltip pattern the tab list already uses
  (`availableTabs` sets `disabled: !d.hasTranscript` for the tab itself).
- An inline error message (`transcriptDownloadError` ref) shown below the
  button on failure, styled like the existing `clipDeleteError` inline message
  (`MeetingDetailView.vue:324`) — this app has no global toast system.

This is a plain button, not a menu — there's no option to choose (unlike the
deferred export spec's transcript-include checkbox), so an anchored dropdown
would be overhead the feature doesn't need.

### Download flow

`onDownloadTranscriptClick()`:

1. Guard: return if `!detail.value?.isLocal || !detail.value.hasTranscript`
   (defensive; the disabled button should make this unreachable).
2. Build a suggested filename:
   `sanitizeFilename(detail.title || 'Meeting') + ' transcript - ' + isoDateStamp(detail.startAt) + '.md'`
   — small local helpers (strip `/ \ : * ? " < > |` and trim; format the date
   as `YYYY-MM-DD`), not a new Rust dependency.
3. `pickMarkdownSavePath(defaultName)` — new `src/tauri.ts` wrapper around
   `save({ defaultPath: defaultName, filters: [{ name: 'Markdown', extensions: ['md'] }] })`.
   Returns the chosen absolute path, or `null` if the user canceled — same
   shape as the existing `pickVaultFolder` wrapper (`src/tauri.ts:557`).
4. If `null`, stop (cancel is silent).
5. `local.copyRecordingFile(detail.id, 'transcript', path)` — new wrapper next
   to `openRecordingFile`/`readRecordingFile` in the `local` object
   (`src/tauri.ts:418-425`). `detail.id` is the recording folder id for local
   recordings (`useBackend.ts:534`). On success, clear any prior error. On
   failure, set `transcriptDownloadError`.

No transcript text crosses the IPC boundary in this flow.

### Backend: copy the file, don't write arbitrary content

One new `#[tauri::command]` in `commands.rs`, a direct sibling of
`open_recording_file` — open-in-place vs. save-a-copy — reusing the same three
validated helpers it uses:

```rust
/// Copy a recording's `ari-note.md` or `transcript.md` to a user-picked path.
/// `kind` must be `"note"` or `"transcript"`; `dest` must be absolute.
#[tauri::command]
pub fn copy_recording_file(id: String, kind: String, dest: String) -> Result<(), String> {
    crate::storage::validate_recording_id(&id)?;
    if !std::path::Path::new(&dest).is_absolute() {
        return Err("Destination path must be absolute.".to_string());
    }
    let src = recording_dir(&id)?.join(note_or_transcript_filename(&kind)?);
    if !src.exists() {
        return Err(format!("recording file not found: {}", src.display()));
    }
    std::fs::copy(&src, &dest)
        .map(|_| ())
        .map_err(|e| format!("copy export file: {e}"))
}
```

Registered in `main.rs`'s `generate_handler![...]`. Like the rest of
`commands::*`, it needs no capability entry itself — only plugin-provided
commands do.

This uses a purpose-built command instead of `@tauri-apps/plugin-fs`, whose
write permission is scope-based (an allowlist of directories); the save dialog
can legitimately point anywhere on disk, so a useful fs-plugin scope here would
have to be a wildcard — a much bigger grant than one command doing exactly one
`fs::copy`. `set_vault_dir` (`commands.rs:1109`) already sets the precedent for
taking a user-picked absolute path and only checking that it's absolute.

**Security note (per `oats-security`):** `copy_recording_file` can write to any
absolute path the `library` window names, so it can clobber an existing file —
the mitigation is that the path normally comes from a native save dialog the
user drove, and the dialog itself prompts on overwrite. What it deliberately
does *not* expose is arbitrary content: the bytes are always a `transcript.md`
or `ari-note.md` inside the vault, selected by a `validate_recording_id`-checked
id plus a two-value `kind` enum (`commands.rs:1700-1706`). A caller controls
*where* bytes land but never *what* they are, which is a strictly smaller grant
than a `write_markdown_export(path, contents)` primitive would be.

**Note for #306:** this command covers "save one existing artifact file
elsewhere" and nothing more. The deferred notes+transcript export composes a
*new* document that doesn't exist on disk, so it will still need its own
`write_markdown_export(path, contents)` command when it's built — about six
lines. Building that now, speculatively, for a spec that hasn't shipped isn't
worth the wider write surface in the meantime. The `dialog:allow-save`
capability below *is* shared: #306 should reuse it rather than add a second
capability file.

### Capability: the Meetings window needs the save dialog

`plugin-dialog`'s `save` command is capability-gated. Today only
`src-tauri/capabilities/settings.json` grants `dialog:allow-open`, scoped to
the `settings` window (used by `pickVaultFolder`). The `library` window —
where `MeetingDetailView` lives — has no dialog permission yet, so this needs
a new capability file, following the existing one-file-per-concern convention:

```json
// src-tauri/capabilities/library-export-dialog.json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "library-export-dialog",
  "description": "Allow the Meetings window to open the native save dialog for downloading a transcript",
  "windows": ["library"],
  "permissions": ["dialog:allow-save"]
}
```

This is the one genuinely new permission surface in this feature — flagged
per `oats-security`'s capabilities checklist. Narrowly scoped (one window,
one dialog verb), mirroring the existing `dialog:allow-open` grant.

## Cloud vs offline

**Local only.** The Download button is gated on `detail.isLocal`; Ariso
meetings get nothing here. The whole design rests on there being a file to
copy, and Ariso transcripts are structured `TranscriptChunk[]` loaded lazily
via `Backend.getMeetingTranscript`, not markdown on disk. A cloud version would
need its own chunk-to-markdown formatter and a content-write command — i.e.
the compose-and-write shape this spec declined — so it is a genuinely different
feature, not an extension of this one. Not designed here; the issue itself
scopes to "recordings stored locally."

Nothing in this feature makes a network call — it's a local file copy driven by
already-loaded `detail` state — so it's compatible with the offline-mode
privacy guarantee by construction.

## Error handling

- **Dialog canceled** (`save()` resolves `null`): silent no-op.
- **Copy failure** (permission denied, disk full, target directory removed
  between picking and writing, source file deleted mid-flight):
  `copy_recording_file` returns `Err`, caught in `onDownloadTranscriptClick`
  and shown as `transcriptDownloadError` ("Couldn't save the transcript:
  {message}") below the Download button.
- **Transcript not ready**: prevented by the disabled button + tooltip; the
  `!exists` check in the command is defense in depth, and returns the same
  "recording file not found" message `open_recording_file` already uses.

## Testing

- **`MeetingDetailView.test.ts`**: Download button rendered only when
  `detail.isLocal`; disabled with the "not ready" tooltip when
  `!detail.hasTranscript`; click calls `pickMarkdownSavePath` then
  `copyRecordingFile` with the recording id, `'transcript'`, and the picked
  path; a canceled dialog (`null` path) means `copyRecordingFile` is never
  called; a rejected `copyRecordingFile` surfaces `transcriptDownloadError`
  and leaves the tab usable.
- **Rust (`commands.rs`)**: `copy_recording_file` copies `transcript.md` to the
  destination with identical bytes and returns `Ok(())`; rejects a relative
  destination without touching the filesystem (mirrors
  `set_vault_dir_rejects_relative_path`); returns "recording file not found"
  when the transcript hasn't been generated. Traversal ids and unknown kinds
  are already covered by `recording_dir_rejects_traversal_ids` and
  `note_or_transcript_filename_rejects_unknown_kind`
  (`commands.rs:2614-2635`), which this command reuses verbatim.
- **Manual**: record locally, wait for transcription, download to Desktop,
  confirm the `.md` file matches
  `<vault>/.oats/recordings/<id>/transcript.md` (`diff` them) and opens
  cleanly in Obsidian with its metadata intact. Also confirm the disabled state
  and tooltip while a recording is still transcribing.

## Open questions

- **Front-matter.** This spec keeps it, which makes the feature a plain file
  copy; see [What the copied file contains](#what-the-copied-file-contains).
  Confirm — this is the one answer that changes the design's shape.
- **Button placement.** This spec puts Download inside the Transcript tab
  itself (only relevant there) rather than the header next to Share, where it
  would be visible but disabled on three of four tabs. Confirm.
- **Filename convention.** Proposed `{title} transcript - {YYYY-MM-DD}.md`.
  Low-stakes (the save dialog lets the user rename before saving), but worth
  settling since #306 will want a consistent default.
