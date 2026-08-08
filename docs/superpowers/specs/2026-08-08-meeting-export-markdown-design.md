# Export meeting notes as Markdown (issue #306)

## Problem

The only way to get a local recording's notes out of oats today is to select the
rendered note in the AI Notes tab and copy-paste. That loses the heading
structure and drops the transcript entirely — there's no way to get the
transcript out of the app at all short of reading `~/.ariso`'s vault files
directly. Someone who keeps their own notes system (the reporter uses Obsidian)
wants a clean, structured `.md` file per meeting they can drop straight into
that system.

The codebase already builds a markdown document like this once — `composeLocalShareText`
(`src/views/meetingShareText.ts`) — but only to feed the native macOS share sheet
(`shareTextNative`, `MeetingDetailView.vue`'s Share button), and it never includes
the transcript.

## Goal

A local recording's detail view gets an **Export** action that:

- Composes a markdown document: title + date heading, AI Notes, My Notes (personal
  note), and — only when the user opts in via a checkbox that **defaults off** —
  a Transcript section.
- Writes it to a location the user picks via the **native OS save dialog**
  (`@tauri-apps/plugin-dialog`'s `save()`), not the share sheet.
- Works cross-platform (macOS and Windows), since `plugin-dialog` is
  cross-platform — unlike the share sheet, which is macOS-only
  (`NSSharingServicePicker`).

Reviewable as: open a local recording with AI notes, a personal note, and a
transcript; click Export; leave the transcript checkbox off; save; the `.md`
file has title/date/AI Notes/My Notes and no transcript. Repeat with the
checkbox on; the file also has a `## Transcript` section with the
speaker-attributed transcript body.

## Non-goals

- **Ariso (cloud) meetings.** Explicitly deferred by shawnzhu ("keep it to local
  recordings for a first cut — cloud can follow once the shape is proven"). The
  Export button is local-only. See [Cloud vs offline](#cloud-vs-offline) for why
  cloud isn't a drop-in extension later.
- **Share-sheet integration.** shawnzhu was explicit: save-to-disk via the native
  save dialog, not the share sheet. The existing Share button and
  `composeLocalShareText` are untouched.
- Other export formats (PDF, DOCX, HTML). Markdown only.
- Bulk/multi-meeting export.
- Exporting audio alongside the markdown.
- A user-facing setting to change the default transcript-checkbox state — it's
  always off on open, per the trusted comment.

## Design

### Composing the document

`src/views/meetingShareText.ts` gets a new pure function alongside the existing
`composeLocalShareText`, rather than modifying that one (the share sheet's
document shape is a settled, tested feature and shouldn't shift because export
exists):

```ts
export function composeLocalExportText(
  detail: Pick<MeetingDetail, 'title' | 'startAt' | 'note'>,
  personalNote: string,
  transcript: string | null // pre-stripped of front-matter by the caller
): string
```

Same heading logic as `composeLocalShareText` (`# {title}` / formatted date /
`## AI Notes` / `## My Notes`, empty sections omitted), plus a `## Transcript`
section appended when `transcript` is non-null and non-blank. Returns `''` when
there is nothing to export at all (mirrors `composeLocalShareText`'s empty-input
contract).

The function takes an already-stripped transcript string rather than importing
`stripFrontmatter` itself: `MeetingDetailView.vue` already computes
`transcriptMarkdown` (`stripFrontmatter(detail.transcript)`, used to render the
Transcript tab) — the export flow reuses that computed value instead of a second
frontmatter-stripping code path. `detail.note` is passed through unstripped, same
as `composeLocalShareText` does today: vault notes read through
`read_recording_file` are already frontmatter-free server-side (`vault::read_note`
returns `note_body(&contents)`); only `transcript.md` legitimately carries
front-matter (`storage::render_transcript_markdown`), which is why only the
transcript needs stripping.

### UI: a new Export button + anchored menu

`MeetingDetailView.vue`'s header (`.head-actions`) gets a new **Export** button
next to Share, shown only for local recordings (`v-if="detail.isLocal"`), with a
small anchored dropdown menu — following the same pattern already used for the
Attendees dropdown (`showAttendees` / `attendeesMenuStyle` / overlay div /
Escape-to-close), not a new popover component like `ShareMeetingPopover.vue`.
`ShareMeetingPopover` earns its own file because it owns non-trivial state
(visibility levels, email invites, expiry); this menu is one checkbox and one
button, so an inline anchored `<div>` block matches the codebase's existing
proportion-to-complexity precedent.

Menu contents:

- A checkbox, label "Include transcript", **bound to a ref that resets to
  `false` every time the menu opens** (per the trusted comment: defaults off).
- An "Export…" button.

Button disabled state: the Export button itself is disabled when the recording
has neither a note nor a transcript yet (`!detail.note && !detail.transcript`) —
the same signal `availableTabs` already uses to grey out the AI Notes / Transcript
tabs before generation finishes. This is slightly more informative than the
existing Share button, which silently no-ops on an empty document; worth a
second look — see [Open questions](#open-questions).

### Export flow

`onExportClick()` in `MeetingDetailView.vue`, invoked from the menu's "Export…"
button:

1. Load the personal note the same way `shareLocal` already does:
   `notesPersistence.load(props.item)`, falling back to `''` on failure
   (non-fatal, matching the existing share flow).
2. `composeLocalExportText(detail, personalNote, includeTranscript.value ? transcriptMarkdown.value : null)`.
   If the result is `''`, stop (the disabled-button guard above should make this
   unreachable in practice, but the function stays defensive).
3. Build a suggested filename: `sanitizeFilename(detail.title || 'Meeting notes')
   + ' - ' + isoDateStamp(detail.startAt) + '.md'` — a small local helper (strip
   `/ \ : * ? " < > |` and trim), not a new Rust dependency.
4. `pickMarkdownSavePath(defaultName)` — new `src/tauri.ts` wrapper around
   `@tauri-apps/plugin-dialog`'s `save({ defaultPath: defaultName, filters: [{
   name: 'Markdown', extensions: ['md'] }] })`. Returns the chosen absolute path,
   or `null` if the user canceled — mirrors the existing `pickVaultFolder`
   wrapper's shape (`open(...)` → typed `string | null`).
5. If `null`, stop (cancel is silent, standard save-dialog behavior).
6. `writeExportFile(path, text)` — new `src/tauri.ts` wrapper for a new Rust
   command (below). On success, close the menu. On failure, show an inline error
   in the menu (`exportError` ref), styled like the existing `shareError` /
   `clipDeleteError` inline messages — no global toast system exists in this app.

### Backend: a small write command, not the fs plugin

Writing the chosen path needs one new `#[tauri::command]` in `commands.rs`:

```rust
#[tauri::command]
pub fn export_meeting_markdown(path: String, contents: String) -> Result<(), String> {
    if !std::path::Path::new(&path).is_absolute() {
        return Err("Export path must be absolute.".to_string());
    }
    std::fs::write(&path, contents).map_err(|e| format!("write export file: {e}"))
}
```

Registered in `main.rs`'s `generate_handler![...]` alongside `commands::set_vault_dir`
/ `commands::read_recording_file`. Like `share_text_native` and the rest of
`commands::*`, a custom app command needs **no** capability entry — only
plugin-provided commands do.

This uses a purpose-built command instead of adding `@tauri-apps/plugin-fs`. The
fs plugin's write permission is scope-based (an allowlist of directories); the
save dialog can legitimately point anywhere on disk the user has write access to,
so a useful fs-plugin scope for this would have to be a wildcard — a much bigger
permission grant than one command that does exactly one `fs::write` to a caller-
supplied path. The precedent for this trade-off already exists in
`set_vault_dir`, which takes an arbitrary user-picked absolute directory and only
checks it's absolute before using it — same level of trust, same validation.

**Security note (flagging per `oats-security`):** `export_meeting_markdown` is
technically an arbitrary-absolute-path-write primitive reachable from any window
that can reach the `library` capability set, same as `set_vault_dir` already is.
The mitigating factor is the same as `set_vault_dir`'s: the path normally comes
from a native OS dialog the user drove, not from meeting content. No secrets or
app-internal data flow through this path — it's just the composed markdown the
user already sees on screen.

### Capability: the Meetings window needs the save dialog

`plugin-dialog`'s `save` command is capability-gated like `open` already is.
Today only `src-tauri/capabilities/settings.json` grants `dialog:allow-open`,
scoped to the `settings` window (used by `pickVaultFolder`). The `library`
window — where `MeetingDetailView` lives — has no dialog permission yet, so this
needs a **new capability file**, following the existing one-file-per-concern
convention (`library-window-controls.json`):

```json
// src-tauri/capabilities/library-export-dialog.json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "library-export-dialog",
  "description": "Allow the Meetings window to open the native save dialog for exporting a meeting to Markdown",
  "windows": ["library"],
  "permissions": ["dialog:allow-save"]
}
```

This is the one genuinely new permission surface in this feature — flagged
explicitly per `oats-security`'s capabilities checklist. It's narrowly scoped
(one window, one dialog verb) and mirrors the existing `dialog:allow-open` grant
pattern exactly.

## Cloud vs offline

**Local only, for now.** The Export button is gated on `detail.isLocal`; Ariso
meetings get nothing in this change.

Cloud isn't a trivial follow-on for two reasons worth flagging up front:

- **Transcript shape differs.** Local transcripts are already rendered markdown
  on disk (`detail.transcript`, read once as part of `getMeetingDetail`). Ariso
  transcripts are structured `TranscriptChunk[]` loaded lazily via
  `Backend.getMeetingTranscript`, requiring a markdown formatter (timestamp +
  speaker + content, similar to what the Transcript tab's `<ol class="transcript">`
  fallback already renders) before they can be appended to an export document.
- **Note content differs.** Ariso's "AI Notes" tab is composed of several fields
  (`digest`, `actionItems`, `summary`) rather than one `note` string — the export
  composer would need its own Ariso-specific section builder, not a reuse of
  `composeLocalExportText`.

Neither is a large lift, but both are real design decisions (what does an Ariso
export look like — digest + action items + summary, one merged section?) that
shawnzhu's comment explicitly deferred rather than asked to be pre-solved here.

Nothing in this feature makes a network call — it's a pure local read (already-
loaded `detail` fields) + a local file write — so it's compatible with the
offline-mode privacy guarantee by construction.

