//! Which model writes meeting notes on the Local backend.
//!
//! The registry is closed: a selection names a model directory or a provider
//! URL, so only ids this build ships may ever be honoured. It mirrors
//! `src/notesModels.ts` — both sides must agree on the persisted shape.
//!
//! The choice also has to be readable from `transcribe::process_notes`, which
//! runs detached with no `AppHandle` and so cannot reach `settings.json`. As
//! with the vault directory (`vault.rs`), the value is seeded into a process
//! global at startup and updated by the command that persists it.

use crate::credentials::RemoteProvider;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::RwLock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum NotesModelId {
    /// An on-device model, run by the `ariso-stt` sidecar.
    Local { id: String },
    /// A provider's model, called over HTTPS with the user's own API key.
    Remote {
        provider: RemoteProvider,
        id: String,
    },
}

/// The model notes have always been written by — needs no key and no network.
const DEFAULT_LOCAL_ID: &str = "gemma-3-1b-it-qat-4bit";

const LOCAL_IDS: [&str; 1] = [DEFAULT_LOCAL_ID];

const REMOTE_MODELS: [(RemoteProvider, &str); 6] = [
    (RemoteProvider::OpenAi, "gpt-5.1"),
    (RemoteProvider::OpenAi, "gpt-5.1-mini"),
    (RemoteProvider::Gemini, "gemini-3.5-flash"),
    (RemoteProvider::Gemini, "gemini-3.7-flash"),
    (RemoteProvider::Anthropic, "claude-haiku-4-5"),
    (RemoteProvider::Anthropic, "claude-sonnet-5"),
];

pub fn default_model() -> NotesModelId {
    NotesModelId::Local {
        id: DEFAULT_LOCAL_ID.to_string(),
    }
}

pub fn is_registered(model: &NotesModelId) -> bool {
    match model {
        NotesModelId::Local { id } => LOCAL_IDS.contains(&id.as_str()),
        NotesModelId::Remote { provider, id } => REMOTE_MODELS
            .iter()
            .any(|(p, m)| p == provider && m == &id.as_str()),
    }
}

/// Coerce a persisted value into a model this build ships. Anything absent,
/// malformed, or unregistered falls back to the default rather than being
/// trusted.
pub fn parse(raw: &Value) -> NotesModelId {
    serde_json::from_value::<NotesModelId>(raw.clone())
        .ok()
        .filter(is_registered)
        .unwrap_or_else(default_model)
}

/// How a model names itself in `meta.json` and in the note's frontmatter, so a
/// note can say which model wrote it.
pub fn tag(model: &NotesModelId) -> String {
    match model {
        NotesModelId::Local { id } => format!("local:{id}"),
        NotesModelId::Remote { provider, id } => format!("{}:{}", provider.as_str(), id),
    }
}

static SELECTED: RwLock<Option<NotesModelId>> = RwLock::new(None);

/// The model notes generation should use. Readable from anywhere, including
/// the detached task that writes notes.
pub fn selected() -> NotesModelId {
    SELECTED
        .read()
        .ok()
        .and_then(|guard| guard.clone())
        .unwrap_or_else(default_model)
}

pub fn set_selected(model: NotesModelId) {
    if let Ok(mut guard) = SELECTED.write() {
        *guard = Some(model);
    }
}

const SETTINGS_PATH: &str = "settings.json";
const SETTINGS_KEY: &str = "notesModel";

/// Seed the selection from `settings.json` at startup, the way the vault
/// override is seeded — notes generation runs with no `AppHandle` and cannot
/// reach the store itself.
pub fn load_selected(app: &tauri::AppHandle) {
    use tauri_plugin_store::StoreExt as _;
    let stored = app
        .store(SETTINGS_PATH)
        .ok()
        .and_then(|store| store.get(SETTINGS_KEY));
    set_selected(match stored {
        Some(value) => parse(&value),
        None => default_model(),
    });
}

