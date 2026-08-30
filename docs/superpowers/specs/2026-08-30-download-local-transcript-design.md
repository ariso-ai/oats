# Download local transcripts (issue #337)

## Problem

Once a local recording finishes transcribing, its transcript only exists two
places: rendered read-only inside the Transcript tab of `MeetingDetailView`,
and as `transcript.md` under `~/.ariso`. There is no user-facing way to get a
copy of it out of the app. The closest thing in the codebase is
`open_recording_file` (`src-tauri/src/commands.rs:1934`) / its frontend
wrapper `local.openRecordingFile` (`src/tauri.ts:418`), which opens a
recording's `transcript.md` in the OS default app — but nothing in the UI
calls it (confirmed: no `.vue` file references `openRecordingFile`). It's a
backend primitive nobody wired up, and even wired up it wouldn't be a
"download": it opens the file in place inside `~/.ariso`, not a copy the user
picks a destination for. Today the only way to get transcript text out is to
open the Transcript tab and manually select/copy the rendered text.

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
  itself in `availableTabs`).
- Writes the transcript to a location the user picks via the native OS save
  dialog (`@tauri-apps/plugin-dialog`'s `save()`), as a `.md` file containing
  the same speaker-attributed transcript markdown already rendered in the tab.

Reviewable as: open a local recording whose transcription hasn't finished —
the Download button is present but disabled with a tooltip explaining why.
Open one with a finished transcript, click Download, save to Desktop; the
resulting `.md` file's content matches what the Transcript tab renders.

## Non-goals

- **AI notes / personal notes.** This is transcript-only. Exporting notes
  alongside the transcript is the scope of the deferred #306 spec, not this
  one — implementing this narrower feature first doesn't block that one.
- **Ariso (cloud) meetings.** Local recordings only; see
  [Cloud vs offline](#cloud-vs-offline).
- **Other formats** (PDF, DOCX, plain `.txt` stripped of markdown). Markdown
  only, matching how the transcript is already stored and rendered.
- **Bulk/multi-meeting download.**
- **Wiring up `open_recording_file`.** It solves a different problem
  (open-in-place) and isn't needed once a real save-to-disk path exists.

## Design

### Composing the document

`src/views/meetingShareText.ts` gets a new pure function alongside
`composeLocalShareText`:

```ts
export function composeLocalTranscriptText(
  detail: Pick<MeetingDetail, 'title' | 'startAt'>,
  transcript: string // pre-stripped of front-matter by the caller
): string
```

Same heading logic as `composeLocalShareText` (`# {title}` / formatted date),
followed by a `## Transcript` section with the transcript body. Returns `''`
when `transcript` is blank, mirroring `composeLocalShareText`'s empty-input
contract. The caller passes `MeetingDetailView.vue`'s existing
`transcriptMarkdown` computed (`stripFrontmatter(detail.transcript)`,
`MeetingDetailView.vue:1109`) — already frontmatter-stripped for rendering the
tab, so there's no second stripping code path to maintain.

### UI: a Download button in the Transcript tab

Add a small header row at the top of the `activeTab === 'transcript'`
tab-pane (`MeetingDetailView.vue`, around line 279, above the existing
`card-audio` block), shown only when `detail.isLocal`:

- A "Download transcript" button, `:disabled="!detail.hasTranscript"`,
  `:title="detail.hasTranscript ? 'Download transcript' : 'Transcript not ready yet'"` —
  the same disabled/tooltip pattern the tab list already uses
  (`availableTabs` sets `disabled: !d.hasTranscript` for the tab itself).
- An inline error message (`transcriptDownloadError` ref) shown below the
  button on write failure, styled like the existing `shareError` /
  `clipDeleteError` inline messages — this app has no global toast system.

This is a plain button, not a menu — there's no option to choose (unlike the
deferred export spec's transcript-include checkbox), so an anchored dropdown
would be overhead the feature doesn't need.

### Download flow

`onDownloadTranscriptClick()`:

1. Guard: return if `!detail.value?.hasTranscript` or `!transcriptMarkdown.value`
   (defensive; the disabled button should make this unreachable).
2. `composeLocalTranscriptText(detail.value, transcriptMarkdown.value)`. If
   `''`, stop.
3. Build a suggested filename: `sanitizeFilename(detail.title || 'Meeting') + ' transcript - ' + isoDateStamp(detail.startAt) + '.md'`
   — small local helpers (strip `/ \ : * ? " < > |` and trim; format the date
   as `YYYY-MM-DD`), not a new Rust dependency.
4. `pickMarkdownSavePath(defaultName)` — new `src/tauri.ts` wrapper around
   `save({ defaultPath: defaultName, filters: [{ name: 'Markdown', extensions: ['md'] }] })`.
   Returns the chosen absolute path, or `null` if the user canceled — same
   shape as the existing `pickVaultFolder` wrapper (`src/tauri.ts:557`).
5. If `null`, stop (cancel is silent).
6. `writeMarkdownExport(path, contents)` — new `src/tauri.ts` wrapper for a
   new Rust command (below). On success, clear any prior error. On failure,
   set `transcriptDownloadError`.

### Backend: a small write command, not the fs plugin

One new `#[tauri::command]` in `commands.rs`:

```rust
#[tauri::command]
pub fn write_markdown_export(path: String, contents: String) -> Result<(), String> {
    if !std::path::Path::new(&path).is_absolute() {
        return Err("Export path must be absolute.".to_string());
    }
    std::fs::write(&path, contents).map_err(|e| format!("write export file: {e}"))
}
```

Registered in `main.rs`'s `generate_handler![...]`. Like the rest of
`commands::*`, it needs no capability entry itself — only plugin-provided
commands do.

This uses a purpose-built command instead of `@tauri-apps/plugin-fs`, whose
write permission is scope-based (an allowlist of directories); the save
dialog can legitimately point anywhere on disk, so a useful fs-plugin scope
here would have to be a wildcard — a much bigger grant than one command doing
exactly one `fs::write` to a caller-supplied path. `set_vault_dir`
(`commands.rs:1109`) already sets this precedent: it takes an arbitrary
user-picked absolute path and only checks it's absolute.

**Security note (per `oats-security`):** `write_markdown_export` is an
arbitrary-absolute-path-write primitive reachable from any window that can
reach the `library` capability set, same class of risk as `set_vault_dir`. The
mitigating factor is the same: the path normally comes from a native save
dialog the user drove, not from meeting content. No secrets or app-internal
data pass through it — just the transcript markdown already on screen.

**Naming note:** this command and the `dialog:allow-save` capability below are
exactly what the deferred #306 export spec also needs — both are "write text
the user composed to a path they picked via the save dialog," independent of
whether the text is a transcript or a full notes+transcript document.
Whichever feature lands first should build these two pieces; the other should
reuse them rather than add a second command/capability file. Naming the
command `write_markdown_export` (not `export_meeting_transcript`) reflects
that shared, content-agnostic shape.

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
meetings get nothing here. Ariso transcripts are structured `TranscriptChunk[]`
loaded lazily via `Backend.getMeetingTranscript`, not markdown on disk — a
cloud version would need its own chunk-to-markdown formatter (the Transcript
tab's `<ol class="transcript">` fallback already renders chunks, but not as
one composable markdown string) before it could reuse
`composeLocalTranscriptText`. Not designed here; the issue itself scopes to
"recordings stored locally."

Nothing in this feature makes a network call — it's a local read of
already-loaded `detail` state plus a local file write — so it's compatible
with the offline-mode privacy guarantee by construction.

## Error handling

- **Dialog canceled** (`save()` resolves `null`): silent no-op.
- **Write failure** (permission denied, disk full, target directory removed
  between picking and writing): `write_markdown_export` returns `Err`, caught
  in `onDownloadTranscriptClick` and shown as `transcriptDownloadError`
  ("Couldn't save the transcript: {message}") below the Download button.
- **Transcript not ready**: prevented by the disabled button + tooltip; the
  guard in step 1 of the download flow stays as defense in depth.

## Testing

- **`meetingShareText.test.ts`**: `composeLocalTranscriptText` — title + date
  heading + transcript body when transcript is present; falls back to the
  default title exactly like `composeLocalShareText`; returns `''` for a
  blank/whitespace-only transcript.
- **`MeetingDetailView.test.ts`**: Download button rendered only when
  `detail.isLocal`; disabled with the "not ready" tooltip when
  `!detail.hasTranscript`; click composes the text and calls
  `pickMarkdownSavePath` then `writeMarkdownExport`; a canceled dialog
  (`null` path) means `writeMarkdownExport` is never called; a rejected
  `writeMarkdownExport` surfaces `transcriptDownloadError` and leaves the tab
  usable.
- **Rust (`commands.rs`)**: `write_markdown_export` writes the given contents
  to the given path and returns `Ok(())`; rejects a relative path without
  touching the filesystem (mirrors `set_vault_dir_rejects_relative_path`);
  surfaces the underlying `io::Error` message on a write failure (e.g. a
  parent directory that doesn't exist).
- **Manual**: record locally, wait for transcription, download to Desktop,
  confirm the `.md` file opens with the same speaker-attributed content shown
  in the Transcript tab. Also confirm the disabled state and tooltip while a
  recording is still transcribing.

## Open questions

- **Button placement.** This spec puts Download inside the Transcript tab
  itself (only relevant there) rather than the header next to Share. Confirm
  that's preferred over a header-level action that's always visible but only
  enabled on the Transcript tab.
- **Filename convention.** Proposed `{title} transcript - {YYYY-MM-DD}.md`.
  Confirm the wording and date format, especially since #306 (if built later)
  will want a consistent convention for its own default filename.
