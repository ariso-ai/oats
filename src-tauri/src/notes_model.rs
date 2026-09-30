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
    /// A user-supplied OpenAI-compatible endpoint. Carries no fields: only one
    /// can be configured at a time, and its base URL/model id live in
    /// `customNotesEndpoint` (settings.json) rather than in the selection
    /// itself — the same separation `Remote`'s key already has from its
    /// selection.
    Custom,
}

/// The model notes have always been written by — needs no key and no network.
const DEFAULT_LOCAL_ID: &str = "gemma-3-1b-it-qat-4bit";

const LOCAL_IDS: [&str; 1] = [DEFAULT_LOCAL_ID];

const REMOTE_MODELS: [(RemoteProvider, &str); 6] = [
    (RemoteProvider::OpenAi, "gpt-5.1"),
    (RemoteProvider::OpenAi, "gpt-5-mini"),
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
        // Whether a Custom selection can actually run is checked at call time
        // (get_custom_endpoint() + the stored key), not here — same as a
        // Remote selection's key is checked at call time, not at parse time.
        NotesModelId::Custom => true,
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
        NotesModelId::Custom => "custom".to_string(),
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

/// The non-secret half of a configured custom endpoint. The key, when
/// present, lives in the keychain (`credentials::get_custom_api_key`) — never
/// here, since this value round-trips through the plaintext `settings.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomEndpoint {
    pub base_url: String,
    pub model_id: String,
}

/// Far longer than any real model id; bounds a runaway paste the same way
/// `credentials::validate_key` bounds a pasted API key.
const MAX_MODEL_ID_LEN: usize = 4096;

/// Parse and normalize a pasted base URL. Rejects anything that is not a
/// well-formed `http`/`https` URL with a host, with a message the user can
/// act on rather than `url`'s own parser error — this is the one place in
/// notes-model settings where the value being validated is free text a user
/// is actively typing, not a hand-edited settings.json.
pub fn validate_base_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Enter a valid http:// or https:// URL.".to_string());
    }
    let mut url = reqwest::Url::parse(trimmed)
        .map_err(|_| "Enter a valid http:// or https:// URL.".to_string())?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err("Enter a valid http:// or https:// URL.".to_string());
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err("Enter a valid http:// or https:// URL.".to_string());
    }
    // Normalize: a base URL is a host, not an endpoint path. Drop any path,
    // query, or fragment a user pastes along with it (e.g. `/v1`) so
    // `remote_notes.rs` appending `v1/chat/completions` can't double up into
    // `/v1/v1/chat/completions`.
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}

/// Bound and sanitize a pasted model id. Mirrors `credentials::validate_key`'s
/// shape check, since both values are about to travel in a JSON request body.
pub fn validate_model_id(raw: &str) -> Result<String, String> {
    let id = raw.trim();
    if id.is_empty() {
        return Err("Enter a model identifier.".to_string());
    }
    if id.chars().count() > MAX_MODEL_ID_LEN {
        return Err("That model identifier is implausibly long.".to_string());
    }
    if id.chars().any(char::is_control) {
        return Err("That model identifier contains characters it can't hold.".to_string());
    }
    Ok(id.to_string())
}

static CUSTOM_ENDPOINT: RwLock<Option<CustomEndpoint>> = RwLock::new(None);

/// Test-only seam so tests can set/clear the process global directly without
/// going through settings.json.
#[cfg(test)]
fn set_custom_endpoint_global(endpoint: Option<CustomEndpoint>) {
    if let Ok(mut guard) = CUSTOM_ENDPOINT.write() {
        *guard = endpoint;
    }
}

/// Test seam: set the custom endpoint directly, bypassing settings.json, for
/// tests in other modules (`transcribe.rs`) that need `generate_notes`'s
/// `Custom` arm to see a specific endpoint.
#[cfg(test)]
pub(crate) fn testing_set_custom_endpoint(endpoint: Option<CustomEndpoint>) {
    if let Ok(mut guard) = CUSTOM_ENDPOINT.write() {
        *guard = endpoint;
    }
}

/// The configured custom endpoint, or `None` if the user hasn't set one up.
/// Readable from anywhere, including the detached notes-generation task,
/// which has no `AppHandle` and so cannot reach `settings.json` directly.
#[tauri::command]
pub fn get_custom_endpoint() -> Option<CustomEndpoint> {
    CUSTOM_ENDPOINT.read().ok().and_then(|guard| guard.clone())
}

const CUSTOM_ENDPOINT_SETTINGS_KEY: &str = "customNotesEndpoint";

/// Seed the custom endpoint from `settings.json` at startup, mirroring
/// `load_selected`.
pub fn load_custom_endpoint(app: &tauri::AppHandle) {
    use tauri_plugin_store::StoreExt as _;
    let stored = app
        .store(SETTINGS_PATH)
        .ok()
        .and_then(|store| store.get(CUSTOM_ENDPOINT_SETTINGS_KEY))
        .and_then(|value| serde_json::from_value::<CustomEndpoint>(value).ok());
    if let Ok(mut guard) = CUSTOM_ENDPOINT.write() {
        *guard = stored;
    }
}

