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

/// The app's bundle identifier, so keychain entries are attributed to oats and
/// are visible as one group in Keychain Access.
const KEYCHAIN_SERVICE: &str = "ai.ariso.desktop";

/// Far longer than any provider key in circulation, and short enough that a
/// runaway paste can't be pushed into the keychain.
const MAX_KEY_LEN: usize = 4096;

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

    /// One keychain account per provider, so switching between two models from
    /// the same provider doesn't ask for the key again.
    fn account(self) -> String {
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

/// Turn a credential-store failure into something a user can act on, without
/// echoing whatever was being stored.
fn store_error(err: keyring::Error) -> String {
    match err {
        keyring::Error::NoStorageAccess(_) => {
            "The system keychain is locked. Unlock it and try again.".to_string()
        }
        other => format!("The system keychain refused the request ({other})."),
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

fn entry(provider: RemoteProvider) -> Result<Entry, String> {
    // `keyring` initializes the platform store on first use; surface a failure
    // here rather than reporting a key as saved when nothing persisted it.
    Entry::store_status()
        .as_ref()
        .map_err(|e| format!("This device has no usable credential store ({e})."))?;
    Entry::new(keychain_service(), &provider.account()).map_err(store_error)
}

fn set_api_key(provider: RemoteProvider, key: String) -> Result<(), String> {
    let key = validate_key(&key)?;
    entry(provider)?.set_password(key).map_err(store_error)
}

/// The stored key, or `None` when the user has not connected this provider.
/// Crate-internal on purpose — no `#[tauri::command]` exposes a key's value.
pub fn get_api_key(provider: RemoteProvider) -> Result<Option<String>, String> {
    // Tests stand in for the keychain entirely rather than reading the
    // developer's real login keychain. The ignored round-trip test opts out.
    #[cfg(test)]
    if testing::store_is_active() {
        return Ok(testing::stored_key(provider));
    }
    match entry(provider)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(store_error(e)),
    }
}

/// Idempotent: removing a key that isn't there leaves the user's intent
/// ("no key stored") satisfied.
fn clear_api_key(provider: RemoteProvider) -> Result<(), String> {
    match entry(provider)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(store_error(e)),
    }
}

fn connected_providers() -> Result<Vec<RemoteProvider>, String> {
    let mut connected = Vec::new();
    for provider in RemoteProvider::ALL {
        if get_api_key(provider)?.is_some() {
            connected.push(provider);
        }
    }
    Ok(connected)
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
    use super::RemoteProvider;
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

    pub(crate) fn stored_key(provider: RemoteProvider) -> Option<String> {
        KEYS.read()
            .ok()?
            .iter()
            .find(|(p, _)| *p == provider)
            .map(|(_, k)| k.clone())
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
    fn each_provider_gets_its_own_keychain_account() {
        let accounts: Vec<String> = RemoteProvider::ALL.iter().map(|p| p.account()).collect();
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
