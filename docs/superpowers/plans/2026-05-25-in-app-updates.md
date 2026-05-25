# In-App Updates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a Sparkle-style in-app updater for the Ariso Mac app (Tauri v2 + Vue 3). Background checks on launch + every 24h, a modern card dialog rendered from the GitHub release body, one-click install + relaunch, support for mandatory updates.

**Architecture:** Use Tauri's official `@tauri-apps/plugin-updater` to do the cryptography, download, and bundle-replacement. A new Rust module `update_manager.rs` owns scheduling, persistent state (skip / snooze / auto-check toggle), and command handling. A new Vue route `/#/update` is opened on-demand by the manager when an update is found. Settings → About becomes a passive status surface; the tray menu gains a single "Check for Updates…" item.

**Tech Stack:** Tauri v2 (Rust), `tauri-plugin-updater@^2`, `tauri-plugin-store@^2` (already in project), Vue 3 + vue-router, Ed25519 signing via `tauri signer`, GitHub Releases as the manifest host.

**Spec:** `docs/superpowers/specs/2026-05-25-in-app-updates-design.md`

---

## File map

| File | Status | Responsibility |
|---|---|---|
| `src-tauri/Cargo.toml` | modify | Add `tauri-plugin-updater = "2"` |
| `src-tauri/tauri.conf.json` | modify | Register `plugins.updater` with endpoint + Ed25519 pubkey |
| `src-tauri/capabilities/default.json` | modify | Add `updater:default` permission, add `"update"` window |
| `src-tauri/src/update_manager.rs` | **create** | State, predicates, commands, scheduler, window opener |
| `src-tauri/src/main.rs` | modify | Register `tauri-plugin-updater`, register manager commands, spawn scheduler task |
| `src-tauri/src/tray.rs` | modify | Add "Check for Updates…" item to idle + recording menus |
| `package.json` | modify | Add `@tauri-apps/plugin-updater` |
| `src/main.ts` | modify | Register `/update` route |
| `src/tauri.ts` | modify | Add `updater` helper exposing the commands |
| `src/views/UpdateView.vue` | **create** | Modern card dialog (mockup B) |
| `src/views/SettingsView.vue` | modify | Replace About section with status + manual check + auto-check toggle |
| `.github/workflows/desktop.yaml` | modify | Export signing secrets, generate `latest.json`, upload tarball + sig + manifest |

**Note on `tauri-plugin-process` (mentioned in spec §2.4):** the spec lists it as a dependency, but on review the relaunch is initiated from Rust (`app.restart()` is a core Tauri method, not a plugin). The plugin only exposes `relaunch()` to JS, which we don't need. This plan **does not** add `tauri-plugin-process` — small deliberate divergence from the spec, justified by YAGNI. Re-add it if a future feature needs JS-initiated relaunch.

---

## Phase 1 — Bootstrap (deps, keypair, plugin config)

### Task 1: Generate the Ed25519 signing keypair (one-time, manual)

**Files:** None in repo. Generates files in user's home + GitHub Environment secrets.

This task is performed by the human operator, not the implementing agent. The agent should pause and instruct the user to do this, then ask for the public key string before proceeding.

- [ ] **Step 1: Instruct the user to generate the keypair**

Tell the user to run, in any terminal:

```bash
mkdir -p ~/.tauri
npx @tauri-apps/cli signer generate -w ~/.tauri/ariso-updater.key
```

This prints a base64 public key to stdout and writes the private key to `~/.tauri/ariso-updater.key`. Both should be captured.

- [ ] **Step 2: Instruct the user to upload the private key to the `release` GitHub Environment**

In the repo's **Settings → Environments → release → Add secret**:
- `TAURI_SIGNING_PRIVATE_KEY` — full contents of `~/.tauri/ariso-updater.key`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — if a password was set during generation; otherwise leave unset

- [ ] **Step 3: Ask the user for the public key string**

The agent needs the base64 public key (the stdout from Step 1) to put in `tauri.conf.json` in Task 4. Ask the user to paste it.

- [ ] **Step 4: No commit yet — proceed to Task 2 with the pubkey in hand**

### Task 2: Add the Rust plugin dependency

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add the dependency**

Edit `src-tauri/Cargo.toml`, adding to the `[dependencies]` block immediately after `tauri-plugin-store = "2"`:

```toml
tauri-plugin-updater = "2"
```

The final `[dependencies]` block should be:

```toml
[dependencies]
tauri = { version = "2", features = ["tray-icon", "image-png", "macos-private-api"] }
tauri-plugin-store = "2"
tauri-plugin-updater = "2"
tauri-plugin-opener = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", features = ["json", "multipart"] }
tokio = { version = "1", features = ["full"] }
url = "2"
tauri-plugin-mcp = { git = "https://github.com/P3GLEG/tauri-plugin-mcp", optional = true }
screencapturekit = { version = "1.5", features = ["macos_13_0"] }
```

- [ ] **Step 2: Verify it resolves**

Run: `cd src-tauri && cargo check --locked`

If `--locked` fails (because `Cargo.lock` doesn't have the new dep yet), run `cargo check` to update the lockfile, then re-run with `--locked` after committing.

Expected: `cargo check` passes; new lockfile entries appear.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "deps(tauri): add tauri-plugin-updater"
```

### Task 3: Add the JS plugin dependency

**Files:**
- Modify: `package.json`

- [ ] **Step 1: Add the dependency**

From the repo root:

```bash
npm install @tauri-apps/plugin-updater@^2
```

Verify `package.json`'s `dependencies` block contains:

```json
"@tauri-apps/plugin-updater": "^2.x.x"
```

(Exact patch version may vary — accept whatever npm resolves.)

- [ ] **Step 2: Verify the frontend still builds**

Run: `npm run vite:build`

Expected: build succeeds, `dist/` is populated.

- [ ] **Step 3: Commit**

```bash
git add package.json package-lock.json
git commit -m "deps(desktop): add @tauri-apps/plugin-updater"
```

### Task 4: Configure the updater plugin in `tauri.conf.json`

**Files:**
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Add the `plugins.updater` block**

Edit `src-tauri/tauri.conf.json`. The current file has no top-level `plugins` key — add one between `bundle` and the closing brace. The full updated file:

```json
{
  "$schema": "https://raw.githubusercontent.com/nicoverbruggen/tauri-v2-schema/main/tauri.schema.json",
  "productName": "Ariso",
  "version": "0.2.0",
  "identifier": "ai.ariso.desktop",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "npm run vite:dev",
    "beforeBuildCommand": "npm run vite:build"
  },
  "app": {
    "windows": [],
    "security": {
      "csp": null
    },
    "macOSPrivateApi": true
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico",
      "icons/icon.png"
    ],
    "macOS": {
      "entitlements": "entitlements.plist",
      "infoPlist": "Info.plist"
    },
    "createUpdaterArtifacts": true
  },
  "plugins": {
    "updater": {
      "endpoints": [
        "https://github.com/ariso-ai/conflux/releases/latest/download/latest.json"
      ],
      "pubkey": "<PASTE PUBLIC KEY FROM TASK 1 HERE>"
    }
  }
}
```

Two important additions besides `plugins.updater`:
1. `"createUpdaterArtifacts": true` inside `bundle` — this tells the Tauri bundler to emit `Ariso.app.tar.gz` and `.sig` alongside the DMG.
2. The pubkey string from Task 1 (base64, single line).

- [ ] **Step 2: Verify config parses**

Run: `cd src-tauri && cargo check --locked`

Expected: no schema errors. (If the build fails because `createUpdaterArtifacts` is in the wrong place, move it; the Tauri v2 schema places it under `bundle`.)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tauri.conf.json
git commit -m "config(tauri): add updater endpoint + pubkey + updater artifacts"
```