/// Validate and persist the custom endpoint's base URL and model id. Does not
/// touch the API key — that is a separate, optional store
/// (`credentials::set_custom_llm_key`).
#[tauri::command]
pub fn set_custom_endpoint(
    app: tauri::AppHandle,
    base_url: String,
    model_id: String,
) -> Result<(), String> {
    use tauri_plugin_store::StoreExt as _;
    let endpoint = CustomEndpoint {
        base_url: validate_base_url(&base_url)?,
        model_id: validate_model_id(&model_id)?,
    };
    let store = app.store(SETTINGS_PATH).map_err(|e| e.to_string())?;
    store.set(
        CUSTOM_ENDPOINT_SETTINGS_KEY,
        serde_json::to_value(&endpoint).map_err(|e| e.to_string())?,
    );
    store.save().map_err(|e| e.to_string())?;
    if let Ok(mut guard) = CUSTOM_ENDPOINT.write() {
        *guard = Some(endpoint);
    }
    Ok(())
}

/// Remove the configured custom endpoint. Idempotent, same as clearing a key.
#[tauri::command]
pub fn clear_custom_endpoint(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_store::StoreExt as _;
    let store = app.store(SETTINGS_PATH).map_err(|e| e.to_string())?;
    store.delete(CUSTOM_ENDPOINT_SETTINGS_KEY);
    store.save().map_err(|e| e.to_string())?;
    if let Ok(mut guard) = CUSTOM_ENDPOINT.write() {
        *guard = None;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;
    static CUSTOM_ENDPOINT_TEST_LOCK: Mutex<()> = Mutex::new(());

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

    #[test]
    fn a_custom_selection_serializes_with_no_fields() {
        assert_eq!(
            serde_json::to_value(NotesModelId::Custom).unwrap(),
            json!({ "kind": "custom" })
        );
        assert_eq!(parse(&json!({ "kind": "custom" })), NotesModelId::Custom);
    }

    #[test]
    fn custom_is_always_a_registered_kind() {
        // Unlike Local/Remote, Custom names no id or provider to check against a
        // registry — whether it can actually run is checked at call time via
        // get_custom_endpoint() and the stored key, not here.
        assert!(is_registered(&NotesModelId::Custom));
    }

    #[test]
    fn custom_tags_itself_without_an_id() {
        assert_eq!(tag(&NotesModelId::Custom), "custom");
    }

    #[test]
    fn a_valid_http_and_https_base_url_is_accepted() {
        assert_eq!(
            validate_base_url("http://10.0.1.20:8000").unwrap(),
            "http://10.0.1.20:8000/"
        );
        assert_eq!(
            validate_base_url("https://dgx.local:8443").unwrap(),
            "https://dgx.local:8443/"
        );
    }

    #[test]
    fn a_trailing_slash_on_the_base_url_does_not_double_up() {
        assert_eq!(
            validate_base_url("http://10.0.1.20:8000/").unwrap(),
            "http://10.0.1.20:8000/"
        );
    }

    #[test]
    fn a_path_on_the_base_url_is_dropped_so_it_cannot_double_up_with_v1() {
        assert_eq!(
            validate_base_url("http://10.0.1.20:8000/v1").unwrap(),
            "http://10.0.1.20:8000/"
        );
    }

    #[test]
    fn a_query_string_or_fragment_on_the_base_url_is_also_dropped() {
        assert_eq!(
            validate_base_url("http://10.0.1.20:8000/v1?foo=bar").unwrap(),
            "http://10.0.1.20:8000/"
        );
        assert_eq!(
            validate_base_url("http://10.0.1.20:8000/v1#frag").unwrap(),
            "http://10.0.1.20:8000/"
        );
    }

    #[test]
    fn a_url_with_no_scheme_is_rejected_with_an_actionable_message() {
        let err = validate_base_url("10.0.1.20:8000").unwrap_err();
        assert!(err.contains("http://") && err.contains("https://"), "{err}");
    }

    #[test]
    fn a_non_http_scheme_is_rejected() {
        assert!(validate_base_url("ftp://10.0.1.20").is_err());
        assert!(validate_base_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn an_empty_or_hostless_url_is_rejected() {
        assert!(validate_base_url("").is_err());
        assert!(validate_base_url("http://").is_err());
        assert!(validate_base_url("   ").is_err());
    }

    #[test]
    fn a_model_id_round_trips_after_trimming() {
        assert_eq!(validate_model_id("  Qwen2.5-72B-Instruct \n").unwrap(), "Qwen2.5-72B-Instruct");
    }

    #[test]
    fn an_empty_or_whitespace_only_model_id_is_rejected() {
        assert!(validate_model_id("").is_err());
        assert!(validate_model_id("   \n\t ").is_err());
    }

    #[test]
    fn a_model_id_with_control_characters_is_rejected() {
        assert!(validate_model_id("Qwen\r\nX-Evil: 1").is_err());
    }

    #[test]
    fn an_implausibly_long_model_id_is_rejected() {
        assert!(validate_model_id(&"q".repeat(4096)).is_ok());
        assert!(validate_model_id(&"q".repeat(4097)).is_err());
    }

    #[test]
    fn get_custom_endpoint_is_none_until_one_is_set() {
        let _guard = CUSTOM_ENDPOINT_TEST_LOCK.lock().unwrap();
        set_custom_endpoint_global(None);
        assert_eq!(get_custom_endpoint(), None);
    }

    #[test]
    fn the_process_global_reflects_the_last_value_set() {
        let _guard = CUSTOM_ENDPOINT_TEST_LOCK.lock().unwrap();
        let endpoint = CustomEndpoint {
            base_url: "http://10.0.1.20:8000/".to_string(),
            model_id: "Qwen2.5-72B-Instruct".to_string(),
        };
        set_custom_endpoint_global(Some(endpoint.clone()));
        assert_eq!(get_custom_endpoint(), Some(endpoint));
        set_custom_endpoint_global(None);
    }
}
