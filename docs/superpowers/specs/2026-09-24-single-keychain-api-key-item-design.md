# Single Keychain item for remote-model API keys (issue #442)

## Problem

`src-tauri/src/credentials.rs` stores each bring-your-own remote-provider API
key (OpenAI, Gemini, Anthropic — `RemoteProvider::ALL`) as its own macOS
Keychain item: one shared service (`ai.ariso.desktop`) but a distinct
**account** per provider (`llm-api-key:openai`, `llm-api-key:gemini`,
`llm-api-key:anthropic`, via `RemoteProvider::account()`). A user who connects
all three providers ends up with three separate Keychain Access entries for
one app feature. Issue #442 asks for all of them to live in a single Keychain
item instead, while keeping every existing capability: per-provider read
(crate-internal only, unchanged), update, and delete, plus a safe migration
for anyone who already has the old per-provider items.

## Goal

- All configured remote-provider API keys are stored together in **one**
  macOS Keychain item (one service + one account), not one item per provider.
- `set_llm_api_key`, `llm_api_key_providers`, and `clear_llm_api_key` — the
  three `#[tauri::command]`s Settings already calls — keep their exact
  signatures and behavior. No frontend change.
- A user who already saved keys under the old per-provider items keeps them:
  a one-time, idempotent, best-effort migration (mirroring
  `vault::migrate_legacy_recordings`) moves each legacy item's value into the
  new combined item and removes the legacy item, running at app startup.
- Storage that fails to parse (corrupted/invalid content) degrades to "no
  keys stored" rather than breaking Settings or notes generation.
- Rust tests cover storage, retrieval, update, delete, and migration.

## Non-goals

- Changing the `RemoteProvider` set, the notes-model registry
  (`notes_model.rs`), or the remote request builders (`remote_notes.rs`).
- Adding a frontend-reachable "read a key's value" command — still
  deliberately absent (`oats-security`: a webview never sees a saved key).
- Changing where secrets are read at request time
  (`transcribe.rs`/`remote_notes.rs` still call
  `credentials::get_api_key(provider)`).
- Any change to `src-tauri/capabilities/`, `tauri.conf.json`, or other
  forbidden paths for this run — none of this needs them; `keyring` calls are
  plain Rust behind existing commands, same posture as today.

## Design

### Storage shape

Replace the per-provider account with one fixed account holding a JSON object
keyed by each provider's wire name (`RemoteProvider::as_str()`):

```
service: "ai.ariso.desktop"     (unchanged)
account: "llm-api-keys"         (new, replaces "llm-api-key:<provider>")
password: {"openai":"sk-...","anthropic":"sk-..."}   (JSON, only connected providers present)
```

`credentials.rs` gains a `type KeyMap = BTreeMap<String, String>` and:

- `parse_keys(raw: &str) -> KeyMap` — `serde_json::from_str`, empty map on any
  parse failure (missing/invalid/corrupted content is "no keys stored", not
  an error — satisfies the issue's "clear handling for missing or invalid
  credentials").
- `serialize_keys(&KeyMap) -> String` — `serde_json::to_string`, empty object
  on the (practically unreachable) serialize failure of a `String→String`
  map.
- `load_keys() -> Result<KeyMap, String>` — reads the consolidated entry;
  `NoEntry` becomes an empty map (migration is a separate, explicit step, not
  a lazy fallback here — see below); other `keyring::Error`s propagate through
  the existing `store_error` mapping.
- `save_keys(&KeyMap) -> Result<(), String>` — writes the serialized map to
  the consolidated entry.
- `get_api_key`, `set_api_key` (private, backs `set_llm_api_key`),
  `clear_api_key` (private, backs `clear_llm_api_key`), and
  `connected_providers` (backs `llm_api_key_providers`) all become
  load-mutate-save around `KeyMap`, keyed by `provider.as_str()`. Clearing the
  last remaining key deletes the consolidated Keychain item entirely (no
  empty-JSON item left behind); clearing an absent key stays a no-op, same
  idempotency the current `clear_api_key` already has.
- The existing `#[cfg(test)] if testing::store_is_active() { … }` short
  circuit in `get_api_key` moves down into `load_keys()` (one bypass point
  instead of one per call site), backed by a new `testing::stored_map()` that
  projects the existing `testing::KEYS` fake store into a `KeyMap`. The
  `testing` module's public surface (`set_key`, `clear_keys`,
  `use_real_keychain`, `store_is_active`) is unchanged, so
  `transcribe.rs`'s existing test seeding keeps compiling untouched.

