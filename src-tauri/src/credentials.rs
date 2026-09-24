//! Bring-your-own API keys for the remote notes models.
//!
//! A key is a secret, so it never touches `plugin-store`'s plaintext
//! `settings.json` (`oats-security` item #4) — it lives in the OS credential
//! store: Keychain Services on macOS, Credential Manager on Windows, both
//! reached through the `keyring` crate's platform-selecting `v1` interface.
//!
//! The webview can set, count and clear keys but can never read one back: the
//! value leaves the keychain only on the Rust side, at the moment a provider
//! request is built.
//!
//! See docs/superpowers/specs/2026-09-19-local-notes-model-picker-design.md.

use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The app's bundle identifier, so keychain entries are attributed to oats and
/// are visible as one group in Keychain Access.
const KEYCHAIN_SERVICE: &str = "ai.ariso.desktop";

/// The single account every provider's key lives under.
const KEYS_ACCOUNT: &str = "llm-api-keys";

/// Far longer than any provider key in circulation, and short enough that a
/// runaway paste can't be pushed into the keychain.
const MAX_KEY_LEN: usize = 4096;

/// Windows Credential Manager caps a credential's secret at
/// `CRED_MAX_CREDENTIAL_BLOB_SIZE` (2560 bytes), and `keyring` writes a
/// password as UTF-16 — so the whole serialized key map, every provider's key
/// together, gets this many UTF-16 units. Real keys (well under 200 chars
/// each) fit comfortably; only a runaway paste can reach it.
const WINDOWS_MAX_SECRET_UTF16: usize = 2560 / 2;

/// Provider wire name -> key. The only shape stored in the consolidated
/// keychain item.
type KeyMap = BTreeMap<String, String>;

/// A provider whose API can generate notes. A closed set: the value names a
/// keychain account (and, later, a request URL), so free text must never
/// reach either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RemoteProvider {
    OpenAi,
    Gemini,
    Anthropic,
}

impl RemoteProvider {
    pub const ALL: [RemoteProvider; 3] = [Self::OpenAi, Self::Gemini, Self::Anthropic];

    /// The provider's wire name — the frontend's value, the keychain account
    /// suffix, and the prefix a note's model tag carries.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Gemini => "gemini",
            Self::Anthropic => "anthropic",
        }
    }

    /// The old (pre-consolidation) per-provider keychain account name.
    /// Migration-only: nothing writes here anymore.
    fn legacy_account(self) -> String {
        format!("llm-api-key:{}", self.as_str())
    }
}

/// Bound and sanitize a pasted key. Errors describe the problem without ever
/// quoting the key itself.
fn validate_key(raw: &str) -> Result<&str, String> {
    let key = raw.trim();
    if key.is_empty() {
        return Err("Enter an API key.".to_string());
    }
    if key.chars().count() > MAX_KEY_LEN {
        return Err("That API key is longer than any provider issues.".to_string());
    }
    // A CR/LF would split the Authorization header this key is destined for;
    // no provider key contains control characters in the first place.
    if key.chars().any(char::is_control) {
        return Err("That API key contains characters a key can't hold.".to_string());
    }
    Ok(key)
}

/// Parse the consolidated item's stored value. Missing, empty, or invalid
/// JSON all mean "no keys stored" rather than an error — a corrupted
/// credential must not break Settings or notes generation.
fn parse_keys(raw: &str) -> KeyMap {
    serde_json::from_str(raw).unwrap_or_default()
}

fn serialize_keys(keys: &KeyMap) -> String {
    serde_json::to_string(keys).unwrap_or_default()
}

/// The platform store's cap on the consolidated item's value, in UTF-16
/// units, or `None` where the cap is far beyond anything we'd store (the
/// macOS Keychain).
fn store_capacity() -> Option<usize> {
    cfg!(windows).then_some(WINDOWS_MAX_SECRET_UTF16)
}

