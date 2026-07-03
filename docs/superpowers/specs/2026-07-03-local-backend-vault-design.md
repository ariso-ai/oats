# Local backend Obsidian vault — design

**Date:** 2026-07-03
**Branch:** `feat/local-backend-vault`
**Status:** Design (pending implementation plan)

## Overview

Store the AI notes and audio for **local-backend** meetings in an Obsidian-compatible
vault at `~/.ariso/vault`, so users can open, read, and edit their meeting notes in
Obsidian (or any editor) as first-class markdown. The vault is the **canonical, single
copy** of the note and its audio — not a mirror of a copy held elsewhere. oats writes each
note exactly once (at notes-generation time) and never rewrites the body afterward, which
is what makes shared read/write with Obsidian safe without reconciliation.

This is a local/offline-backend feature only. Cloud-backend meetings (stored on Ariso
servers) are untouched.

## Motivation

oats' local store is already markdown-first (`transcript.md` and `ari-note.md` with YAML
front-matter, plain files, no database). Obsidian's vault model is the same philosophy:
a plain folder of markdown + attachments, with a rebuildable metadata cache rather than a
source-of-truth database. Adopting it gives users genuine data ownership and interop
(Obsidian graph/backlinks/plugins) without oats depending on Obsidian being installed.

## Decisions (locked during brainstorming)

1. **Local backend only.** Cloud meetings are out of scope.
2. **Always-on.** Every local recording writes a note + audio attachment to the vault; no
   settings toggle in v1.
3. **Fixed vault location:** `~/.ariso/vault`. A user-configurable path is deferred.
4. **oats owns the vault.** oats bootstraps a minimal `.obsidian/` so the folder opens
   cleanly in Obsidian.
5. **Split of truth:**
   - **Canonical in the vault:** the note (`<meeting>.md`) and audio
     (`Attachments/<meeting>.mp3`). Each written once.
   - **Canonical in `~/.ariso/recordings/<id>/`:** `meta.json`, `segments.json`,
     `transcript.md` — unchanged, oats-private machine state.
6. **Link by `oats_id`.** The note's front-matter carries `oats_id` equal to the recording
   folder id. oats locates a note by scanning front-matter, so renames/moves in Obsidian
   never break the link (path-independent).
7. **Respect deletions; no mirror.** If the user deletes the vault note, it stays deleted.
   A `notes_written` timestamp in `meta.json` records that oats already generated the note;
   its presence + an absent vault note ⇒ user deletion ⇒ never regenerate. That flag is the
   tombstone (no separate file).
8. **Cascade delete.** Deleting a recording inside oats removes its vault note + audio
   attachment as well as the private `~/.ariso` folder.
9. **Migration = fallback (option B).** Existing local recordings are not migrated. Reads
   check the vault first (by `oats_id`) and fall back to the legacy
   `~/.ariso/recordings/<id>/ari-note.md` and `recording.mp3`. Only new recordings use the
   vault.

## On-disk layout

```
~/.ariso/vault/                              # the Obsidian vault (fixed for now)
  .obsidian/
    app.json                                 # minimal; attachmentFolderPath: "Attachments"
  Attachments/
    2026-06-02 Team Standup.mp3              # audio — canonical, lives only here
  2026-06-02 Team Standup.md                 # note — canonical, lives only here

~/.ariso/recordings/2026-06-02T14-30-05Z/    # unchanged, oats-private
  meta.json
  segments.json
  transcript.md
```

New recordings no longer write `recording.mp3` or `ari-note.md` under
`~/.ariso/recordings/<id>/`; those artifacts live in the vault. Legacy recordings keep
their old files (see Legacy fallback).

### Note format

```markdown
---
oats_id: 2026-06-02T14-30-05Z
title: Team Standup
date: 2026-06-02T14:30:05Z
duration: "00:42:13"
participants: ["Speaker 1", "Speaker 2"]
---
![[Attachments/2026-06-02 Team Standup.mp3]]

<AI notes body>
```

`oats_id` is the only field oats depends on; the rest populate Obsidian's Properties view
and may be freely edited by the user. The audio embed uses an Obsidian wikilink so it plays
inline.

## Components

A new `src-tauri/src/vault.rs` module, mirroring `storage.rs` conventions (atomic writes,
`ARISO_ROOT`-aware root, traversal-guarded ids, pure functions with tempdir tests).

- `vault_root() -> PathBuf` — `~/.ariso/vault`, honoring `ARISO_ROOT` like `ariso_root()`.
- `ensure_vault()` — first-use bootstrap: create the vault dir, `Attachments/`, and a
  minimal `.obsidian/app.json` (sets `attachmentFolderPath: "Attachments"`). Idempotent.
- `note_basename(date, title) -> String` — `YYYY-MM-DD <title>`, sanitized (strip path
  separators, `:`, `..`, reserved chars), collision-suffixed with a numeric counter.
- `write_note(meta, notes_md)` — render front-matter + audio embed + body; atomic
  temp-then-rename; called **once**, at notes-generation time.
- `move_audio_into_vault(meta, bytes|path)` — place the recording's audio in
  `Attachments/<basename>.mp3` at finalize.