### Migration

`RemoteProvider::account()` (the old `"llm-api-key:{provider}"` name) is
renamed `legacy_account()` and kept — migration is the only remaining caller.
A new `pub fn migrate_legacy_keys() -> Result<(), String>` in `credentials.rs`
mirrors `vault::migrate_legacy_recordings`'s shape exactly:

```rust
pub fn migrate_legacy_keys() -> Result<(), String> {
    let consolidated = consolidated_entry()?;
    match consolidated.get_password() {
        Ok(_) => return Ok(()),           // already migrated, or a key was
                                           // already saved post-migration
        Err(keyring::Error::NoEntry) => {}
        Err(e) => return Err(store_error(e)),
    }
    let mut keys = KeyMap::new();
    for provider in RemoteProvider::ALL {
        let legacy = legacy_entry(provider)?;
        if let Ok(value) = legacy.get_password() {
            if let Ok(valid) = validate_key(&value) {
                keys.insert(provider.as_str().to_string(), valid.to_string());
            }
            let _ = legacy.delete_credential(); // best-effort cleanup either way
        }
    }
    if !keys.is_empty() {
        save_keys(&keys)?;
    }
    Ok(())
}
```

Idempotent (a present consolidated item short-circuits immediately, so it's
safe to call on every launch), and best-effort per-provider: a legacy value
that fails `validate_key` (shouldn't happen — it passed validation when
originally saved — but a manually-edited Keychain entry could produce one) is
dropped rather than migrated, and its stale legacy item is still removed
either way so it doesn't linger.

Called from `main.rs`'s `.setup()` closure, alongside the other one-time
startup migrations, log-and-continue on error:

```rust
// One-time upgrade: move per-provider API-key Keychain items into the
// single combined item. Best-effort: log and continue.
if let Err(e) = crate::credentials::migrate_legacy_keys() {
    eprintln!("migrate legacy api keys: {e}");
}
```

No ordering dependency on the vault/recordings migrations already there —
placed near them for discoverability.

## Testing