### Task 5: Grant capabilities

**Files:**
- Modify: `src-tauri/capabilities/default.json`

- [ ] **Step 1: Add updater permission and the `update` window to the allowlist**

Replace the file's contents with:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default permissions for the desktop app",
  "windows": ["main", "waveform", "settings", "oauth", "update"],
  "permissions": [
    "core:default",
    "core:window:allow-create",
    "core:window:allow-set-focus",
    "core:webview:allow-create-webview",
    "core:webview:allow-create-webview-window",
    "core:webview:allow-set-webview-focus",
    "core:webview:allow-webview-close",
    "core:window:allow-close",
    "core:window:allow-start-dragging",
    "store:default",
    "opener:default",
    "updater:default"
  ]
}
```

Two changes: `"update"` added to `windows`, `"updater:default"` added to `permissions`.

- [ ] **Step 2: Verify cargo check still passes**

Run: `cd src-tauri && cargo check --locked`

Expected: passes.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/capabilities/default.json
git commit -m "capabilities: grant updater + allow update window"
```

### Task 6: Register the updater plugin in `main.rs`

**Files:**
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add the plugin registration**

In `src-tauri/src/main.rs`, find the `.plugin(tauri_plugin_opener::init())` line and add the updater plugin immediately after it. The updated builder block:

```rust
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
```

(Rest of file unchanged for now — commands and setup additions come in later tasks.)

- [ ] **Step 2: Build the app**

Run: `npm run tauri:build` (or `cd src-tauri && cargo build`)

This will take a few minutes the first time. Expected: builds successfully. If it fails with a permission error at runtime, the capabilities file from Task 5 is the fix.

- [ ] **Step 3: Smoke test that the app still launches**

Run: `npm run tauri:dev`

Expected: app boots normally, tray icon appears, Settings opens. No update behavior yet — we haven't added it. Just verifying no regression.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat(updater): register tauri-plugin-updater"
```

---

## Phase 2 — `update_manager.rs` state & pure-function logic (TDD)

This phase builds the foundation: state types, the `should_check` predicate, and the skip-clearing logic. All pure functions, all easy to unit-test.

### Task 7: Create `update_manager.rs` skeleton with state types

**Files:**
- Create: `src-tauri/src/update_manager.rs`
- Modify: `src-tauri/src/main.rs` (add `mod update_manager;`)

- [ ] **Step 1: Create the new module**

Create `src-tauri/src/update_manager.rs` with:

```rust
//! In-app update orchestration. Owns scheduling, persistent state
//! (skip / snooze / auto-check toggle), and command handling. See
//! `docs/superpowers/specs/2026-05-25-in-app-updates-design.md`.

use serde::{Deserialize, Serialize};

/// User-facing snapshot of update state. Returned by `update_get_state`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UpdateState {
    pub last_check_unix: Option<i64>,
    pub latest_known: Option<UpdateInfo>,
    pub auto_check_enabled: bool,
    pub skipped_version: Option<String>,
    pub snoozed_until_unix: Option<i64>,
}

