//! Which model transcribes meeting audio on the Local backend.
//!
//! Closed registry, like `notes_model.rs`: an id selects model directories and
//! a sidecar flag, so only ids this build ships are honoured. The frontend
//! persists the choice as `speech:<id>` (its catalog key) under `speechModel`.
//!
//! Transcription runs detached (checkpoints, the final pass) with no
//! `AppHandle`, so the selection is seeded into a process global at startup
//! and updated by the command that persists it.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SpeechModelId {
    #[serde(rename = "parakeet-tdt-0.6b-v3")]
    Parakeet,
    #[serde(rename = "qwen3-asr-0.6b-4bit")]
    Qwen3Asr,
}

impl SpeechModelId {
    pub const ALL: [SpeechModelId; 2] = [SpeechModelId::Parakeet, SpeechModelId::Qwen3Asr];

    pub fn id(self) -> &'static str {
        match self {
            SpeechModelId::Parakeet => "parakeet-tdt-0.6b-v3",
            SpeechModelId::Qwen3Asr => "qwen3-asr-0.6b-4bit",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }

    /// Qwen3 runs on MLX inside the macOS sidecar; Windows has no artifact.
    pub fn is_available(self) -> bool {
        match self {
            SpeechModelId::Parakeet => true,
            SpeechModelId::Qwen3Asr => cfg!(target_os = "macos"),
        }
    }
}

pub fn available() -> Vec<SpeechModelId> {
    SpeechModelId::ALL.into_iter().filter(|m| m.is_available()).collect()
}

pub fn default_model() -> SpeechModelId {
    SpeechModelId::Parakeet
}

const KEY_PREFIX: &str = "speech:";

/// Coerce a persisted `speechModel` value into a model this build can run.
/// Anything absent, malformed, unregistered, or unavailable on this platform
/// falls back to the default.
pub fn parse(raw: &Value) -> SpeechModelId {
    raw.as_str()
        .and_then(|s| s.strip_prefix(KEY_PREFIX))
        .and_then(SpeechModelId::from_id)
        .filter(|m| m.is_available())
        .unwrap_or_else(default_model)
}

/// The value written to `settings.json` — the frontend's catalog key.
pub fn settings_value(model: SpeechModelId) -> String {
    format!("{KEY_PREFIX}{}", model.id())
}

static SELECTED: RwLock<Option<SpeechModelId>> = RwLock::new(None);

pub fn selected() -> SpeechModelId {
    SELECTED.read().ok().and_then(|g| *g).unwrap_or_else(default_model)
}

pub fn set_selected(model: SpeechModelId) {
    if let Ok(mut guard) = SELECTED.write() {
        *guard = Some(model);
    }
}

const SETTINGS_PATH: &str = "settings.json";
const SETTINGS_KEY: &str = "speechModel";

pub fn load_selected(app: &tauri::AppHandle) {
    use tauri_plugin_store::StoreExt as _;
    let stored = app
        .store(SETTINGS_PATH)
        .ok()
        .and_then(|store| store.get(SETTINGS_KEY));
    set_selected(stored.as_ref().map(parse).unwrap_or_else(default_model));
}

/// Persist the choice and make it live for the next transcription. The
/// frontend goes through here so the process global and `settings.json`
/// never disagree within a session.
#[tauri::command]
pub fn set_speech_model(app: tauri::AppHandle, model: SpeechModelId) -> Result<(), String> {
    use tauri_plugin_store::StoreExt as _;
    if !model.is_available() {
        return Err("That speech model isn't available on this platform.".to_string());
    }
    let store = app.store(SETTINGS_PATH).map_err(|e| e.to_string())?;
    store.set(SETTINGS_KEY, Value::String(settings_value(model)));
    store.save().map_err(|e| e.to_string())?;
    set_selected(model);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_default_is_parakeet() {
        assert_eq!(default_model(), SpeechModelId::Parakeet);
    }

    #[test]
    fn serde_uses_the_exact_ids() {
        assert_eq!(serde_json::to_value(SpeechModelId::Parakeet).unwrap(), json!("parakeet-tdt-0.6b-v3"));
        assert_eq!(serde_json::to_value(SpeechModelId::Qwen3Asr).unwrap(), json!("qwen3-asr-0.6b-4bit"));
        assert!(serde_json::from_value::<SpeechModelId>(json!("../../etc")).is_err());
    }

    #[test]
    fn the_frontend_catalog_key_round_trips() {
        for model in available() {
            assert_eq!(parse(&json!(settings_value(model))), model);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn qwen3_is_selectable_on_macos() {
        assert_eq!(parse(&json!("speech:qwen3-asr-0.6b-4bit")), SpeechModelId::Qwen3Asr);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn qwen3_falls_back_off_macos() {
        assert_eq!(parse(&json!("speech:qwen3-asr-0.6b-4bit")), SpeechModelId::Parakeet);
    }

    #[test]
    fn unknown_or_malformed_values_fall_back_to_the_default() {
        for raw in [
            json!("speech:whisper-large-v3"),
            json!("qwen3-asr-0.6b-4bit"),
            json!("speech:../../etc"),
            json!({ "id": "qwen3-asr-0.6b-4bit" }),
            json!(null),
        ] {
            assert_eq!(parse(&raw), default_model(), "rejecting {raw}");
        }
    }

    #[test]
    fn the_selection_is_readable_without_an_app_handle() {
        let previous = selected();
        set_selected(SpeechModelId::Qwen3Asr);
        assert_eq!(selected(), SpeechModelId::Qwen3Asr);
        set_selected(previous);
    }
}