/// Reject a serialized key map the store can't hold with a message the user
/// can act on, instead of `keyring`'s opaque `TooLong`. Never quotes `raw`.
fn ensure_fits(raw: &str, max_utf16_units: usize) -> Result<(), String> {
    if raw.encode_utf16().count() > max_utf16_units {
        return Err(
            "Together, your API keys are longer than Windows Credential Manager \
                    can store. Remove a key you no longer use and try again."
                .to_string(),
        );
    }
    Ok(())
}

/// The user-facing name of this platform's credential store.
const STORE_NAME: &str = if cfg!(windows) {
    "Windows Credential Manager"
} else {
    "The system keychain"
};

/// Turn a credential-store failure into something a user can act on, without
/// echoing whatever was being stored.
fn store_error(err: keyring::Error) -> String {
    match err {
        keyring::Error::NoStorageAccess(_) if cfg!(windows) => {
            "Windows Credential Manager denied access. Try again.".to_string()
        }
        keyring::Error::NoStorageAccess(_) => {
            "The system keychain is locked. Unlock it and try again.".to_string()
        }
        other => format!("{STORE_NAME} refused the request ({other})."),
    }
}

/// The keychain service to file entries under. Tests use a service of their
/// own: the round-trip test writes and deletes real entries, and must never be
/// able to touch the ones the app stored for the person running the suite.
fn keychain_service() -> &'static str {
    #[cfg(test)]
    {
        "ai.ariso.desktop.tests"
    }
    #[cfg(not(test))]
    {
        KEYCHAIN_SERVICE
    }
}

/// The single keychain entry every provider's key lives under.
fn consolidated_entry() -> Result<Entry, String> {
    // `keyring` initializes the platform store on first use; surface a failure
    // here rather than reporting a key as saved when nothing persisted it.
    Entry::store_status()
        .as_ref()
        .map_err(|e| format!("This device has no usable credential store ({e})."))?;
    Entry::new(keychain_service(), KEYS_ACCOUNT).map_err(store_error)
}

/// A pre-consolidation per-provider entry. Migration-only.
fn legacy_entry(provider: RemoteProvider) -> Result<Entry, String> {
    Entry::new(keychain_service(), &provider.legacy_account()).map_err(store_error)
}

fn load_keys() -> Result<KeyMap, String> {
    #[cfg(test)]
    if testing::store_is_active() {
        return Ok(testing::stored_map());
    }
    match consolidated_entry()?.get_password() {
        Ok(raw) => Ok(parse_keys(&raw)),
        Err(keyring::Error::NoEntry) => Ok(KeyMap::new()),
        Err(e) => Err(store_error(e)),
    }
}

fn save_keys(keys: &KeyMap) -> Result<(), String> {
    let raw = serialize_keys(keys);
    // Checked before the store is touched, so an over-budget save leaves the
    // stored keys exactly as they were.
    if let Some(max) = store_capacity() {
        ensure_fits(&raw, max)?;
    }
    consolidated_entry()?
        .set_password(&raw)
        .map_err(store_error)
}

fn set_api_key(provider: RemoteProvider, key: String) -> Result<(), String> {
    let key = validate_key(&key)?.to_string();
    let mut keys = load_keys()?;
    keys.insert(provider.as_str().to_string(), key);
    save_keys(&keys)
}

/// The stored key, or `None` when the user has not connected this provider.
/// Crate-internal on purpose — no `#[tauri::command]` exposes a key's value.
pub fn get_api_key(provider: RemoteProvider) -> Result<Option<String>, String> {
    Ok(load_keys()?.get(provider.as_str()).cloned())
}

/// Idempotent: removing a key that isn't there leaves the user's intent
/// ("no key stored") satisfied, without ever touching the keychain.
fn clear_api_key(provider: RemoteProvider) -> Result<(), String> {
    let mut keys = load_keys()?;
    if keys.remove(provider.as_str()).is_none() {
        return Ok(());
    }
    if keys.is_empty() {
        // No keys left: remove the item rather than leave an empty `{}`.
        match consolidated_entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(store_error(e)),
        }
    } else {
        save_keys(&keys)
    }
}