impl Default for UpdateState {
    fn default() -> Self {
        Self {
            last_check_unix: None,
            latest_known: None,
            auto_check_enabled: true,
            skipped_version: None,
            snoozed_until_unix: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
    pub mandatory: bool,
}

/// Predicate: should the background scheduler trigger a check now?
///
/// Returns true iff:
/// - auto-check is enabled, AND
/// - no check has happened in the last 24h (or never), AND
/// - we are not currently snoozed
pub fn should_check(state: &UpdateState, now_unix: i64) -> bool {
    if !state.auto_check_enabled {
        return false;
    }
    let last_ok = match state.last_check_unix {
        None => true,
        Some(t) => now_unix - t > 24 * 60 * 60,
    };
    let snooze_ok = match state.snoozed_until_unix {
        None => true,
        Some(t) => now_unix > t,
    };
    last_ok && snooze_ok
}

/// After a check returns `new_version`, decide whether to clear the
/// persisted skip. Skip is cleared when the new version is strictly
/// greater than the skipped one (semver-ish string compare for simple
/// `MAJOR.MINOR.PATCH`; the actual updater uses real semver internally).
pub fn skip_cleared_by(skipped: &Option<String>, new_version: &str) -> bool {
    match skipped {
        None => false,
        Some(s) => version_gt(new_version, s),
    }
}

/// Returns true if `a > b` under simple dotted-integer comparison.
/// Both strings expected to be `N.N.N` (optionally with a `-suffix`
/// that is ignored for comparison purposes).
fn version_gt(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.split('-').next().unwrap_or("")
            .split('.')
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect()
    };
    let pa = parse(a);
    let pb = parse(b);
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st() -> UpdateState {
        UpdateState::default()
    }

    const NOW: i64 = 1_800_000_000;
    const DAY: i64 = 24 * 60 * 60;
}
```

- [ ] **Step 2: Register the module in `main.rs`**

In `src-tauri/src/main.rs`, find the `mod` declarations near the top:

```rust
mod audio_capture;
mod commands;
mod tray;
```

Add:

```rust
mod audio_capture;
mod commands;
mod tray;
mod update_manager;
```

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check --locked`

Expected: passes. (No warnings for unused code — the helpers above are `pub`, the tests module is `cfg(test)`.)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/update_manager.rs src-tauri/src/main.rs
git commit -m "feat(update_manager): skeleton state types + should_check predicate"
```

### Task 8: TDD `should_check` — write failing tests first

**Files:**
- Modify: `src-tauri/src/update_manager.rs` (extend `tests` module)

- [ ] **Step 1: Add the failing tests**

In `src-tauri/src/update_manager.rs`, replace the `tests` module with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn st() -> UpdateState {
        UpdateState::default()
    }

    const NOW: i64 = 1_800_000_000;
    const DAY: i64 = 24 * 60 * 60;

    #[test]
    fn should_check_when_never_checked_and_auto_on() {
        assert!(should_check(&st(), NOW));
    }

    #[test]
    fn should_not_check_when_auto_off() {
        let s = UpdateState {
            auto_check_enabled: false,
            ..Default::default()
        };
        assert!(!should_check(&s, NOW));
    }

    #[test]
    fn should_not_check_when_recently_checked() {
        let s = UpdateState {
            last_check_unix: Some(NOW - 10 * 60),
            ..Default::default()
        };
        assert!(!should_check(&s, NOW));
    }

    #[test]
    fn should_check_when_check_is_25h_old() {
        let s = UpdateState {
            last_check_unix: Some(NOW - 25 * 60 * 60),
            ..Default::default()
        };
        assert!(should_check(&s, NOW));
    }

    #[test]
    fn should_not_check_while_snoozed() {
        let s = UpdateState {
            snoozed_until_unix: Some(NOW + 60 * 60),
            ..Default::default()
        };
        assert!(!should_check(&s, NOW));
    }

    #[test]
    fn should_check_after_snooze_expires() {
        let s = UpdateState {
            snoozed_until_unix: Some(NOW - 60),
            ..Default::default()
        };
        assert!(should_check(&s, NOW));
    }

    #[test]
    fn skip_cleared_by_higher_version() {
        let skipped = Some("0.3.0".to_string());
        assert!(skip_cleared_by(&skipped, "0.3.1"));
        assert!(skip_cleared_by(&skipped, "0.4.0"));
        assert!(skip_cleared_by(&skipped, "1.0.0"));
    }

    #[test]
    fn skip_not_cleared_by_same_or_lower() {
        let skipped = Some("0.3.0".to_string());
        assert!(!skip_cleared_by(&skipped, "0.3.0"));
        assert!(!skip_cleared_by(&skipped, "0.2.9"));
    }

    #[test]
    fn skip_cleared_when_none_returns_false() {
        assert!(!skip_cleared_by(&None, "1.0.0"));
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cd src-tauri && cargo test update_manager`

Expected: all tests PASS. (The functions were written in Task 7 along with the skeleton; this task is the test coverage that pins their behavior. If anything fails, the bug is in `should_check` or `version_gt` — fix before continuing.)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/update_manager.rs
git commit -m "test(update_manager): cover should_check + skip semantics"
```

### Task 9: Persistence helpers — load/save state via tauri-plugin-store

**Files:**
- Modify: `src-tauri/src/update_manager.rs`

This task wires `UpdateState` to the existing `settings.json` store under namespaced keys (`update.*`). No automated test — we'd be mostly testing the store plugin itself. Manual smoke test at the end of the phase.

- [ ] **Step 1: Add the persistence functions**

Append to `src-tauri/src/update_manager.rs` (above the `#[cfg(test)]` block):

```rust
use serde_json::Value;
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "settings.json";

const KEY_AUTO_CHECK: &str = "update.auto_check_enabled";
const KEY_SKIPPED: &str = "update.skipped_version";
const KEY_SNOOZED_UNTIL: &str = "update.snoozed_until_unix";
const KEY_LAST_CHECK: &str = "update.last_check_unix";

/// Read persisted state from `settings.json`. Missing keys fall back
/// to `UpdateState::default()`. `latest_known` is in-memory only — it
/// is intentionally not persisted (we re-check on startup anyway).
pub fn load_state<R: Runtime>(app: &AppHandle<R>) -> UpdateState {
    let mut state = UpdateState::default();
    let Ok(store) = app.store(STORE_FILE) else {
        return state;
    };
    if let Some(Value::Bool(b)) = store.get(KEY_AUTO_CHECK) {
        state.auto_check_enabled = b;
    }
    if let Some(Value::String(s)) = store.get(KEY_SKIPPED) {
        state.skipped_version = Some(s);
    }
    if let Some(Value::Number(n)) = store.get(KEY_SNOOZED_UNTIL) {
        state.snoozed_until_unix = n.as_i64();
    }
    if let Some(Value::Number(n)) = store.get(KEY_LAST_CHECK) {
        state.last_check_unix = n.as_i64();
    }
    state
}

/// Persist the four mutable fields. `latest_known` is not written.
pub fn save_state<R: Runtime>(app: &AppHandle<R>, state: &UpdateState) {
    let Ok(store) = app.store(STORE_FILE) else {
        return;
    };
    store.set(KEY_AUTO_CHECK, Value::Bool(state.auto_check_enabled));
    match &state.skipped_version {
        Some(v) => store.set(KEY_SKIPPED, Value::String(v.clone())),
        None => { store.delete(KEY_SKIPPED); }
    }
    match state.snoozed_until_unix {
        Some(t) => store.set(KEY_SNOOZED_UNTIL, Value::Number(t.into())),
        None => { store.delete(KEY_SNOOZED_UNTIL); }
    }
    match state.last_check_unix {
        Some(t) => store.set(KEY_LAST_CHECK, Value::Number(t.into())),
        None => { store.delete(KEY_LAST_CHECK); }
    }
    let _ = store.save();
}
```

- [ ] **Step 2: Verify compilation**

Run: `cd src-tauri && cargo check --locked`

Expected: passes.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/update_manager.rs
git commit -m "feat(update_manager): persist state via tauri-plugin-store"
```

---

## Phase 3 — Commands

### Task 10: Add shared state + the `update_check` command

**Files:**
- Modify: `src-tauri/src/update_manager.rs`

- [ ] **Step 1: Add a `Manager` struct that wraps shared state behind a Mutex**

Append to `src-tauri/src/update_manager.rs` (above the `#[cfg(test)]` block):

```rust
use std::sync::Mutex;
use tauri::{Emitter, Manager as _};
use tauri_plugin_updater::UpdaterExt;

pub struct Manager {
    state: Mutex<UpdateState>,
    in_flight: Mutex<bool>,
}

impl Manager {
    pub fn new(initial: UpdateState) -> Self {
        Self {
            state: Mutex::new(initial),
            in_flight: Mutex::new(false),
        }
    }

    pub fn snapshot(&self) -> UpdateState {
        self.state.lock().unwrap().clone()
    }
}

fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Read the `mandatory` flag from the manifest's raw JSON. Missing
/// or non-boolean values are treated as `false`.
fn extract_mandatory(raw: &serde_json::Value) -> bool {
    raw.get("mandatory")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Run a check. If `force` is false and policy says not to (auto-check
/// off, snoozed, or last check too recent), returns early with no
/// events. On success, may emit `update://checking`, `update://available`,
/// or `update://none`. Errors emit `update://error` only if `force=true`
/// (silent for the background path).
pub async fn run_check<R: Runtime>(app: AppHandle<R>, force: bool) {
    {
        let mgr = app.state::<Manager>();
        let mut in_flight = mgr.in_flight.lock().unwrap();
        if *in_flight {
            return;
        }
        let state = mgr.state.lock().unwrap();
        if !force && !should_check(&state, now_unix()) {
            return;
        }
        *in_flight = true;
    }

    let _ = app.emit("update://checking", ());

    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => {
            finish_check(&app, force, Err(e.to_string()));
            return;
        }
    };

    let result = updater.check().await;

