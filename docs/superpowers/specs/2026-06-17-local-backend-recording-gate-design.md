# Local Backend Recording Gate — Design

**Date:** 2026-06-17
**Branch:** `fix/settings-local-model-download`

## Problem

When the user switches to the Local backend, recording must not start until **both**
on-device models are fully downloaded:

- the **STT / transcript** model (FluidAudio; readiness = `manifest.json` present)
- the **LLM / notes** model (gemma; readiness = `.complete` marker present)

If the user attempts to record before both models are ready, the app should surface
the Settings window so the download can complete, and the missing downloads should
start automatically.

## Current State (gaps)

Readiness signals already exist:
- `model_manager::is_ready(root)` — STT (manifest present)
- `model_manager::llm_is_ready(root)` — LLM (`.complete` marker present)

But the recording gate is applied inconsistently across the three entry points:

| Entry point | File | Today |
| --- | --- | --- |
| Library "Record" button / picker | `commands.rs` `ensure_recording_allowed` | Local returns `true` unconditionally — **no gate** |
| Tray "Start Recording" | `tray.rs:107-126` | Checks **STT only** (`is_ready`); LLM ignored |
| Auto-record (mic-monitor) | `mic_monitor.rs:288` | Opens recorder directly — **no gate** |

The frontend already has:
- A first-switch `showDownloadConfirm` modal (auto-downloads both on confirm).
- A `tray://show-model-prompt` listener that sets `modelPrompt` and a banner gated on
  `!sttInstalled` only (LLM not considered).

## Decisions

1. **Gate trigger:** Block *any* Local recording attempt unless **both** models are
   ready. "First switch" is just the common case, not the literal condition.
2. **Auto-record:** When models aren't ready, surface Settings the same as a manual
   attempt (do not silently skip).
3. **On block:** Auto-start the missing model download(s) and show progress in Settings.

## Design

### Rust

**1. Pure readiness helper** — `model_manager.rs`:

```rust
pub fn local_models_ready(root: &Path) -> bool {
    is_ready(root) && llm_is_ready(root)
}
```

**2. Gate helpers** — `commands.rs`:

- `local_models_ready(app: &AppHandle) -> bool` — resolves `storage::ariso_root()`,
  returns the pure check; returns `false` if the root is unresolvable.
- `surface_model_download(app: &AppHandle)` — show + focus the `settings` window and
  `emit("tray://show-model-prompt", ())`. This is the same UI surface the tray
  already triggers.

**3. Apply the gate at all three entry points:**

- **`ensure_recording_allowed`** (Library button / picker): for the `local` backend,
  return `true` if `local_models_ready(app)`, else call `surface_model_download(app)`
  and return `false`. The ariso session path is unchanged.
- **`tray.rs` `start_recording`** (local path): replace the STT-only `is_ready(&root)`
  check with `local_models_ready` and reuse `surface_model_download`. Keep the existing
  `run_on_main_thread` wrapping for window operations.
- **`mic_monitor.rs` auto-record** (`Action::Start`): after the auto-join check and
  **before** prompting the user to auto-record, if the backend is `local` and models
  are not ready, call `surface_model_download` and skip (no record prompt). The ariso
  auto-record path is unchanged.

### Frontend — `SettingsView.vue`

Update the existing `tray://show-model-prompt` listener to auto-start downloads
(decision #3):

```js
listen('tray://show-model-prompt', async () => {
  modelPrompt.value = true;
  await refreshModelStatus();
  if (!sttInstalled.value && sttBusy.value !== 'downloading') void onInstallStt();
  if (!llmInstalled.value && llmBusy.value !== 'downloading') void onInstallLlm();
});
```

- Broaden the banner condition from `modelPrompt && !sttInstalled` to
  `modelPrompt && (!sttInstalled || !llmInstalled)`, with wording that references both
  on-device models.
- The Rust per-target download guards already de-dupe, so re-firing the prompt while a
  download is in progress is a safe no-op.

The first-switch `showDownloadConfirm` modal stays as-is. The new gate is the safety
net for the cancel / interrupted-download / deleted-model cases.

## Testing

- **Rust:** unit-test `model_manager::local_models_ready(root)` against temp roots for
  the four marker combinations (neither / STT-only / LLM-only / both), extending the
  existing marker tests.
- **Frontend:** test that the `tray://show-model-prompt` handler auto-starts only the
  missing model(s), and that the banner is visible while either model is incomplete.

## Out of Scope

- Changing `shouldPromptDownload` / the first-switch confirm modal logic.
- Model download mechanics (URLs, progress events, markers).
- Ariso auto-record session gating.
