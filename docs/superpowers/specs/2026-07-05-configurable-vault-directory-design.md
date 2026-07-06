# Configurable vault directory

**Status:** Design approved 2026-07-05
**Related:** `2026-07-03-local-backend-vault-design.md` (the vault this builds on)

## Problem

The local (offline) backend stores a user's notes and audio as an Obsidian vault
whose location is hardcoded to `~/.ariso/vault`. Users want to choose where that
vault lives (e.g. an existing Obsidian vault, a synced folder, an external
drive). Changing the location should point oats at a **fresh, independent
store** — old recordings are left untouched, not migrated into the new location.

## Goals

- A **Vault location** setting, shown under the local backend, that displays the
  current path and lets the user pick a new directory via a native folder picker.
- Default location stays `~/.ariso/vault`.
- Switching to a new directory yields a clean, independent store: new
  recordings/notes/audio are written there, the Library lists only what is found
  there, and switching back to a prior directory restores that library intact.
- No data is copied when switching (the "no migration" rule).
- Existing users keep their current library across the upgrade that introduces
  this feature (a one-time move, distinct from the never-migrate switch case).

## Non-goals

- Migrating, copying, or merging recordings between vault directories on switch.
- Making `models/` (on-device models, ~750 MB) or `pending-uploads/` (cloud
  upload buffer) follow the vault — they are not vault content and stay under
  `~/.ariso`.
- Multi-vault UI (list of vaults, quick switch). One active vault at a time.
- Any change to the cloud (Ariso) backend's behavior.

## Storage model

Today the vault and the recording metadata are **separate roots**:

- `~/.ariso/vault/` — Obsidian notes (`*.md`) + `Attachments/*.mp3` + `.obsidian/`
- `~/.ariso/recordings/<id>/` — per-recording `meta.json`, `transcript.md`,
  legacy `ari-note.md` / `recording.mp3`

Because the Library list is built from the recordings metadata, a vault-only
relocation would leave old recordings listed with broken note/audio links. To
make each vault a self-contained store, **all local recording state moves under
the vault directory**, with bookkeeping in a hidden `.oats/` folder (Obsidian
hides dot-folders, like `.obsidian/`):

```
<vault-dir>/
  .oats/
    recordings/<id>/{meta.json, transcript.md, ari-note.md, recording.mp3(legacy)}
  <YYYY-MM-DD Title>.md          # user-facing Obsidian notes
  Attachments/*.mp3              # audio attachments
  .obsidian/app.json
```

`models/` and `pending-uploads/` remain resolved from `~/.ariso` (via
`ariso_root()`), unchanged.

## Architecture

### Rust: configurable vault root

`vault.rs` gains a process-global override so the free `vault_root()` function
(called deep in the transcription pipeline where no `AppHandle` is in scope) can
resolve the configured path:

- `static VAULT_DIR: RwLock<Option<PathBuf>>` (defaults to `None`).
- `set_vault_override(PathBuf)` / `clear_vault_override()` — set at startup and
  on change; `clear` supports hermetic tests.
- `vault_root()` — returns the override if set, else `ariso_root()?.join("vault")`.
  Existing tests that set `ARISO_ROOT` and expect `<ARISO_ROOT>/vault` keep
  working because the override defaults to `None`.
- `meta_root()` — `vault_root()?.join(".oats")`, the hidden bookkeeping root.
- `ensure_vault()` — additionally creates `.oats/recordings/`.

### Rust: recording paths move under the vault

Callers that currently build recording paths from `ariso_root()` switch to
`meta_root()`:

- `commands::recording_dir` → `recordings_dir(&meta_root()?)`
  (= `<vault>/.oats/recordings/<id>`).
- `commands::list_local_recordings` → lists from `meta_root()`; the vault overlay
  (`scan_vault`, `audio_path`) already uses `vault_root()`.
- `transcribe.rs` metadata reads/writes → `meta_root()`.

`storage::recordings_dir(root)` keeps its signature (`root/recordings`); only the
`root` passed in changes. `models_dir` / `pending_uploads_dir` callers keep
passing `ariso_root()`.