    match result {
        Ok(Some(update)) => {
            let mandatory = extract_mandatory(&update.raw_json);
            let version = update.version.clone();
            let notes = update.body.clone().unwrap_or_default();

            let info = UpdateInfo {
                version: version.clone(),
                notes: notes.clone(),
                mandatory,
            };

            // Decide: is this version actually one we should show?
            let show = {
                let mgr = app.state::<Manager>();
                let mut state = mgr.state.lock().unwrap();
                state.last_check_unix = Some(now_unix());
                state.latest_known = Some(info.clone());

                // Clear skip if this is a newer version than the skipped one.
                if skip_cleared_by(&state.skipped_version, &version) {
                    state.skipped_version = None;
                }

                // Decide whether to show:
                // - force=true → always show (manual check overrides skip)
                // - mandatory → always show
                // - otherwise → show iff this version is not the skipped one
                let suppressed = !force
                    && !mandatory
                    && state.skipped_version.as_deref() == Some(version.as_str());

                save_state(&app, &state);
                !suppressed
            };

            if show {
                let _ = app.emit(
                    "update://available",
                    serde_json::json!({
                        "version": version,
                        "notes": notes,
                        "mandatory": mandatory,
                    }),
                );
                open_update_window(&app);
            } else {
                let _ = app.emit("update://none", ());
            }
            finish_check(&app, force, Ok(()));
        }
        Ok(None) => {
            {
                let mgr = app.state::<Manager>();
                let mut state = mgr.state.lock().unwrap();
                state.last_check_unix = Some(now_unix());
                state.latest_known = None;
                save_state(&app, &state);
            }
            let _ = app.emit("update://none", ());
            finish_check(&app, force, Ok(()));
        }
        Err(e) => {
            finish_check(&app, force, Err(e.to_string()));
        }
    }
}

fn finish_check<R: Runtime>(app: &AppHandle<R>, force: bool, result: Result<(), String>) {
    let mgr = app.state::<Manager>();
    *mgr.in_flight.lock().unwrap() = false;
    if let Err(msg) = result {
        eprintln!("[update_manager] check failed: {msg}");
        if force {
            let _ = app.emit("update://error", serde_json::json!({ "message": msg }));
        }
    }
}

/// Open the update window if it doesn't already exist; focus it if it does.
/// Mandatory updates intercept the close button.
fn open_update_window<R: Runtime>(app: &AppHandle<R>) {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    if let Some(win) = app.get_webview_window("update") {
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }

    let mandatory = {
        let mgr = app.state::<Manager>();
        let s = mgr.state.lock().unwrap();
        s.latest_known.as_ref().map(|i| i.mandatory).unwrap_or(false)
    };

    let result = WebviewWindowBuilder::new(
        app,
        "update",
        WebviewUrl::App("/#/update".into()),
    )
    .title("")
    .inner_size(420.0, 360.0)
    .resizable(false)
    .center()
    .skip_taskbar(true)
    .build();

    if let Ok(win) = result {
        if mandatory {
            let win_clone = win.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = win_clone.set_focus();
                }
            });
        }
    }
}

#[tauri::command]
pub async fn update_check<R: Runtime>(app: AppHandle<R>, force: bool) {
    run_check(app, force).await;
}

#[tauri::command]
pub fn update_get_state<R: Runtime>(app: AppHandle<R>) -> UpdateState {
    app.state::<Manager>().snapshot()
}
```

- [ ] **Step 2: Build and verify**

Run: `cd src-tauri && cargo check --locked`

Expected: passes. (If `app.updater()` doesn't resolve, the import of `UpdaterExt` is missing — confirm `use tauri_plugin_updater::UpdaterExt;` is at the top of the new block.)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/update_manager.rs
git commit -m "feat(update_manager): update_check command + window opener"
```

### Task 11: Implement `update_install_and_relaunch` with progress events

**Files:**
- Modify: `src-tauri/src/update_manager.rs`

- [ ] **Step 1: Add the command**

Append to `src-tauri/src/update_manager.rs` (still above the `#[cfg(test)]` block):

```rust
#[tauri::command]
pub async fn update_install_and_relaunch<R: Runtime>(
    app: AppHandle<R>,
) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No update available".to_string())?;

    let app_for_progress = app.clone();
    let mut total: Option<u64> = None;
    let mut downloaded: u64 = 0;

    update
        .download_and_install(
            move |chunk_length, content_length| {
                downloaded += chunk_length as u64;
                if total.is_none() {
                    total = content_length;
                }
                let _ = app_for_progress.emit(
                    "update://download-progress",
                    serde_json::json!({
                        "downloaded": downloaded,
                        "total": total,
                    }),
                );
            },
            || {
                // download complete; install begins
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    app.restart();
}
```

Note: `app.restart()` does not return — its return type is `!`. Hence the function ends here.

- [ ] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check --locked`

Expected: passes.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/update_manager.rs
git commit -m "feat(update_manager): install_and_relaunch with progress events"
```

### Task 12: Implement `update_skip_version` and `update_snooze`

**Files:**
- Modify: `src-tauri/src/update_manager.rs`

- [ ] **Step 1: Add the commands**

Append to `src-tauri/src/update_manager.rs`:

```rust
#[tauri::command]
pub fn update_skip_version<R: Runtime>(
    app: AppHandle<R>,
    version: String,
) -> Result<(), String> {
    let mgr = app.state::<Manager>();
    let mut state = mgr.state.lock().unwrap();
    if let Some(info) = &state.latest_known {
        if info.mandatory {
            return Err("Cannot skip a mandatory update".to_string());
        }
    }
    state.skipped_version = Some(version);
    save_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn update_snooze<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let mgr = app.state::<Manager>();
    let mut state = mgr.state.lock().unwrap();
    if let Some(info) = &state.latest_known {
        if info.mandatory {
            return Err("Cannot snooze a mandatory update".to_string());
        }
    }
    state.snoozed_until_unix = Some(now_unix() + 24 * 60 * 60);
    save_state(&app, &state);
    Ok(())
}
```

- [ ] **Step 2: Add a setter for the auto-check toggle (used by Settings)**

Append:

```rust
#[tauri::command]
pub fn update_set_auto_check<R: Runtime>(app: AppHandle<R>, enabled: bool) {
    let mgr = app.state::<Manager>();
    let mut state = mgr.state.lock().unwrap();
    state.auto_check_enabled = enabled;
    save_state(&app, &state);
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check --locked`

Expected: passes.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/update_manager.rs
git commit -m "feat(update_manager): skip / snooze / set_auto_check commands"
```

### Task 13: Register commands and shared state in `main.rs`

**Files:**
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Register state and commands**

In `src-tauri/src/main.rs`, modify the builder block:

1. Add the manager's initial state to the app via `.manage()`. This must happen inside `setup()` because we need `app.handle()` to call `load_state`.
2. Add the five new commands to `generate_handler!`.

Update `invoke_handler` to include the new commands:

```rust
        .invoke_handler(tauri::generate_handler![
            commands::google_sign_in,
            commands::check_session,
            commands::sign_out,
            commands::api_request,
            commands::upload_file,
            commands::set_tray_recording,
            commands::create_waveform_window,
            commands::destroy_waveform_window,
            commands::create_settings_window,
            commands::put_presigned,
            audio_capture::start_system_audio_capture,
            audio_capture::stop_system_audio_capture,
            update_manager::update_check,
            update_manager::update_install_and_relaunch,
            update_manager::update_skip_version,
            update_manager::update_snooze,
            update_manager::update_set_auto_check,
            update_manager::update_get_state,
        ])
```

Inside `.setup(|app| { ... })`, add `.manage()` immediately after the `tray::create_tray(app.handle())?;` line:

```rust
            tray::create_tray(app.handle())?;

            let initial_state = update_manager::load_state(&app.handle());
            app.manage(update_manager::Manager::new(initial_state));
