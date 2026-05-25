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