### Rust: settings + commands

- Persist the chosen directory as an absolute path string under key `vaultDir` in
  the existing `settings.json` store (same store as `backend`).
- `get_vault_dir() -> String` — returns the resolved active path (the stored
  value, or the default `~/.ariso/vault`).
- `set_vault_dir(path: String) -> Result<(), String>`:
  1. Reject if a recording is in progress (mirrors the backend-switch guard).
  2. Validate the path is absolute; create the directory if missing; surface
     create/permission errors.
  3. `set_vault_override(path)` then `ensure_vault()` at the new location.
  4. Persist `vaultDir` to `settings.json`.
  5. Emit a library-refresh event so open windows reload from the new vault.
  6. **No** copying of existing data.

### Rust: startup + one-time upgrade migration

In `main.rs` setup (before `ensure_vault()`):

1. Read `vaultDir` from the store; if present, `set_vault_override`.
2. **Upgrade migration** (idempotent, default-vault only): if `vaultDir` is unset
   (default vault in use), legacy `~/.ariso/recordings/` exists, and
   `~/.ariso/vault/.oats/recordings/` does not, move the legacy recordings dir
   into the default vault's `.oats/recordings/`. This preserves existing users'
   libraries and runs at most once.
3. `ensure_vault()`.

### Frontend

- Add `tauri-plugin-dialog` (Cargo dependency, JS dependency, and a capability
  entry) to provide the native folder picker via `open({ directory: true })`.
- `tauri.ts` — `getVaultDir()` and `setVaultDir(path)` invoke wrappers, plus a
  small helper that opens the folder dialog and returns the chosen path (or null
  on cancel).
- `SettingsView.vue` — replace the hardcoded `~/.ariso/vault` hint (in the local
  backend area) with a **Vault location** row: the current path (from
  `getVaultDir`) and a **Change…** button. The button opens the folder picker,
  calls `setVaultDir`, then triggers a Library refresh. Disabled while recording,
  with the existing "can't change while recording" affordance. Copy explains the
  new directory is a fresh store and existing recordings are not moved, plus the
  existing sync/privacy note.

### Library refresh

Reuse the frontend's existing reload path (the Library already reloads on window
focus and on recording lifecycle events). `set_vault_dir` emits a refresh event
the Library listens for; on receipt it re-runs `list_local_recordings` against
the new vault.

## Error handling

- Non-absolute or unusable path → `set_vault_dir` returns an error; the Settings
  UI surfaces it and leaves the active vault unchanged.
- Change attempted while recording → rejected with a clear message; UI also
  disables the control in that state.
- Folder picker cancelled → no-op.
- Migration failure at startup → logged; the app still launches with the default
  vault (matches the existing `ensure_vault` failure handling in `main.rs`).

## Testing

**Rust (unit):**
- `vault_root()` honors the override and falls back to `<ariso_root>/vault`.
- `meta_root()` = `<vault>/.oats`; `ensure_vault()` creates `.oats/recordings`.
- Recording paths resolve under the vault's `.oats` when the override is set.
- Upgrade migration moves legacy recordings once and is idempotent (no-op when
  the target already exists or the legacy dir is absent).
- `set_vault_dir` persists `vaultDir`, updates the override, and rejects while
  recording.

**Frontend (Vitest, run in isolation per the heavy-view brittleness note):**
- SettingsView renders the current vault path and the Change… control; the
  control is disabled while recording; choosing a directory calls `setVaultDir`
  and refreshes.
- `getVaultDir` / `setVaultDir` wrappers invoke the expected commands.

## Security

Covered by an `oats-security` pass:
- `set_vault_dir` accepts a user-chosen absolute path from the native dialog —
  validate it's absolute and handle create/permission failures; the path is
  chosen by the user, so traversal is not the threat, but malformed input must
  fail cleanly.
- The new `tauri-plugin-dialog` capability is the minimal folder-open permission,
  scoped to the windows that need it.
- Offline privacy guarantee is unchanged: the vault stays fully local; the copy
  reiterates that syncing the chosen folder moves data off-device.