## Error handling

- **Dialog canceled** (`save()` resolves `null`): silent no-op, standard
  cross-platform save-dialog behavior.
- **Write failure** (permission denied, disk full, target directory removed
  between picking and writing): `export_meeting_markdown` returns `Err`, the
  `invoke` rejects, caught in `onExportClick` and shown as an inline message in
  the export menu ("Couldn't save the file: {message}") — the menu stays open so
  the user can retry or pick a different location.
- **Personal note load failure**: falls back to `''`, non-fatal — identical to
  the existing `shareLocal` behavior, not a new failure mode.
- **Nothing to export**: prevented by the button's disabled state
  (no note and no transcript); `composeLocalExportText` still defensively
  returns `''` rather than writing an empty file.

## Testing

- **`meetingShareText.test.ts`**: `composeLocalExportText` — AI+personal+transcript
  all present; transcript omitted when the flag/string is `null` even though
  content exists; each section independently omitted when blank; falls back to
  the default title exactly like `composeLocalShareText`; returns `''` when
  everything is empty.
- **`MeetingDetailView.test.ts`**: Export button rendered only when
  `detail.isLocal`; disabled when there's no note and no transcript; menu open
  toggles and the transcript checkbox starts unchecked on every open (including
  re-opening after checking it once); Export click composes the right text based
  on checkbox state and calls `pickMarkdownSavePath` then `writeExportFile`;
  canceled dialog (`null` path) means `writeExportFile` is never called; a
  rejected `writeExportFile` surfaces the inline error and leaves the menu open.