fn connected_providers() -> Result<Vec<RemoteProvider>, String> {
    let keys = load_keys()?;
    Ok(RemoteProvider::ALL
        .into_iter()
        .filter(|p| keys.contains_key(p.as_str()))
        .collect())
}

/// One-time upgrade: move each pre-consolidation per-provider keychain item
/// into the single combined item, then remove the old item. Idempotent (a
/// present consolidated item short-circuits immediately) and best-effort per
/// provider, so it's safe to call on every startup.
pub fn migrate_legacy_keys() -> Result<(), String> {
    let consolidated = consolidated_entry()?;
    match consolidated.get_password() {
        Ok(_) => return Ok(()),
        Err(keyring::Error::NoEntry) => {}
        Err(e) => return Err(store_error(e)),
    }

    let mut keys = KeyMap::new();
    let mut to_delete = Vec::new();
    for provider in RemoteProvider::ALL {
        let legacy = legacy_entry(provider)?;
        if let Ok(value) = legacy.get_password() {
            if let Ok(valid) = validate_key(&value) {
                keys.insert(provider.as_str().to_string(), valid.to_string());
            }
            to_delete.push(legacy);
        }
    }
    if !keys.is_empty() {
        save_keys(&keys)?;
    }
    // Only remove the old items once the combined item safely holds their
    // values (or held nothing worth keeping) — never delete a legacy item
    // before its value is durably migrated. If `save_keys` above failed,
    // this line is never reached and every legacy item survives untouched
    // for the next launch to retry.
    for legacy in to_delete {
        let _ = legacy.delete_credential();
    }
    Ok(())
}

/// Store a provider's API key. The value is never returned to a webview again.
#[tauri::command]
pub fn set_llm_api_key(provider: RemoteProvider, key: String) -> Result<(), String> {
    set_api_key(provider, key)
}

/// Which providers have a key stored — all Settings needs to render
/// "Connected" without ever seeing a key.
#[tauri::command]
pub fn llm_api_key_providers() -> Result<Vec<RemoteProvider>, String> {
    connected_providers()
}

/// Forget a provider's API key.
#[tauri::command]
pub fn clear_llm_api_key(provider: RemoteProvider) -> Result<(), String> {
    clear_api_key(provider)
}

/// Test-only stand-in for the OS keychain, so tests covering the code paths
/// that *read* a key never touch the developer's real credential store. The
/// keychain itself is exercised by the ignored round-trip test below.
#[cfg(test)]
pub(crate) mod testing {
    use super::{KeyMap, RemoteProvider};
    use std::cell::Cell;

    static KEYS: std::sync::RwLock<Vec<(RemoteProvider, String)>> =
        std::sync::RwLock::new(Vec::new());

    thread_local! {
        // Off by default: the fake store is authoritative so a test that
        // forgets to stand in a key reads "no key" instead of whatever the
        // developer's real keychain happens to hold.
        static USE_REAL_KEYCHAIN: Cell<bool> = Cell::new(false);
    }

    pub(crate) fn store_is_active() -> bool {
        USE_REAL_KEYCHAIN.with(|mode| !mode.get())
    }

    pub(crate) struct RealKeychainGuard {
        previous: bool,
    }

    /// Opt this thread out of the fake store for the guard's lifetime, for the
    /// one test that means to hit the real OS keychain.
    pub(crate) fn use_real_keychain() -> RealKeychainGuard {
        USE_REAL_KEYCHAIN.with(|mode| RealKeychainGuard {
            previous: mode.replace(true),
        })
    }

    impl Drop for RealKeychainGuard {
        fn drop(&mut self) {
            USE_REAL_KEYCHAIN.with(|mode| mode.set(self.previous));
        }
    }