```

- [ ] **Step 2: Build and smoke test**

Run: `npm run tauri:dev`

Expected: app boots normally, tray opens, Settings opens, no panic. The commands are registered but nothing calls them yet.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat(updater): wire update_manager state + commands into main"
```

---

## Phase 4 — Background scheduler

### Task 14: Spawn the periodic check task

**Files:**
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add the scheduler at the end of `setup()`**

In `src-tauri/src/main.rs`, inside `.setup(|app| { ... })`, after the `settings.on_window_event(...)` block and before `Ok(())`, add:

```rust
            // Background update scheduler: wake every hour, but only
            // actually check once per 24h (or on snooze expiry). The
            // initial 10-second delay lets startup finish first.
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                loop {
                    update_manager::run_check(app_handle.clone(), false).await;
                    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                }
            });
```

- [ ] **Step 2: Build and verify the timer starts**

Run: `npm run tauri:dev`

Expected: app launches; nothing visible happens (no update is published yet). To confirm the scheduler runs, temporarily add `eprintln!("[update_manager] tick");` at the top of `run_check` and observe stdout. Remove the print before committing.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat(updater): background scheduler (10s + hourly tick)"
```

---

## Phase 5 — Frontend update window

### Task 15: Add the `/update` route

**Files:**
- Modify: `src/main.ts`

- [ ] **Step 1: Register the route**

Replace `src/main.ts` with:

```ts
import './assets/main.css';
import { setupPluginListeners } from 'tauri-plugin-mcp';
import { createApp } from 'vue';
import { createRouter, createWebHashHistory } from 'vue-router';
import App from './App.vue';
import WaveformView from './views/WaveformView.vue';
import SettingsView from './views/SettingsView.vue';
import UpdateView from './views/UpdateView.vue';

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', name: 'Bootstrap', component: { template: '' } },
    { path: '/waveform', name: 'Waveform', component: WaveformView },
    { path: '/settings', name: 'Settings', component: SettingsView },
    { path: '/update', name: 'Update', component: UpdateView },
  ],
});

const app = createApp(App);
app.use(router);
app.mount('#app');

setupPluginListeners();
```

`UpdateView.vue` is created in Task 17 — this step will fail to type-check until then. That's expected; we'll verify the build at the end of the phase.

- [ ] **Step 2: No commit until UpdateView exists**

### Task 16: Add updater bindings to `src/tauri.ts`

**Files:**
- Modify: `src/tauri.ts`

- [ ] **Step 1: Add an `updater` namespace**

Append to `src/tauri.ts`, after the existing `api` export:

```ts
export interface UpdateInfo {
  version: string;
  notes: string;
  mandatory: boolean;
}

export interface UpdateStateSnapshot {
  last_check_unix: number | null;
  latest_known: UpdateInfo | null;
  auto_check_enabled: boolean;
  skipped_version: string | null;
  snoozed_until_unix: number | null;
}

export const updater = {
  check(force = false): Promise<void> {
    return invoke('update_check', { force });
  },
  installAndRelaunch(): Promise<void> {
    return invoke('update_install_and_relaunch');
  },
  skipVersion(version: string): Promise<void> {
    return invoke('update_skip_version', { version });
  },
  snooze(): Promise<void> {
    return invoke('update_snooze');
  },
  setAutoCheck(enabled: boolean): Promise<void> {
    return invoke('update_set_auto_check', { enabled });
  },
  getState(): Promise<UpdateStateSnapshot> {
    return invoke('update_get_state');
  },
};
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `npx tsc --noEmit -p .`

