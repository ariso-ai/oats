---
name: oats-tauri
description: Use when changing oats backend — Rust commands in src-tauri, the invoke contract, capabilities/permissions, Tauri plugins, multi-window, or running the cargo test suite. Includes the macOS build/test workarounds.
---

# oats Backend (Tauri v2) Conventions

The backend is Rust in `src-tauri/`, exposing commands the Vue frontend calls via
`invoke`. Tauri v2, Apple-Silicon macOS only.

## Layout (`src-tauri/src/`)

- `main.rs` — app setup, plugin registration, tray, window creation, `invoke_handler`.
- `commands.rs` — the bulk of `#[tauri::command]` functions (the frontend API surface).
- Domain modules: `audio_capture.rs`, `transcribe.rs`, `model_manager.rs`,
  `storage.rs`, `recording_state.rs`, `mic_monitor.rs`, `meeting_notifications.rs`,
  `recorder_pill.rs`, `tray.rs` / `tray_meeting.rs`, `update_manager.rs`.

## The invoke contract

Frontend → backend goes through typed wrappers in `src/tauri.ts`
(`import { invoke } from '@tauri-apps/api/core'`). To add a command:
1. Write `#[tauri::command] async fn foo(...)` in `commands.rs` (or a domain module).
2. Register it in the `invoke_handler![...]` in `main.rs`.
3. Add a typed wrapper in `src/tauri.ts`; call that from views/composables.

## Capabilities & permissions

`src-tauri/capabilities/default.json` is the allowlist — it names the **windows**
(`main`, `waveform`, `settings`, `oauth`, `update`, `library`, `onboarding`) and the
**permissions** they get (`core:window:*`, `store:default`, `opener:default`,
`notification:default`, `updater:default`, plus a scoped `opener:allow-open-url` for
`x-apple.systempreferences:*`). A new window or plugin capability must be added here or
calls are rejected at runtime.

## Plugins in use

`@tauri-apps/plugin-store` (persisted settings), `plugin-updater` (R2-hosted auto-update,
configured under `plugins.updater` in `tauri.conf.json`), `plugin-notification`,
`plugin-opener`. The `tauri-plugin-mcp` server is gated behind `--features mcp` — see
`oats-debugging`.

## Run / build

- `npm run tauri:dev` — run the app.
- `npm run tauri:dev:debug` — run with the MCP server (`--features mcp`).
- `npm run tauri:build` — bundle. **Exits non-zero on the updater signing step
  (`TAURI_SIGNING_PRIVATE_KEY` missing) but the `.app` is already built** at
  `src-tauri/target/release/bundle/macos/oats.app` and is fully usable.

## Testing the Rust suite (macOS workarounds — important)

Default `cargo test` fails on this platform two ways. Full green command:

```
DYLD_LIBRARY_PATH="/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift-5.5/macosx" \
  cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1
```

- `DYLD_LIBRARY_PATH` fixes a missing `libswift_Concurrency.dylib` SIGABRT. A benign
  `objc[...] Class ... implemented in both` warning still prints.
- `--test-threads=1` avoids a pre-existing isolation flake (some `rename_local_recording*`
  / `finalize*` tests mutate process-wide state and collide in parallel).

### Fresh git worktree bootstrap

A fresh worktree is missing `.gitignore`d build inputs; bootstrap before building/testing:
1. **Sidecar binaries** (`src-tauri/binaries/` is gitignored) — copy from the primary
   checkout: `cp -R <primary>/src-tauri/binaries/{ariso-stt-aarch64-apple-darwin,mlx-swift_Cmlx.bundle} src-tauri/binaries/`.
2. `npm ci` (repo uses npm + `package-lock.json`).
3. `npm run vite:build` once — `generate_context!` needs `dist/` to exist.

## macOS permission (TCC) testing

Test TCC grants (system-audio etc.) on the **bundled app** (stable id `ai.ariso.desktop`),
not the adhoc-signed `tauri:dev` binary (unstable identity across rebuilds). System-audio
capture (Core Audio process taps) prompts on **capture start** and the grant lands under
"System Audio Recording Only" in Privacy → Screen & System Audio Recording. Reset a grant
with `tccutil reset ScreenCapture ai.ariso.desktop`; a leftover broad Screen Recording
grant silently satisfies the tap and suppresses the narrow prompt.

For frontend conventions see `oats-vue`; for the big picture see `oats-architecture`;
for driving the running app see `oats-debugging`.