    pub(crate) fn stored_map() -> KeyMap {
        KEYS.read()
            .map(|keys| {
                keys.iter()
                    .map(|(p, k)| (p.as_str().to_string(), k.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn set_key(provider: RemoteProvider, key: &str) {
        if let Ok(mut keys) = KEYS.write() {
            keys.retain(|(p, _)| *p != provider);
            keys.push((provider, key.to_string()));
        }
    }

    pub(crate) fn clear_keys() {
        if let Ok(mut keys) = KEYS.write() {
            keys.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_round_trips_through_its_wire_name() {
        for (provider, wire) in [
            (RemoteProvider::OpenAi, "\"openai\""),
            (RemoteProvider::Gemini, "\"gemini\""),
            (RemoteProvider::Anthropic, "\"anthropic\""),
        ] {
            assert_eq!(serde_json::to_string(&provider).unwrap(), wire);
            assert_eq!(
                serde_json::from_str::<RemoteProvider>(wire).unwrap(),
                provider
            );
        }
    }

    #[test]
    fn a_provider_this_build_does_not_ship_is_rejected() {
        // The provider decides a keychain account name; only the closed set may
        // ever reach it.
        assert!(serde_json::from_str::<RemoteProvider>("\"mistral\"").is_err());
        assert!(serde_json::from_str::<RemoteProvider>("\"../../openai\"").is_err());
    }

    #[test]
    fn each_provider_has_a_distinct_legacy_account_name() {
        // Migration depends on these matching exactly what prior releases
        // wrote under the old one-item-per-provider scheme.
        let accounts: Vec<String> = RemoteProvider::ALL
            .iter()
            .map(|p| p.legacy_account())
            .collect();
        assert_eq!(
            accounts,
            vec![
                "llm-api-key:openai",
                "llm-api-key:gemini",
                "llm-api-key:anthropic"
            ]
        );
    }

    #[test]
    fn a_pasted_key_loses_its_surrounding_whitespace() {
        assert_eq!(validate_key("  sk-abc123\n").unwrap(), "sk-abc123");
    }

    #[test]
    fn an_empty_key_is_rejected() {
        assert!(validate_key("").is_err());
        assert!(validate_key("   \n\t ").is_err());
    }

    #[test]
    fn a_key_carrying_control_characters_is_rejected() {
        // A CR/LF inside the key would let it split the Authorization header of
        // the provider request this key is destined for.
        assert!(validate_key("sk-abc\r\nX-Evil: 1").is_err());
        assert!(validate_key("sk-abc\ndef").is_err());
        assert!(validate_key("sk-abc\u{0}def").is_err());
    }

    #[test]
    fn an_implausibly_long_key_is_rejected() {
        assert!(validate_key(&"k".repeat(MAX_KEY_LEN)).is_ok());
        assert!(validate_key(&"k".repeat(MAX_KEY_LEN + 1)).is_err());
    }

    #[test]
    fn a_rejected_key_never_appears_in_the_error() {
        let secret = "sk-live-do-not-leak\nX-Evil: 1";
        let err = validate_key(secret).unwrap_err();
        assert!(!err.contains("sk-live"), "error leaked the key: {err}");
    }

    #[test]
    fn keys_round_trip_through_json() {
        let mut keys = KeyMap::new();
        keys.insert("openai".to_string(), "sk-abc".to_string());
        keys.insert("anthropic".to_string(), "sk-def".to_string());
        let raw = serialize_keys(&keys);
        assert_eq!(parse_keys(&raw), keys);
    }

    #[test]
    fn missing_or_corrupted_stored_json_reads_as_no_keys() {
        assert_eq!(parse_keys(""), KeyMap::new());
        assert_eq!(parse_keys("not json"), KeyMap::new());
        assert_eq!(parse_keys("[\"openai\"]"), KeyMap::new());
    }

    #[test]
    fn windows_budget_is_credential_managers_blob_cap_in_utf16_units() {
        // CRED_MAX_CREDENTIAL_BLOB_SIZE is 2560 bytes and `keyring` writes a
        // password as UTF-16, two bytes per unit.
        assert_eq!(WINDOWS_MAX_SECRET_UTF16 * 2, 2560);
    }

    #[test]
    fn only_windows_caps_the_stored_key_map() {
        #[cfg(windows)]
        assert_eq!(store_capacity(), Some(WINDOWS_MAX_SECRET_UTF16));
        #[cfg(not(windows))]
        assert_eq!(store_capacity(), None);
    }

    #[test]
    fn a_key_map_at_the_budget_fits_and_one_unit_over_does_not() {
        assert!(ensure_fits(&"k".repeat(10), 10).is_ok());
        assert!(ensure_fits(&"k".repeat(11), 10).is_err());
    }

    #[test]
    fn the_budget_counts_utf16_units_not_chars() {
        // An astral-plane character is one char but two UTF-16 units, which is
        // what Credential Manager's byte cap actually measures.
        assert!(ensure_fits("😀", 1).is_err());
        assert!(ensure_fits("😀", 2).is_ok());
    }

    #[test]
    fn an_over_budget_error_never_quotes_a_key() {
        let raw = serialize_keys(&KeyMap::from([(
            "openai".to_string(),
            format!("sk-live-do-not-leak{}", "k".repeat(2000)),
        )]));
        let err = ensure_fits(&raw, WINDOWS_MAX_SECRET_UTF16).unwrap_err();
        assert!(!err.contains("sk-live"), "error leaked the key: {err}");
        assert!(err.contains("Windows Credential Manager"), "{err}");
    }

    /// Runs on the windows-latest CI job: the budget is enforced before the
    /// store is ever reached, so this never writes a real credential.
    #[cfg(windows)]
    #[test]
    fn saving_keys_over_the_windows_budget_fails_before_touching_the_store() {
        testing::clear_keys();
        let err = set_api_key(RemoteProvider::OpenAi, "k".repeat(2000)).unwrap_err();
        assert!(err.contains("Windows Credential Manager"), "{err}");
    }

    #[test]
    fn a_store_error_names_this_platforms_credential_store() {
        let err = store_error(keyring::Error::PlatformFailure("boom".into()));
        #[cfg(windows)]
        assert!(
            err.contains("Windows Credential Manager") && !err.contains("keychain"),
            "{err}"
        );
        #[cfg(not(windows))]
        assert!(err.contains("keychain"), "{err}");
    }

    #[test]
    fn clearing_an_unset_provider_never_touches_the_keychain() {
        testing::clear_keys();
        // No key stored for any provider; clearing one must short-circuit
        // before ever reaching the keychain, not attempt to delete an item
        // that was never created.
        assert!(clear_api_key(RemoteProvider::OpenAi).is_ok());
    }

    #[test]
    #[ignore = "writes to the real OS keychain"]
    fn multiple_providers_share_a_single_keychain_item() {
        let _real_keychain = testing::use_real_keychain();
        for provider in RemoteProvider::ALL {
            let _ = legacy_entry(provider).unwrap().delete_credential();
        }
        let _ = consolidated_entry().unwrap().delete_credential();

        set_api_key(RemoteProvider::OpenAi, "sk-openai".to_string()).unwrap();
        set_api_key(RemoteProvider::Anthropic, "sk-anthropic".to_string()).unwrap();

        // Exactly one keychain item backs both keys.
        let raw = consolidated_entry().unwrap().get_password().unwrap();
        let keys = parse_keys(&raw);
        assert_eq!(keys.get("openai").map(String::as_str), Some("sk-openai"));
        assert_eq!(
            keys.get("anthropic").map(String::as_str),
            Some("sk-anthropic")
        );
        assert_eq!(keys.len(), 2);

        // No legacy per-provider item was created by either set.
        assert!(matches!(
            legacy_entry(RemoteProvider::OpenAi).unwrap().get_password(),
            Err(keyring::Error::NoEntry)
        ));

        // Clearing one key preserves the other.
        clear_api_key(RemoteProvider::OpenAi).unwrap();
        assert_eq!(get_api_key(RemoteProvider::OpenAi).unwrap(), None);
        assert_eq!(
            get_api_key(RemoteProvider::Anthropic).unwrap().as_deref(),
            Some("sk-anthropic")
        );

        // Clearing the last remaining key removes the keychain item entirely.
        clear_api_key(RemoteProvider::Anthropic).unwrap();
        assert!(matches!(
            consolidated_entry().unwrap().get_password(),
            Err(keyring::Error::NoEntry)
        ));
    }

    #[test]
    #[ignore = "writes to the real OS keychain"]
    fn legacy_per_provider_entries_are_migrated_into_one_item_and_removed() {
        let _real_keychain = testing::use_real_keychain();
        let _ = consolidated_entry().unwrap().delete_credential();
        for provider in RemoteProvider::ALL {
            let _ = legacy_entry(provider).unwrap().delete_credential();
        }

        legacy_entry(RemoteProvider::OpenAi)
            .unwrap()
            .set_password("sk-legacy-openai")
            .unwrap();
        legacy_entry(RemoteProvider::Gemini)
            .unwrap()
            .set_password("sk-legacy-gemini")
            .unwrap();

        migrate_legacy_keys().unwrap();

        assert_eq!(
            get_api_key(RemoteProvider::OpenAi).unwrap().as_deref(),
            Some("sk-legacy-openai")
        );
        assert_eq!(
            get_api_key(RemoteProvider::Gemini).unwrap().as_deref(),
            Some("sk-legacy-gemini")
        );
        assert_eq!(get_api_key(RemoteProvider::Anthropic).unwrap(), None);

        // The legacy items are gone; one consolidated item now holds both keys.
        assert!(matches!(
            legacy_entry(RemoteProvider::OpenAi).unwrap().get_password(),
            Err(keyring::Error::NoEntry)
        ));
        assert!(matches!(
            legacy_entry(RemoteProvider::Gemini).unwrap().get_password(),
            Err(keyring::Error::NoEntry)
        ));
        let raw = consolidated_entry().unwrap().get_password().unwrap();
        assert_eq!(parse_keys(&raw).len(), 2);

        // Safe to run again on every launch: already migrated, so a no-op.
        migrate_legacy_keys().unwrap();
        assert_eq!(
            get_api_key(RemoteProvider::OpenAi).unwrap().as_deref(),
            Some("sk-legacy-openai")
        );

        clear_api_key(RemoteProvider::OpenAi).unwrap();
        clear_api_key(RemoteProvider::Gemini).unwrap();
    }

    #[test]
    #[ignore = "writes to the real OS keychain"]
    fn an_invalid_legacy_value_is_dropped_but_its_stale_item_still_removed() {
        let _real_keychain = testing::use_real_keychain();
        let _ = consolidated_entry().unwrap().delete_credential();
        for provider in RemoteProvider::ALL {
            let _ = legacy_entry(provider).unwrap().delete_credential();
        }

        // A CR/LF makes this fail `validate_key`, simulating corrupt legacy
        // data — migration must drop it, not propagate an error.
        legacy_entry(RemoteProvider::OpenAi)
            .unwrap()
            .set_password("sk-bad\r\nX-Evil: 1")
            .unwrap();

        migrate_legacy_keys().unwrap();

        assert_eq!(get_api_key(RemoteProvider::OpenAi).unwrap(), None);
        assert!(matches!(
            legacy_entry(RemoteProvider::OpenAi).unwrap().get_password(),
            Err(keyring::Error::NoEntry)
        ));
    }

    /// Hits the real OS credential store, so it stays out of the default run;
    /// `cargo test -- --ignored keychain_round_trip` exercises it on demand.
    #[test]
    #[ignore = "writes to the real OS keychain"]
    fn keychain_round_trip_sets_reads_and_clears_a_key() {
        let _real_keychain = testing::use_real_keychain();
        let provider = RemoteProvider::OpenAi;
        let _ = clear_api_key(provider);

        assert_eq!(get_api_key(provider).unwrap(), None);
        set_api_key(provider, " sk-round-trip \n".to_string()).unwrap();
        assert_eq!(
            get_api_key(provider).unwrap().as_deref(),
            Some("sk-round-trip")
        );
        assert_eq!(connected_providers().unwrap(), vec![provider]);

        clear_api_key(provider).unwrap();
        assert_eq!(get_api_key(provider).unwrap(), None);
        // Clearing a key that is already gone is not an error — the user's
        // intent ("no key stored") is already satisfied.
        clear_api_key(provider).unwrap();
    }
}