/// Persist the user's choice and make it live for the next notes run.
///
/// The frontend goes through here rather than writing the store directly, so
/// the process global and `settings.json` can never disagree within a session.
#[tauri::command]
pub fn set_notes_model(app: tauri::AppHandle, model: NotesModelId) -> Result<(), String> {
    use tauri_plugin_store::StoreExt as _;
    if !is_registered(&model) {
        return Err("That model isn't one this version of oats can use.".to_string());
    }
    let store = app.store(SETTINGS_PATH).map_err(|e| e.to_string())?;
    store.set(
        SETTINGS_KEY,
        serde_json::to_value(&model).map_err(|e| e.to_string())?,
    );
    store.save().map_err(|e| e.to_string())?;
    // Only after the write survives: a failed save must not leave this run
    // using a model the next launch won't remember.
    set_selected(model);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_default_is_the_on_device_model() {
        assert_eq!(
            default_model(),
            NotesModelId::Local {
                id: "gemma-3-1b-it-qat-4bit".to_string()
            }
        );
    }

    #[test]
    fn a_registered_selection_survives_a_round_trip_through_settings_json() {
        for model in [
            NotesModelId::Local {
                id: "gemma-3-1b-it-qat-4bit".to_string(),
            },
            NotesModelId::Remote {
                provider: RemoteProvider::OpenAi,
                id: "gpt-5.1".to_string(),
            },
            NotesModelId::Remote {
                provider: RemoteProvider::Anthropic,
                id: "claude-haiku-4-5".to_string(),
            },
            NotesModelId::Remote {
                provider: RemoteProvider::Gemini,
                id: "gemini-3.7-flash".to_string(),
            },
        ] {
            let json = serde_json::to_value(&model).unwrap();
            assert_eq!(parse(&json), model, "round trip of {model:?}");
        }
    }

    #[test]
    fn the_persisted_shape_is_the_one_the_frontend_writes() {
        assert_eq!(
            serde_json::to_value(NotesModelId::Remote {
                provider: RemoteProvider::OpenAi,
                id: "gpt-5.1".to_string(),
            })
            .unwrap(),
            json!({ "kind": "remote", "provider": "openai", "id": "gpt-5.1" })
        );
    }

    #[test]
    fn a_model_this_build_does_not_ship_falls_back_to_the_default() {
        // An id reaches a model directory or a provider URL, so only registered
        // ones may be honoured — a hand-edited settings.json included.
        for raw in [
            json!({ "kind": "local", "id": "../../etc/passwd" }),
            json!({ "kind": "remote", "provider": "openai", "id": "gpt-9" }),
            json!({ "kind": "remote", "provider": "mistral", "id": "large" }),
            json!({ "kind": "local" }),
            json!("gemma"),
            json!(null),
        ] {
            assert_eq!(parse(&raw), default_model(), "rejecting {raw}");
        }
    }

    #[test]
    fn gemini_ships_only_the_two_flash_models() {
        assert!(is_registered(&NotesModelId::Remote {
            provider: RemoteProvider::Gemini,
            id: "gemini-3.5-flash".to_string(),
        }));
        assert!(is_registered(&NotesModelId::Remote {
            provider: RemoteProvider::Gemini,
            id: "gemini-3.7-flash".to_string(),
        }));
        assert!(!is_registered(&NotesModelId::Remote {
            provider: RemoteProvider::Gemini,
            id: "gemini-3.0-flash".to_string(),
        }));
    }

    #[test]
    fn a_model_tags_itself_for_the_note_that_it_writes() {
        assert_eq!(
            tag(&NotesModelId::Local {
                id: "gemma-3-1b-it-qat-4bit".to_string()
            }),
            "local:gemma-3-1b-it-qat-4bit"
        );
        assert_eq!(
            tag(&NotesModelId::Remote {
                provider: RemoteProvider::OpenAi,
                id: "gpt-5.1".to_string()
            }),
            "openai:gpt-5.1"
        );
    }

    #[test]
    fn the_selection_is_readable_without_an_app_handle() {
        // `process_notes` runs detached, so it reads the choice from here
        // rather than from the store.
        let previous = selected();
        let remote = NotesModelId::Remote {
            provider: RemoteProvider::Anthropic,
            id: "claude-sonnet-5".to_string(),
        };
        set_selected(remote.clone());
        assert_eq!(selected(), remote);
        set_selected(previous);
        assert_eq!(selected(), default_model());
    }
}