(If the project doesn't have a `tsc` script, this is the simplest invocation. The `tsconfig.json` is at the repo root.)

Expected: no errors in `src/tauri.ts`. There will still be errors about the missing `UpdateView` import until Task 17 lands.

### Task 17: Create `UpdateView.vue` (modern card layout)

**Files:**
- Create: `src/views/UpdateView.vue`

- [ ] **Step 1: Write the component**

Create `src/views/UpdateView.vue`:

```vue
<template>
  <div class="update-window">
    <img class="app-icon" src="../assets/ariso-logo-w.png" alt="" />

    <h1 class="title">Update Available</h1>

    <div class="subtitle">
      <span class="version-chip">{{ info.version }}</span>
      <span class="dot">·</span>
      <span>You have {{ currentVersion }}</span>
    </div>

    <div class="notes-card">
      <div class="notes-title">What's New</div>
      <div class="notes-body" v-html="renderedNotes"></div>
    </div>

    <div v-if="downloadState === 'idle' && downloadError" class="error-line">
      {{ downloadError }}
    </div>

    <div v-if="downloadState === 'downloading'" class="progress-row">
      <div class="progress-track">
        <div class="progress-fill" :style="{ width: progressPct + '%' }"></div>
      </div>
      <div class="progress-label">{{ progressPct }}%</div>
    </div>

    <div v-if="downloadState === 'idle'" class="actions">
      <div class="left-actions">
        <a
          v-if="!info.mandatory"
          href="#"
          @click.prevent="onSkip"
          class="link-action"
        >Skip</a>
        <a
          v-if="!info.mandatory"
          href="#"
          @click.prevent="onLater"
          class="link-action"
        >Later</a>
      </div>
      <button class="install-btn" @click="onInstall">Install Update</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { updater, type UpdateInfo } from '../tauri';

const currentVersion = __APP_VERSION__;

const info = ref<UpdateInfo>({ version: '', notes: '', mandatory: false });
const downloadState = ref<'idle' | 'downloading'>('idle');
const downloadError = ref('');
const downloaded = ref(0);
const total = ref<number | null>(null);

const progressPct = computed(() => {
  if (!total.value || total.value === 0) return 0;
  return Math.min(100, Math.floor((downloaded.value / total.value) * 100));
});

// Render Markdown-ish release notes. We deliberately do not pull in a
// full Markdown parser — GitHub release bodies are bullet-list-heavy
// and a tiny renderer covers 99% of cases without the dependency cost.
const renderedNotes = computed(() => {
  const text = info.value.notes || '_No release notes provided._';
  return text
    .split('\n')
    .map((line) => {
      const trimmed = line.trim();
      if (trimmed.startsWith('- ') || trimmed.startsWith('* ')) {
        return `<div class="bullet">• ${escapeHtml(trimmed.slice(2))}</div>`;
      }
      if (trimmed.startsWith('### ')) {
        return `<div class="h3">${escapeHtml(trimmed.slice(4))}</div>`;
      }
      if (trimmed.startsWith('## ')) {
        return `<div class="h2">${escapeHtml(trimmed.slice(3))}</div>`;
      }
      if (trimmed === '') return '<br/>';
      return `<div>${escapeHtml(trimmed)}</div>`;
    })
    .join('');
});

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

let unlistenProgress: UnlistenFn | null = null;
let unlistenAvailable: UnlistenFn | null = null;

onMounted(async () => {
  // Initial state (covers the case where the window is opened from
  // Settings → "Show Details" after the event already fired).
  const snap = await updater.getState();
  if (snap.latest_known) {
    info.value = snap.latest_known;
  }

  // Stay in sync if a fresh check fires while we're open.
  unlistenAvailable = await listen<UpdateInfo>('update://available', (e) => {
    info.value = e.payload;
  });

  unlistenProgress = await listen<{ downloaded: number; total: number | null }>(
    'update://download-progress',
    (e) => {
      downloaded.value = e.payload.downloaded;
      total.value = e.payload.total;
    }
  );
});

onUnmounted(() => {
  unlistenProgress?.();
  unlistenAvailable?.();
});

async function onInstall() {
  downloadError.value = '';
  downloadState.value = 'downloading';
  downloaded.value = 0;
  total.value = null;
  try {
    await updater.installAndRelaunch();
    // App restarts; this never returns.
  } catch (e) {
    downloadState.value = 'idle';
    downloadError.value =
      e instanceof Error ? e.message : 'Download interrupted. Try again?';
  }
}

async function onSkip() {
  await updater.skipVersion(info.value.version);
  await getCurrentWindow().close();
}

async function onLater() {
  await updater.snooze();
  await getCurrentWindow().close();
}
</script>

<style scoped>
.update-window {
  background: white;
  padding: 22px 22px 18px;
  font-family: -apple-system, system-ui, sans-serif;
  min-height: 100vh;
  display: flex;
  flex-direction: column;
}

.app-icon {
  width: 64px;
  height: 64px;
  border-radius: 14px;
  align-self: center;
  box-shadow: 0 4px 12px rgba(99, 102, 241, 0.25);
  margin-bottom: 12px;
}

.title {
  font-size: 16px;
  font-weight: 700;
  color: #1d1d1f;
  text-align: center;
  margin: 0 0 4px 0;
}

.subtitle {
  font-size: 12px;
  color: #86868b;
  text-align: center;
  margin-bottom: 16px;
}

.version-chip {
  background: #eef2ff;
  color: #4f46e5;
  padding: 1px 7px;
  border-radius: 4px;
  font-weight: 600;
}

.dot {
  margin: 0 5px;
}

.notes-card {
  background: #f5f5f7;
  border-radius: 8px;
  padding: 12px 14px;
  flex: 1;
  overflow-y: auto;
  font-size: 12px;
  line-height: 1.6;
  color: #1d1d1f;
  max-height: 140px;
}

.notes-title {
  font-weight: 600;
  margin-bottom: 6px;
  font-size: 12px;
}

.notes-body .bullet { padding-left: 4px; }
.notes-body .h2    { font-weight: 700; margin: 6px 0 2px; }
.notes-body .h3    { font-weight: 600; margin: 4px 0 2px; }

.error-line {
  margin-top: 10px;
  font-size: 12px;
  color: #dc2626;
  text-align: center;
}

.progress-row {
  margin-top: 16px;
  display: flex;
  align-items: center;
  gap: 10px;
}

.progress-track {
  flex: 1;
  height: 6px;
  background: #e5e7eb;
  border-radius: 3px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: linear-gradient(to right, #6366f1, #4f46e5);
  transition: width 0.2s;
}

.progress-label {
  font-size: 11px;
  color: #6b7280;
  width: 32px;
  text-align: right;
}

.actions {
  margin-top: 16px;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.left-actions {
  display: flex;
  gap: 14px;
}

.link-action {
  font-size: 12px;
  color: #86868b;
  text-decoration: none;
  cursor: pointer;
}

.link-action:hover {
  color: #1d1d1f;
}

.install-btn {
  font-size: 13px;
  padding: 6px 18px;
  border-radius: 6px;
  border: none;
  background: linear-gradient(to bottom, #6366f1, #4f46e5);
  color: white;
  font-weight: 600;
  cursor: pointer;
}

.install-btn:hover {
  filter: brightness(1.05);
}
</style>
```

- [ ] **Step 2: Build the frontend to confirm types resolve**

Run: `npm run vite:build`

Expected: build succeeds. (The route registration from Task 15 is now valid.)

- [ ] **Step 3: Commit Tasks 15-17 together**

```bash
git add src/main.ts src/tauri.ts src/views/UpdateView.vue
git commit -m "feat(ui): UpdateView modern card + /update route + updater bindings"
```

---

## Phase 6 — Settings → About section

### Task 18: Replace the About section in `SettingsView.vue`

**Files:**
- Modify: `src/views/SettingsView.vue`

- [ ] **Step 1: Update the template's About section**

In `src/views/SettingsView.vue`, replace the existing About section:

```vue
    <!-- About Section -->
    <section class="section">
      <h2 class="section-title">About</h2>
      <div class="card">
        <span class="about-text">Ariso v{{ appVersion }}</span>
      </div>
    </section>
```

with:

```vue
    <!-- About / Updates Section -->
    <section class="section">
      <h2 class="section-title">About</h2>
      <div class="card">
        <div class="about-header">
          <span class="version-text">Ariso {{ appVersion }}</span>
          <span class="status-line" :class="statusClass">
            {{ statusText }}
          </span>
        </div>

        <div class="update-controls">
          <button
            v-if="updateAvailable"
            class="primary-btn"
            @click="showUpdateDetails"
          >Show Details</button>
          <button
            v-else
            class="secondary-btn"
            :disabled="checking"
            @click="checkNow"
          >{{ checking ? 'Checking…' : 'Check for Updates' }}</button>
        </div>

        <label class="auto-check-row">
          <input
            type="checkbox"
            :checked="autoCheck"
            @change="onToggleAutoCheck"
          />
          <span>Automatically check for updates</span>
        </label>

        <div v-if="updateError" class="error">{{ updateError }}</div>
      </div>
    </section>
```

- [ ] **Step 2: Add the matching `<script setup>` state**

Add this import to the existing `<script setup lang="ts">` import block:

```ts
import { updater } from '../tauri';
```

Add new reactive state immediately after the existing `const appVersion = __APP_VERSION__;` line:

```ts
const checking = ref(false);
const autoCheck = ref(true);
const updateAvailable = ref(false);
const updateAvailableVersion = ref('');
const updateError = ref('');
const lastCheckUnix = ref<number | null>(null);

const statusText = computed(() => {
  if (checking.value) return 'Checking…';
  if (updateAvailable.value) return `Update available: ${updateAvailableVersion.value}`;
  if (lastCheckUnix.value == null) return "You haven't checked yet.";
  const ago = humanizeAgo(Date.now() / 1000 - lastCheckUnix.value);
  return `You're up to date. Last checked: ${ago}`;
});

const statusClass = computed(() => {
  if (updateAvailable.value) return 'status-available';
  if (checking.value) return 'status-checking';
  return 'status-ok';
});

function humanizeAgo(secs: number): string {
  if (secs < 60) return 'just now';
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
}

async function loadUpdateState() {
  const snap = await updater.getState();
  autoCheck.value = snap.auto_check_enabled;
  lastCheckUnix.value = snap.last_check_unix;
  if (snap.latest_known) {
    updateAvailable.value = true;
    updateAvailableVersion.value = snap.latest_known.version;
  } else {
    updateAvailable.value = false;
    updateAvailableVersion.value = '';
  }
}

async function checkNow() {
  checking.value = true;
  updateError.value = '';
  try {
    await updater.check(true);
  } catch (e) {
    updateError.value = e instanceof Error ? e.message : String(e);
  } finally {
    checking.value = false;
    await loadUpdateState();
  }
}