- `scan_vault() -> HashMap<oats_id, PathBuf>` — read each top-level `.md` file's
  front-matter head, index by `oats_id`; skip files with no/invalid `oats_id` (same "skip
  junk" tolerance as `list_recordings`).
- `find_note(oats_id)`, `read_note(oats_id)`, `delete_note(oats_id)` — the last removes the
  note and its referenced attachment (cascade).

`meta.json` gains a `notes_written: Option<String>` (RFC3339) field, written when the vault
note is first created.

## Data flow

1. **Finalize** (new recording): audio → `vault/Attachments/`; `meta/segments/transcript` →
   `~/.ariso`; the row appears in the library from `meta.json` as today.
2. **Notes generation completes:** `write_note` to the vault once, then set
   `notes_written` in `meta.json`.
3. **Obsidian:** user opens `~/.ariso/vault`, sees the note with inline audio, edits
   freely. oats never rewrites the body.
4. **`list_recordings`:** build the `scan_vault` map once per call; `has_note` / `has_audio`
   derive from vault presence keyed by `oats_id`, falling back to legacy paths. The detail
   view reads the note from the vault (or legacy).
5. **Delete in Obsidian:** the next scan finds no note for the id, but `notes_written` is
   set ⇒ respected, never regenerated. The meeting still shows in the library from
   `meta.json`, with no note (equivalent to notes-removed).
6. **Delete recording in oats:** cascade removes the vault note + attachment and the
   `~/.ariso/recordings/<id>/` folder.

**Invariant:** oats writes the note body only while the recording is still accreting; once
finalized (`notes_written` set and the append window closed), it never rewrites the body.
Thereafter the vault is the user's. No mirror, no reconciliation of edits.

### Append interaction

Local multi-recording lets a clip that starts within the append window (`APPEND_WINDOW_SECONDS`)
merge into the prior recording, re-stitching `segments.json` and regenerating notes for the
merged recording. Because that regeneration must reach the vault, the note may be (re)written
more than once **while the recording is still appendable**. `notes_written` records the most
recent write; the recording is only considered "final" (and thus off-limits to further oats
writes) once the append window has elapsed. User edits made in Obsidian during an active,
still-appendable recording session are an accepted edge case — appends occur within minutes of
recording, before a user would normally open and edit the note.

## Deletion semantics

| Trigger | Behavior |
|---|---|
| User deletes vault note (Obsidian) | Respected. `notes_written` set + note absent ⇒ never regenerate. Meeting remains in library without a note. |
| User deletes recording (oats library) | Cascade: remove vault note + attachment + `~/.ariso` folder. |
| User deletes audio attachment only | `has_audio` becomes false via fallback/scan; playback unavailable; note unaffected. |

## Legacy fallback (migration option B)

No migration runs. Per-recording resolution order for notes and audio:

1. Vault, by `oats_id` (new recordings).
2. Legacy `~/.ariso/recordings/<id>/ari-note.md` / `recording.mp3` (pre-feature recordings).
3. Otherwise absent.

Legacy recordings have no `notes_written` flag and no vault entry, so respect-deletion never
applies to them — they simply render from the old path. New and legacy recordings coexist
with no data movement.

## Error handling

- **Vault unwritable/missing when notes are ready:** treat like the existing `notes_error`
  path — status stays `Pending`/`Failed`, retried on the next generation attempt; never
  crashes.
- **Unparseable front-matter / missing `oats_id`:** the file is skipped during scan.
- **Filename collision:** append a numeric suffix.
- **Path safety:** title→filename sanitization blocks traversal and reserved characters; all
  writes are atomic (temp+rename in the same dir) and confined to the vault root.

## Security & privacy (`oats-security`)

- A Tauri capability must allow reading `~/.ariso/vault/Attachments/*` for audio playback via
  the asset protocol / `convertFileSrc`.
- Filename sanitization from user-controlled titles guards against path traversal and
  reserved names.
- **Privacy shift:** notes *and audio* now live in the vault. If the user syncs
  `~/.ariso/vault` (iCloud, Obsidian Sync, git), that content leaves the machine — a real
  departure from the offline-mode guarantee. Surface this clearly in the UI. (v1 keeps the
  vault under `~/.ariso`, which is not synced by default.)

## Testing

Pure-function unit tests with tempdir + `ARISO_ROOT`, matching `storage.rs`:

- `note_basename` derivation and collision suffixing.
- Front-matter render/parse roundtrip (including `oats_id` extraction).
- `scan_vault` builds the correct `oats_id → path` map and skips junk files.
- Respect-deletion logic: `notes_written` set + absent note ⇒ skip regeneration.
- Cascade delete removes note + attachment.
- Legacy fallback: vault-miss resolves to the old `~/.ariso` paths.

## Non-goals / deferred (YAGNI)

- User-configurable vault path.
- Migrating existing recordings' notes/audio into the vault (option A backfill).
- Putting `transcript.md`, `segments.json`, or `meta.json` in the vault.
- Wikilinks/backlinks between meetings.
- Cloud-backend meetings in the vault.
- A settings toggle to disable vault writing.

## Risks

- **Hidden folder in Obsidian:** `~/.ariso` is hidden; opening it as a vault needs "show
  hidden folders" in the OS picker. Acceptable for v1; a configurable path removes this
  later.
- **Sync amplifies audio:** audio in the vault means a synced vault carries every recording.
  Mitigated for v1 by keeping the vault unsynced under `~/.ariso`; called out in the UI.
- **Scan cost:** `scan_vault` reads front-matter for every note per `list_recordings` call.
  Fine for hundreds of notes; revisit with an index if libraries grow large.