- **Unit (no keychain access, run in default `cargo test`)**:
  - `parse_keys`/`serialize_keys` round-trip a multi-entry map.
  - `parse_keys` on empty/garbage input returns an empty map (the "invalid
    credential" degrade-gracefully path), not an error.
  - Existing validation tests (`validate_key`, provider (de)serialization,
    rejection of an unknown provider string) unchanged.
  - `legacy_account()` still pins the exact legacy strings
    (`llm-api-key:openai` etc.) — migration depends on these matching what
    was actually written by every prior release.
  - `get_api_key`/`set_llm_api_key` behavior against the fake `testing` store
    (already-existing pattern) extended to cover: setting two providers and
    reading both back independently; clearing one leaves the other; clearing
    a never-set provider is a no-op.
- **Ignored, real-Keychain (`cargo test -- --ignored`, unchanged convention)**:
  - Extend the existing round-trip test's spirit with: two providers set
    together live under **one** Keychain item (assert the consolidated
    entry's raw password contains both, and that no legacy per-provider item
    exists); clearing one provider preserves the other; clearing the last
    provider deletes the consolidated item.
  - New test: pre-seed two legacy per-provider items directly (bypassing the
    module, via `legacy_entry(provider).set_password(...)`), call
    `migrate_legacy_keys()`, and assert: both keys are now readable through
    `get_api_key`, the legacy items are gone, and a second
    `migrate_legacy_keys()` call is a no-op (idempotency).
- **Manual** (not verifiable in this environment — no interactive Keychain
  prompt): after upgrading a build with existing per-provider items, open
  Settings and confirm previously connected providers still show
  "Connected" with no re-entry required, and Keychain Access shows one oats
  item instead of three.

## Windows (Credential Manager)

Nothing above is macOS-only. `keyring`'s `v1` interface picks Windows
Credential Manager on Windows, so the same code keeps one generic
credential there (target `llm-api-keys.ai.ariso.desktop`), and the same
startup migration folds in the legacy `llm-api-key:<provider>.ai.ariso.desktop`
credentials.

One Windows-only constraint matters once keys share an item: Credential
Manager caps a secret at `CRED_MAX_CREDENTIAL_BLOB_SIZE` (2560 bytes), and
`keyring` writes a password as UTF-16, so the **whole serialized key map** is
limited to 1280 UTF-16 units (`WINDOWS_MAX_SECRET_UTF16`), while one key alone
may be up to `MAX_KEY_LEN` (4096). Real keys (Anthropic ~108, OpenAI ~164,
Gemini ~39 chars) fit easily; only a runaway paste reaches the cap.

- `save_keys` checks the serialized map against `store_capacity()` (Windows:
  1280; elsewhere: none) **before** touching the store, and fails with an
  actionable message naming Windows Credential Manager instead of
  `keyring`'s opaque `TooLong`. The stored keys are left exactly as they were.
- Migration gets the same check through `save_keys`: if the legacy keys don't
  fit together, it returns that error (logged at startup) and — as with any
  failed save — deletes no legacy credential, so nothing is lost.
- `store_error` names Windows Credential Manager on Windows rather than "the
  system keychain" (matching Settings' existing `keychainName`).

The budget logic is platform-neutral (`ensure_fits`) and unit-tested on every
platform; a `#[cfg(windows)]` test on the windows-latest CI job proves an
over-budget `set_api_key` fails before reaching the real store.

## Decisions

Non-interactive run (`autofix:approved`, no trusted maintainer comments on
the issue) — every question below defaults per the autopilot contract.

1. **What should the single item's account name be, replacing the three
   `llm-api-key:<provider>` accounts?**
   Default (used): `"llm-api-keys"` under the existing service
   `ai.ariso.desktop`.
2. **How should the combined item's value be structured?**
   Default (used): a JSON object mapping each provider's existing wire name
   (`RemoteProvider::as_str()`) to its key — reuses the already-closed
   provider set, no new schema to design.
3. **When does migration from the legacy per-provider items run?**
   Default (used): once at app startup, in `main.rs`'s `.setup()`, following
   this codebase's existing precedent (`vault::migrate_legacy_recordings`) —
   idempotent and best-effort, not a lazy per-read fallback.
4. **What happens to Keychain content that fails to parse (corrupted/invalid
   credential)?**
   Default (used): treated as "no keys stored" (empty map) so Settings and
   notes generation keep working; the user can simply reconnect a provider.
5. **Does the public Tauri command surface or frontend change?**
   Default (used): no — `set_llm_api_key`, `llm_api_key_providers`,
   `clear_llm_api_key` keep their exact signatures; `src/tauri.ts`,
   `SettingsView.vue`, and `notesModels.ts` are untouched.

## Acceptance criteria

- [ ] All configured remote-provider API keys are stored in one macOS
      Keychain item (verified by the ignored real-Keychain test: setting two
      providers produces one consolidated entry and no legacy entries).
- [ ] A user with existing per-provider Keychain items keeps their saved
      credentials after upgrading — `migrate_legacy_keys()` moves them into
      the combined item without requiring re-entry.
- [ ] Reading, updating, and deleting an individual provider's key works
      without creating separate per-provider items (unit + ignored tests).
- [ ] Missing or invalid/corrupted credential data is handled without
      crashing or erroring Settings — degrades to "not connected".
- [ ] Settings' "Connected" state and model selection continue to work
      unchanged after the storage-layer refactor (no frontend/command-surface
      change; covered by the existing `SettingsView.test.ts` suite staying
      green).
- [ ] Rust tests added/updated for storage, retrieval, update, deletion, and
      migration.