- **Rust (`commands.rs`)**: `export_meeting_markdown` writes the given contents to
  the given path and returns `Ok(())`; rejects a relative path without touching
  the filesystem (mirrors the existing `set_vault_dir_rejects_relative_path`
  test); surfaces the underlying `io::Error` message on a write failure (e.g. a
  parent directory that doesn't exist).
- **Manual**: run the save flow on both a macOS and a Windows build (the save
  dialog is the cross-platform piece this feature specifically relies on, unlike
  the macOS-only share sheet); open the resulting `.md` in Obsidian and confirm
  headings, the AI/My Notes split, and the speaker-attributed transcript lines
  render as expected.

## Open questions

- **Button placement.** This spec puts Export in the header next to Share (two
  buttons: Share, Export, Close). The issue only said "somewhere in the meeting
  detail view" — confirm the header is the right home versus, say, folding it
  into a menu if more per-meeting actions are expected soon.
- **Disabled vs. silent no-op.** This spec disables the Export button when
  there's nothing to export, which is a small UX deviation from the existing
  Share button's silent no-op on empty content. Confirm that's wanted, or drop it
  for consistency with Share.
- **Default filename convention.** Proposed `{title} - {YYYY-MM-DD}.md`. Confirm
  the date format and separator match what would be useful in a tool like
  Obsidian (e.g. sortable prefix `{YYYY-MM-DD} {title}.md` instead).