async function showUpdateDetails() {
  // Re-run check with force=true; the Rust side opens (or focuses) the
  // update window. This is simpler than calling a separate "open window"
  // command and ensures the data shown is current.
  await updater.check(true);
}

async function onToggleAutoCheck(e: Event) {
  const checked = (e.target as HTMLInputElement).checked;
  autoCheck.value = checked;
  await updater.setAutoCheck(checked);
}
```

- [ ] **Step 3: Wire event listeners + initial load**

Find the existing `onMounted(async () => { ... })` block. At the end of it, add:

```ts
  await loadUpdateState();

  const unAvail = await listen('update://available', async () => {
    await loadUpdateState();
  });
  const unNone = await listen('update://none', () => {
    updateAvailable.value = false;
    updateAvailableVersion.value = '';
    lastCheckUnix.value = Math.floor(Date.now() / 1000);
  });
  const unChecking = await listen('update://checking', () => {
    checking.value = true;
  });
  const unError = await listen<{ message: string }>('update://error', (e) => {
    updateError.value = e.payload.message;
    checking.value = false;
  });

  // Save unlisteners so onUnmounted can clear them.
  unlistenUpdates = [unAvail, unNone, unChecking, unError];
```

Declare `let unlistenUpdates: UnlistenFn[] = [];` near the top of the `<script setup>` block (next to the existing `let unlistenSignInPrompt`).

Find the existing `onUnmounted` block and add:

```ts
  unlistenUpdates.forEach((un) => un());
```

- [ ] **Step 4: Add the matching CSS**

Append to the existing `<style scoped>` block:

```css
.about-header {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 12px;
}

.version-text {
  font-size: 14px;
  font-weight: 500;
  color: #1d1d1f;
}

.status-line {
  font-size: 12px;
}

