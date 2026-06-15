# First-time download confirm for the Local backend

**Date:** 2026-06-15
**Status:** Approved, ready for planning

## Problem

When a user switches the transcription backend to **Local** for the first
time, nothing asks them whether to fetch the on-device models. Today,
`selectBackend` (`src/views/SettingsView.vue`) *silently* auto-starts only the
STT (speech) download, while the language model (LLM) stays opt-in behind its
own Install button. The result: the user lands on a backend that is not
actually ready to record, with no clear prompt and no LLM until they discover a
second button.

## Goal

On the **first** switch to Local, show a confirm dialog offering to download the
required on-device models. On confirm, download **both** models (STT + LLM)
**at the same time**. After that first decision, never prompt again.

## Behavior

Switching to Local triggers the dialog only when **all** hold:

- the target backend is `local`,
- the user has **not** previously been prompted (persisted flag), and
- models are not already ready / not unsupported on this device.

The dialog is an in-app modal styled like the existing settings cards. Copy:

> **Download on-device models?**
> Local transcription needs the speech and language models (~750 MB). They
> download once and run entirely on your device.
> **[ Download ]  [ Cancel ]**

- **Download** → persist the first-time flag, start **both** downloads in
  parallel, close the modal. The two existing model rows show live progress
  (the existing per-model progress events and `anyDownloading` disabling handle
  this unchanged).
- **Cancel** → revert the backend to **Ariso**, close the modal. The flag is
  **not** persisted, so a later switch to Local will prompt again.

This **replaces** the current silent auto-STT-download branch in
`selectBackend`.

### Flag semantics: mark on confirm, not on show

The flag (`localModelsPrompted` in `settings.json`) is written **only when the
user clicks Download**, not when the dialog appears. Rationale: marking on show
would strand a user who cancels (or misclicks) — bounced to Ariso and never
prompted again, left to discover the two manual Install buttons unaided.
Marking on confirm means "once the user has committed to local setup, never nag
again," while a cancel simply defers the decision.

## Components

### Rust — `src-tauri/src/model_manager.rs`

Today a single global `DOWNLOAD_IN_PROGRESS: AtomicBool` serializes *all*
downloads: calling `download_local_stt` and `download_local_llm` concurrently
makes the second fail with "a model download is already in progress." True
parallel download requires splitting this into **per-target guards**:

- one guard for STT, one for LLM.
- STT writes `manifest.json` at the models root; the LLM writes into
  `llm/<name>/` with its own `.complete` marker. The paths are disjoint, so two
  *different* targets cannot race. Each guard still rejects a duplicate of its
  **own** target (two STT downloads, or two LLM downloads).

`run_download` is generic over its events; thread the relevant guard through it
(or check/set the guard in each `#[tauri::command]` entry point before calling
`run_download`). The existing Drop-guard pattern that clears the flag on every
exit path is preserved per target.

### TypeScript — `src/tauri.ts`

Add a persisted flag pair over `settings.json`, mirroring the existing
`isOnboarded` / `setOnboarded`:

```ts
export async function hasPromptedLocalModels(): Promise<boolean>
export async function setPromptedLocalModels(value: boolean): Promise<void>
```

Key: `localModelsPrompted`.

### TypeScript — `src/views/settingsDownload.ts`

Replace `shouldAutoDownload` with a pure decision function:

```ts
export function shouldPromptDownload(
  backend: 'ariso' | 'local',
  alreadyPrompted: boolean,
  state: ModelStatus['state'],
): boolean
```

Returns true only when `backend === 'local'`, `!alreadyPrompted`, and the state
is not `ready`, `downloading`, or `unsupported`. (`rowStatusText` and the `Busy`
type are unchanged.)

### Vue — `src/views/SettingsView.vue`

- New reactive state `showDownloadConfirm`.
- `selectBackend`: when switching to `local`, read the persisted flag and call
  `shouldPromptDownload`; if true, open the modal instead of silently
  downloading. Remove the old `shouldAutoDownload` / silent `onInstallStt`
  branch.
- Confirm handler: `await setPromptedLocalModels(true)`, then start both
  downloads in parallel (`void onInstallStt(); void onInstallLlm();`), close
  the modal.
- Cancel handler: revert `backend` to `'ariso'` (and persist via
  `setBackendSetting`), close the modal.
- Modal markup: a simple overlay + card with the copy above and the two
  buttons, reusing existing `.primary-btn` / `.secondary-btn` styles.

## Data flow

```
selectBackend('local')
  └─ setBackendSetting('local'); emit sync
  └─ refreshModelStatus()
  └─ shouldPromptDownload(local, alreadyPrompted, state)?
       ├─ true  → showDownloadConfirm = true
       │            ├─ Download → setPromptedLocalModels(true)
       │            │              onInstallStt() ┐ (parallel; Rust per-target
       │            │              onInstallLlm() ┘  guards allow both)
       │            └─ Cancel   → backend='ariso'; setBackendSetting('ariso')
       └─ false → no dialog (manual Install buttons remain available)
```

## Error handling

- Each download already has its own try/catch in `onInstallStt` /
  `onInstallLlm`, setting per-model `busy = 'error'` and surfacing
  `rowStatusText`. Running them in parallel does not change this — one failing
  does not abort the other.
- If `setPromptedLocalModels` throws, the downloads still proceed; the flag
  write is best-effort (worst case: prompted once more later).
- The Rust per-target guard still returns an error string if the *same* target
  is double-invoked; the existing UI disables the buttons while downloading, so
  this is a backstop, not an expected path.

## Testing

- **`src/views/SettingsView.download.test.ts`** — add a `shouldPromptDownload`
  block: prompts for `local` + not-prompted + `not_downloaded`/`error`; does not
  prompt when `alreadyPrompted`, when `ready`/`downloading`/`unsupported`, or for
  `ariso`.
- **Rust `model_manager` tests** — existing status/manifest/marker tests must
  still pass after the guard split. The per-target guard preserves
  single-target serialization, so no behavioral regression for either target.
- **Manual** — switch to Local first time → dialog → Download → both rows
  progress concurrently to ✓; Cancel → back on Ariso, re-prompts next switch;
  second-ever switch (flag set) → no dialog. Run the cargo suite with the
  `DYLD_LIBRARY_PATH` + `--test-threads=1` workaround.

## Out of scope

- Changing the existing tray-driven `tray://show-model-prompt` banner.
- Re-downloading / repair flows beyond the existing Install buttons.
- Disk-space preflight or download cancellation UI.
