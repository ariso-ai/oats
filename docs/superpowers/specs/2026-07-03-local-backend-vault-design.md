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
5. **Split of truth (Option A — vault is the sole home for both note and audio):**
   - **Canonical in the vault:** the note (`<basename>.md`) and audio
     (`Attachments/<basename>.mp3`). The audio lives *only* here — new recordings do not
     keep a `recording.mp3` under `~/.ariso`. The whole recording pipeline (fresh write,
     STT input, append-concatenation, retry, playback) reads and writes audio at the vault
     attachment path.
   - **Canonical in `~/.ariso/recordings/<id>/`:** `meta.json`, `segments.json`,
     `transcript.md` — oats-private machine state. `meta.json` gains two pointer/marker
     fields (below).
6. **Link by `oats_id`; resolve audio by `meta.audio_file`.** The note's front-matter
   carries `oats_id` equal to the recording folder id, so oats locates a note by scanning
   front-matter (renames/moves in Obsidian never break it). The attachment filename is
   title-derived, so the pipeline can't guess it from the id — `meta.json` stores
   `audio_file` (the attachment's basename, e.g. `2026-06-02 Team Standup.mp3`) as the
   id→attachment pointer. `meta.audio_file` stays private; it just names a file in the vault.
7. **Respect deletions; no mirror.** If the user deletes the vault note, it stays deleted —
   oats has no spontaneous regeneration path (notes are only (re)written at finalize, on
   append, or on an explicit retry), so a deleted note is respected by construction. A
   `notes_written` timestamp in `meta.json` records that oats generated the note at least
   once; it exists so `derive_notes_status` can tell "never generated (still pending)" from
   "generated then deleted (show as absent, not a perpetual spinner)".
8. **Cascade delete (forward requirement).** Deleting a recording inside oats must remove
   its vault note + audio attachment as well as the private `~/.ariso` folder. **Note:** the
   app has no local "delete recording" command today (only Ariso clip deletion exists), so
   there is nothing to build in v1. `vault::delete_recording_artifacts` is provided so that
   whoever adds local deletion later wires in the cascade; a unit test covers it.
9. **Migration = fallback, no backfill.** Existing local recordings are not migrated. Reads
   check the vault first (by `oats_id` / `meta.audio_file`) and fall back to the legacy
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

~/.ariso/recordings/2026-06-02T14-30-05Z/    # oats-private
  meta.json                                  # + audio_file, notes_written
  segments.json
  transcript.md
  append-clip.mp3                            # transient scratch during an append only
```

New recordings no longer write `recording.mp3` or `ari-note.md` under
`~/.ariso/recordings/<id>/`; those artifacts live in the vault. The only audio ever written
under `~/.ariso` is the transient `append-clip.mp3` scratch file (a single clip being
transcribed mid-append; deleted when the append commits). Legacy recordings keep their old
files (see Legacy fallback).

`RecordingMeta` gains two optional fields:

```rust
/// Basename of this recording's audio attachment in the vault
/// (`<vault>/Attachments/<audio_file>`). None for legacy recordings, whose audio
/// still lives at `~/.ariso/recordings/<id>/recording.mp3`.
pub audio_file: Option<String>,
/// RFC3339 time oats last wrote this recording's note into the vault. None means
/// never generated. Set means generated at least once; an absent vault note then
/// signals a user deletion rather than "still pending".
pub notes_written: Option<String>,
```

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
  separators, `:`, `..`, reserved chars). The caller collision-suffixes against existing
  files with a numeric counter (`(2)`, `(3)`, …); `note_basename` is the pure derivation and
  a `unique_basename(dir, base)` helper does the suffixing.
- `audio_path(audio_file) -> PathBuf` / `note_path(basename) -> PathBuf` — resolve
  `<vault>/Attachments/<audio_file>` and `<vault>/<basename>.md`.
- `write_audio(audio_file, bytes)` — atomic write into `Attachments/`; used by the whole
  pipeline (fresh write, append-concat result, retry, save-failed-clip) as the audio's only
  home.
- `read_audio(audio_file) -> Vec<u8>` — read the attachment (STT input, append-concat source,
  retry, playback all go through this).
- `render_note(meta, notes_md) -> String` — pure: front-matter (`oats_id`, `title`, `date`,
  `duration`, `participants`) + `![[Attachments/<audio_file>]]` embed + body.
- `note_body(contents) -> String` — pure inverse: strip front-matter and the leading audio
  embed, returning just the notes body for in-app rendering.
- `write_note(basename, contents)` — atomic write of a note `.md`; called by
  `process_notes`, which then sets `meta.notes_written`.
- `scan_vault() -> HashMap<oats_id, PathBuf>` — read each top-level `.md` file's front-matter
  head, index by `oats_id`; skip files with no/invalid `oats_id` (same "skip junk" tolerance
  as `list_recordings`).
- `find_note(oats_id)`, `read_note(oats_id)`, `delete_recording_artifacts(oats_id, audio_file)`
  — the last removes the vault note (by scan) and its attachment (cascade).

## Data flow

1. **Fresh recording:** derive `audio_file = unique_basename(...) + ".mp3"`, store it in
   `meta.audio_file`, `write_audio` into the vault; STT transcribes by reading the attachment
   back via `read_audio`; `segments`/`transcript`/`meta` → `~/.ariso`. The library row comes
   from `meta.json` as today.
2. **Notes generation completes:** `render_note` + `write_note` into the vault, then set
   `notes_written` in `meta.json`.
3. **Obsidian:** user opens `~/.ariso/vault`, sees the note with inline audio, edits freely.
   oats never rewrites the body.
4. **`list_recordings`:** build the `scan_vault` map once per call; `has_note` derives from
   vault presence by `oats_id` (legacy: `~/.ariso` `ari-note.md`); `has_audio` derives from
   `meta.audio_file` present + attachment exists (legacy: `~/.ariso` `recording.mp3`). The
   detail view reads the note body from the vault via `note_body` (or legacy).
5. **Delete in Obsidian:** the next scan finds no note for the id; `notes_written` is set, so
   `derive_notes_status` reports "absent" rather than "pending", and nothing regenerates.
6. **Delete recording in oats:** cascade removes the vault note + attachment and the
   `~/.ariso/recordings/<id>/` folder.

**Invariant:** oats writes the note body only while the recording is still accreting; once
finalized (`notes_written` set and the append window closed), it never rewrites the body.
Thereafter the vault is the user's. No mirror, no reconciliation of edits.

### Append interaction

Local multi-recording lets a clip that starts within the append window (`APPEND_WINDOW_SECONDS`)
merge into the prior recording, re-stitching `segments.json` and regenerating notes for the
merged recording. Two consequences for the vault:

- **Audio concatenation reads/writes the vault attachment.** `append_recording_core` reads
  the target's audio via `read_audio(target.audio_file)`, concatenates the new clip's bytes,
  and `write_audio`s the result back to the same attachment. The per-clip scratch file
  (`append-clip.mp3`, used only to transcribe the isolated clip) stays in `~/.ariso` and is
  deleted when the append commits — it is never the canonical audio.
- **The note may be (re)written while the recording is still appendable.** `notes_written`
  records the most recent write. User edits made in Obsidian during an active, still-appendable
  session are an accepted edge case — appends occur within minutes of recording, before a user
  would normally open and edit the note.

## Deletion semantics

| Trigger | Behavior |
|---|---|
| User deletes vault note (Obsidian) | Respected. `notes_written` set + note absent ⇒ never regenerate. Meeting remains in library without a note. |
| User deletes recording (oats library) | Cascade: remove vault note + attachment + `~/.ariso` folder. *No local delete command exists yet — `delete_recording_artifacts` is provided and tested for when one is added.* |
| User deletes audio attachment only | `has_audio` becomes false via fallback/scan; playback unavailable; note unaffected. |

## Legacy fallback (no backfill)

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

- **No new Tauri capability needed.** Audio plays through the existing `read_recording_audio`
  command, which returns bytes over IPC (the frontend builds a Blob URL) — it is not the
  asset protocol, so relocating the file to the vault needs no capability change. The command
  just resolves the vault attachment (via `meta.audio_file`) with a legacy fallback.
- Filename sanitization from user-controlled titles guards against path traversal and
  reserved names. `note_basename` strips `/`, `\`, `:`, `..`, and leading dots; a title that
  sanitizes to empty falls back to the recording id.
- **Privacy shift:** notes *and audio* now live in the vault. If the user syncs
  `~/.ariso/vault` (iCloud, Obsidian Sync, git), that content leaves the machine — a real
  departure from the offline-mode guarantee. Surface this clearly in the UI. (v1 keeps the
  vault under `~/.ariso`, which is not synced by default.)

## Testing

Pure-function unit tests with tempdir + `ARISO_ROOT`, matching `storage.rs`:

- `note_basename` derivation/sanitization (traversal chars, empty→id fallback) and
  `unique_basename` collision suffixing.
- `render_note` + `note_body` roundtrip (front-matter + embed + body → body), and `oats_id`
  extraction by `scan_vault`.
- `scan_vault` builds the correct `oats_id → path` map and skips junk files.
- `derive_notes_status` with the new `notes_written` arg: absent note + `notes_written` set ⇒
  not `Pending`; absent note + `notes_written` None ⇒ `Pending`.
- `write_audio`/`read_audio` roundtrip and `delete_recording_artifacts` cascade (note +
  attachment removed).
- Legacy fallback: `meta.audio_file` None ⇒ audio resolves to `~/.ariso` `recording.mp3`;
  no vault note ⇒ note resolves to `~/.ariso` `ari-note.md`.
- `finalize_core` / `append_recording_core` / `retry_transcription_core` integration
  (tempdir + `ARISO_ROOT`): audio ends up in the vault, append concatenates the vault
  attachment, and no `recording.mp3` is written under `~/.ariso`.

## Non-goals / deferred (YAGNI)

- User-configurable vault path.
- Migrating existing recordings' notes/audio into the vault (one-time backfill).
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