.status-ok       { color: #16a34a; }
.status-checking { color: #86868b; }
.status-available { color: #4f46e5; font-weight: 500; }

.update-controls {
  margin-bottom: 12px;
}

.primary-btn {
  font-size: 13px;
  padding: 5px 14px;
  border-radius: 6px;
  border: none;
  background: linear-gradient(to bottom, #6366f1, #4f46e5);
  color: white;
  font-weight: 500;
  cursor: pointer;
}

.secondary-btn {
  font-size: 13px;
  padding: 5px 14px;
  border-radius: 6px;
  border: 1px solid #d1d5db;
  background: white;
  color: #1d1d1f;
  cursor: pointer;
}

.secondary-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.auto-check-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: #1d1d1f;
  cursor: pointer;
}
```

- [ ] **Step 5: Build and verify**

Run: `npm run vite:build`

Expected: build succeeds. Then `npm run tauri:dev`, open Settings from the tray. You should see "Ariso 0.2.0" + "You haven't checked yet." + a "Check for Updates" button + the auto-check checkbox.

Clicking "Check for Updates" will produce an error because no `latest.json` exists yet at the configured endpoint — that's expected. The button should return to its normal state and surface the error inline.

- [ ] **Step 6: Commit**

```bash
git add src/views/SettingsView.vue
git commit -m "feat(ui): Settings About section — update status + manual check + auto-check toggle"
```

---

## Phase 7 — Tray menu

### Task 19: Add "Check for Updates…" to the tray menu

**Files:**
- Modify: `src-tauri/src/tray.rs`

- [ ] **Step 1: Add the menu item to both idle and recording menus, and handle the click**

In `src-tauri/src/tray.rs`:

1. In `build_idle_menu`, add a new item before the `quit` separator:

```rust
pub fn build_idle_menu(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let start = MenuItemBuilder::with_id("start_recording", "Start Recording").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings...").build(app)?;
    let check_updates = MenuItemBuilder::with_id("check_updates", "Check for Updates…").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit Ariso").build(app)?;

    MenuBuilder::new(app)
        .item(&start)
        .separator()
        .item(&settings)
        .item(&check_updates)
        .separator()
        .item(&quit)
        .build()
}
```

2. In `build_recording_menu`, add the same item below `settings`:

```rust
pub fn build_recording_menu(app: &AppHandle, is_paused: bool) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let pause_or_resume = if is_paused {
        MenuItemBuilder::with_id("resume_recording", "Resume Recording").build(app)?
    } else {
        MenuItemBuilder::with_id("pause_recording", "Pause Recording").build(app)?
    };
    let stop = MenuItemBuilder::with_id("stop_recording", "Stop Recording").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings...").build(app)?;
    let check_updates = MenuItemBuilder::with_id("check_updates", "Check for Updates…").build(app)?;

    MenuBuilder::new(app)
        .item(&pause_or_resume)
        .item(&stop)
        .separator()
        .item(&settings)
        .item(&check_updates)
        .build()
}
```

3. In the `on_menu_event` block, add a new arm before `"quit"`:

```rust
                "check_updates" => {
                    let app_async = app.clone();
                    tauri::async_runtime::spawn(async move {
                        crate::update_manager::run_check(app_async, true).await;
                    });
                }
```

- [ ] **Step 2: Build and smoke test**

Run: `npm run tauri:dev`

Expected: tray menu shows "Check for Updates…" in both idle and recording modes. Clicking it triggers a check (will fail with no manifest yet — that's fine for now).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/tray.rs
git commit -m "feat(tray): add Check for Updates… menu item"
```

---

## Phase 8 — CI: publish updater artifacts on each release

### Task 20: Generate `latest.json` and upload it with each release

**Files:**
- Modify: `.github/workflows/desktop.yaml`

- [ ] **Step 1: Export signing env vars to the release job**

In `.github/workflows/desktop.yaml`, find the `release` job's `env:` block and add two entries:

```yaml
    env:
      APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
      APPLE_ID: ${{ secrets.APPLE_ID }}
      APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }}
      APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
      TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
```

- [ ] **Step 2: Add a "Generate latest.json" step**

In the same `release` job, add a new step between `Build, sign, and notarize` and `Attach signed DMG to GitHub Release`:

```yaml
      - name: Generate latest.json updater manifest
        env:
          RELEASE_TAG: ${{ github.event.release.tag_name }}
          RELEASE_BODY: ${{ github.event.release.body }}
          REPO: ${{ github.repository }}
        run: |
          set -euo pipefail

          # Locate the updater artifacts the bundler produced. Tauri v2
          # writes these to src-tauri/target/release/bundle/macos/.
          TARBALL=$(ls src-tauri/target/release/bundle/macos/*.app.tar.gz)
          SIGFILE="${TARBALL}.sig"
          BASENAME=$(basename "$TARBALL")

          # The version in tauri.conf.json (strip leading 'v' from tag).
          VERSION="${RELEASE_TAG#v}"

          # Asset URL is the release-asset download URL on GitHub.
          ASSET_URL="https://github.com/${REPO}/releases/download/${RELEASE_TAG}/${BASENAME}"

          # Read the detached signature contents (single line of base64).
          SIG=$(cat "$SIGFILE")

          # Mandatory flag: derived from the release title containing "[mandatory]".
          # Reads the published release title via the GitHub CLI.
          TITLE=$(gh release view "$RELEASE_TAG" --json name --jq .name)
          if [[ "$TITLE" == *"[mandatory]"* ]]; then
            MANDATORY="true"
          else
            MANDATORY="false"
          fi

          # Build the manifest using jq to ensure valid JSON escaping
          # of the (possibly multi-line) release body.
          jq -n \
            --arg version "$VERSION" \
            --arg notes "$RELEASE_BODY" \
            --arg pub_date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
            --argjson mandatory "$MANDATORY" \
            --arg signature "$SIG" \
            --arg url "$ASSET_URL" \
            '{
              version: $version,
              notes: $notes,
              pub_date: $pub_date,
              mandatory: $mandatory,
              platforms: {
                "darwin-aarch64": {
                  signature: $signature,
                  url: $url
                }
              }
            }' > latest.json

          # Also copy the tarball and sig to repo root so the upload glob
          # below picks them up alongside the DMG.
          cp "$TARBALL" "$SIGFILE" .
        env:
          GH_TOKEN: ${{ github.token }}
```

- [ ] **Step 3: Extend the upload step to include the new files**

Modify the existing `Attach signed DMG to GitHub Release` step's `files:` field:

```yaml
      - name: Attach signed DMG + updater manifest to GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          tag_name: ${{ github.event.release.tag_name }}
          files: |
            src-tauri/target/release/bundle/dmg/*.dmg
            *.app.tar.gz
            *.app.tar.gz.sig
            latest.json
          fail_on_unmatched_files: true
```

- [ ] **Step 4: Verify YAML syntax**

Run from the repo root:

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/desktop.yaml'))"
```

Expected: no error.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/desktop.yaml
git commit -m "ci(desktop): generate latest.json + upload updater artifacts"
```

---

## Phase 9 — Manual end-to-end test plan

### Task 21: Document the manual E2E test procedure

**Files:**
- Create: `docs/superpowers/specs/2026-05-25-in-app-updates-e2e.md`

- [ ] **Step 1: Write the procedure**

Create `docs/superpowers/specs/2026-05-25-in-app-updates-e2e.md`:

```markdown
# In-App Updates — Manual End-to-End Test

This procedure must pass before declaring the updater feature complete.
The plugin handles the cryptography, but our orchestration (skip,
snooze, mandatory, window opening, progress) is only proven by running
a real install against a real GitHub release.

## Prerequisites

- A real signed v0.2.0 build of Ariso installed in `/Applications/Ariso.app`.
- The Ed25519 signing keypair from Phase 1, Task 1.
- A throwaway GitHub repo or branch where you can publish a test
  release at `v0.2.1-test`.

## Test releases

For each scenario, edit `tauri.conf.json` to point at the throwaway
endpoint, run `npm run tauri:build`, and install the resulting DMG
manually. Or: stage the test artifacts in the real repo behind a
pre-release tag (e.g., `v0.2.1-test`), publish, observe, then delete
the release.

## Scenarios

### 1. Happy path

1. Install v0.2.0.
2. Publish test release v0.2.1-test (non-mandatory) with notes.
3. Launch Ariso. Within 10 seconds, the update window should appear
   showing the correct version, notes, and "You have 0.2.0" subtitle.
4. Click **Install Update**. Progress bar advances to 100%.
5. App relaunches as v0.2.1-test. Verify in Settings → About.

### 2. Skip This Version

1. From the happy-path window, click **Skip**.
2. Wait 1 minute, then trigger another automatic check by editing
   `update.last_check_unix` in `~/Library/Application Support/ai.ariso.desktop/settings.json`
   to be 25 hours in the past.
3. Within 1 hour the scheduler ticks and runs a check. Expected:
   **no dialog appears** because v0.2.1-test is skipped.
4. From the tray menu, click **Check for Updates…**. Expected:
   dialog **does appear** because the manual path bypasses skip.
5. Publish v0.2.2-test. Trigger an automatic check. Expected:
   dialog **does appear** (newer version clears the skip).

### 3. Remind Me Later

1. From the update window, click **Later**.
2. Open Settings → About. Status should be "You're up to date" (no
   re-prompt expected).
3. Edit `update.last_check_unix` to be 25 hours in the past.
4. Force-trigger an auto-check (restart the app, wait 10s).
5. Expected: no dialog because snooze hasn't expired.
6. Edit `update.snoozed_until_unix` to be in the past. Re-trigger.
7. Expected: dialog appears.

### 4. Mandatory update

1. Publish a test release with title containing `[mandatory]`.
2. Launch Ariso. Update window appears within 10 seconds.
3. Expected: **no Skip or Later links** visible.
4. Try to close the window via the red traffic-light button.
5. Expected: window does not close (stays focused).
6. Click **Install Update**. Verify install + relaunch.

### 5. Bad signature

1. Publish a test release with a deliberately corrupted `.sig` file
   (e.g., a single random base64 string).
2. Launch Ariso. Click through to install.
3. Expected: install fails. Update window shows "Couldn't download
   update. Try again?" inline. Settings → About surfaces an error.

### 6. Offline

1. Disconnect from network.
2. Launch Ariso. Open Settings → About.
3. Click **Check for Updates**.
4. Expected: status returns to idle with an inline network error.
   No dialog appears.
5. Reconnect. Click again. Expected: normal behavior.

## Cleanup

Delete the throwaway test releases. Revert `tauri.conf.json` if you
pointed it at a test endpoint.
```

- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-05-25-in-app-updates-e2e.md
git commit -m "docs: manual E2E test procedure for in-app updates"
```

### Task 22: Run the E2E test plan

- [ ] **Step 1: Execute the procedure in `docs/superpowers/specs/2026-05-25-in-app-updates-e2e.md`**

All six scenarios must pass. If any fails, debug and fix the underlying issue — do not modify the test procedure to make it pass.

- [ ] **Step 2: No commit — this task only succeeds when all scenarios pass**

Once they do, the feature is complete. Cut a real (non-test) release using the existing process documented in `README.md` to ship to users.

---

## Self-review notes (post-write)

The plan covers every section of the spec:

- **Spec §1 Architecture** → Tasks 7–14 (`update_manager.rs`, scheduler)
- **Spec §2 Release artifact + signing** → Tasks 1, 4, 20 (keypair, config, CI)
- **Spec §3 update_manager.rs** → Tasks 7–14
- **Spec §4.1 Update window** → Tasks 15–17
- **Spec §4.2 Settings → About** → Task 18
- **Spec §4.3 Tray menu** → Task 19
- **Spec §5 Testing** → Tasks 8 (unit), 21–22 (E2E)
- **Spec §6 Error handling** → covered inline in Tasks 10 (silent auto-fail, surfaced manual-fail), 11 (download error → reset button), 17 (UI for download error)
- **Spec §7 Edge cases** → covered by `first_run_after_update` not needing special code (the `last_check` write inside `run_check`), `app.restart()` semantics in Task 11, mandatory close-intercept in Task 10's `open_update_window`

One spec divergence, called out at the top of the File map: `tauri-plugin-process` is not added (YAGNI; we only relaunch from Rust where `app.restart()` is core API). If this proves wrong during E2E, add it back via a one-line `Cargo.toml` change.
