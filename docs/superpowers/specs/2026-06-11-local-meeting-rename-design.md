# Local Meeting Rename — Design

**Date:** 2026-06-11
**Status:** Approved

## Goal

When using the local backend, renaming a meeting works exactly like the ariso
backend: the same inline-edit UX on the meeting title, persisted to disk.

## Requirements

1. Local titles are editable with the same inline-edit UX as ariso meetings
   (clickable title → input, Enter commits, Escape cancels, blur behaves as
   described below).
2. Local titles are limited to 40 characters (after trimming). Exceeding the
   limit shows an inline warning and blocks saving; it does not hard-cap input.
3. No character restrictions and no duplicate-name check. The title lives in
   `meta.json` (`~/.ariso/recordings/<id>/meta.json`); `serde_json` escapes
   quotes and special characters natively. The recording folder name (the
   timestamp ID) is never renamed.
4. Ariso rename behavior is unchanged — no new client-side validation; the
   server remains the authority.

## Design

### 1. Rust — new Tauri command

`rename_local_recording(id: String, title: String)` in
`src-tauri/src/commands.rs`, registered in `main.rs`:

- Trim the title; return an error if empty or longer than 40 characters
  (defense in depth — the UI validates first).
- `read_meta(id)` → set `title` → `write_meta()` (existing atomic write).
- Return an error if the recording does not exist.

### 2. TypeScript bridge

`local.renameRecording(id, title)` in `src/tauri.ts`, alongside the existing
`local.*` wrappers.

### 3. Backend abstraction

Add to the `Backend` interface in `src/composables/useBackend.ts`:

```ts
renameMeeting(id: string, title: string): Promise<void>;
```

- `ArisoBackend.renameMeeting` wraps the existing `updateMeetingNotesTitle`
  PATCH call (logic moves out of the view; behavior unchanged).
- `LocalBackend.renameMeeting` calls `local.renameRecording`.

### 4. View — `src/views/MeetingDetailView.vue`

- `canEditTitle` becomes `!!detail.value` — local titles get the clickable
  title / inline input / "Click to rename" tooltip, same as ariso.
- New computed `titleError`: for local meetings only, when the trimmed draft
  exceeds 40 characters, returns `"Title must be 40 characters or fewer"`.
  Rendered as a small inline warning under the input with a `(47/40)` counter.
- `commitTitle`:
  - While `titleError` is set, Enter does nothing (warning stays visible);
    blur cancels and reverts (existing cancel semantics).
  - Valid commits call `backend.renameMeeting(d.id, next)` instead of the
    direct ariso API call.
  - Error handling, retry semantics, and the `titleUpdated` emit are
    unchanged, so `LibraryView`'s sidebar sync works for local recordings
    with zero changes.

### 5. Error handling

A Rust command failure surfaces like the existing ariso failure path: the
editor stays open, the draft is preserved, and the error is logged.

## Testing

- **Rust unit tests** for `rename_local_recording`: happy path, empty title,
  over-limit title, missing recording, title containing quotes round-trips
  through `meta.json`.
- **Vitest** (`MeetingDetailView.test.ts`): update the existing "local
  recordings block editing" test to the new editable behavior; add tests for
  local rename commit, over-limit blocking on Enter, and blur-revert while
  invalid.

## Out of scope

- Renaming the recording folder on disk.
- Duplicate-title detection.
- Any change to ariso rename validation.
