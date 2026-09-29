# Custom remote LLM endpoint for notes (issue #456)

## Problem

The Local backend's notes model picker (`notes_model.rs`, `notesModels.ts`,
`SettingsView.vue`'s "AI Models" card) already supports one on-device model
(Gemma) and a fixed catalog of remote models from three SaaS providers —
OpenAI, Gemini, Anthropic (`notes_model.rs::REMOTE_MODELS`,
`remote_notes.rs::provider_base_url`). Both the provider's base URL and its
model-id list are closed and hardcoded; a user cannot point notes generation
at any other HTTP endpoint.

Issue #456 asks for a fourth option: a user-supplied endpoint — base URL,
model identifier, and API key — for people who run their own inference
infrastructure, the concrete example being a Qwen model served from a DGX
box. That's a real, common shape: vLLM, NVIDIA NIM, Ollama's OpenAI-compat
mode, and text-generation-webui all expose an OpenAI-style
`/v1/chat/completions` endpoint, which is exactly the request/response shape
`remote_notes.rs` already speaks for the `OpenAi` branch — the gap is that
today's code can only send that request to `api.openai.com`.

The issue is explicit that this must work for the Local backend without
requiring Ariso — this is the same backend, and the same "bring your own key"
posture, as the existing three providers; it is not a cloud-backend feature.

## Goal

From Settings, with the Local backend selected, a user can:

- Add a custom endpoint by entering a base URL, a model identifier, and an
  API key, in the same "AI Models" card the three fixed providers already
  live in.
- Test that configuration before committing to it, and get back a clear
  "connected" or a specific, actionable failure (bad URL, connection
  refused, timed out, key rejected, unexpected response).
- Edit the model identifier or base URL later without being forced to
  re-paste a key that hasn't changed.
- Remove the endpoint (URL, model id, and key together) in one action.
- See the same kind of permanent, explicit disclosure the fixed providers
  already show — this endpoint receives the meeting transcript — plus a
  clear warning when the URL isn't `https://`, since that's the realistic
  case for an on-prem box and the disclosure should say so.

Reviewable as: open Settings with Local selected, add a custom endpoint
pointing at a local OpenAI-compatible server (e.g. `vllm serve` or `ollama`
running on the LAN) with a real model id, click "Test connection" and see it
succeed, save it, record a meeting, and confirm notes generation makes
exactly one HTTPS/HTTP request to that host carrying the transcript, with no
key or transcript text in oats' own logs. Separately, confirm the three
existing fixed providers and the on-device model are completely unaffected.

## Non-goals

- **Arbitrary request/response schemas.** The custom endpoint is assumed
  OpenAI-compatible chat completions (`POST {baseUrl}/v1/chat/completions`,
  `{"model", "messages"}` in, `choices[0].message.content` out) — the same
  shape `remote_notes.rs` already builds for `RemoteProvider::OpenAi`. A
  fully generic request builder (custom headers, custom JSON paths) is out of
  scope; see [Open questions](#open-questions).
- **Multiple simultaneous custom endpoints.** One configured endpoint at a
  time, matching the issue's own wording ("a remote LLM endpoint," singular).
  Supporting a list is a straightforward extension of this design later, not
  a blocker now.
- **Changing anything about the three fixed providers or the on-device
  model.** `notes_model.rs::REMOTE_MODELS`, `RemoteProvider`, and their
  Settings rows are untouched; this adds a new option alongside them.
- **Changing the Ariso (cloud) backend.** Cloud recordings keep generating
  notes server-side regardless of this setting, same as the existing remote
  providers.
- **A "test connection" affordance for the three existing fixed providers.**
  Those are vetted SaaS hosts with one connection failure mode (a bad key);
  a custom host has many more (wrong port, wrong path, firewalled, DNS,
  self-signed cert), which is why testing matters here specifically. Adding
  it retroactively to the fixed three is a reasonable follow-up, not bundled
  into this change.
- **Capabilities changes.** No new window, no new `src-tauri/capabilities/`
  entry — this is plain Rust behind existing `#[tauri::command]`s, same
  posture as the fixed-provider work.

## Design

### Persisted shape

`notes_model.rs`'s `NotesModelId` gains a third, field-less variant:

```rust
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum NotesModelId {
    Local { id: String },
    Remote { provider: RemoteProvider, id: String },
    Custom,
}
```

`Custom` carries no fields because, unlike `Remote`, there is only ever one
configured custom endpoint — its base URL and model id live in a separate,
non-secret settings key, the same way a `Remote` selection's API key already
lives outside the selection itself (in the keychain, looked up at call time).
This keeps "which model is selected" and "what that model needs to run"
separated the way the existing design already separates them for `Remote`,
just carried one step further.

A new settings.json key, `customNotesEndpoint`, holds the non-secret part:

```json
{ "baseUrl": "http://10.0.1.20:8000", "modelId": "Qwen2.5-72B-Instruct" }
```

Two new `#[tauri::command]`s in `notes_model.rs`, mirroring
`set_notes_model`'s pattern (validate, then write-through settings.json and
the process-global seed, so `transcribe::process_notes` — which runs
detached with no `AppHandle` — can read it without a round trip):

```rust
pub fn set_custom_endpoint(app: AppHandle, base_url: String, model_id: String) -> Result<(), String>;
pub fn get_custom_endpoint() -> Option<CustomEndpoint>; // {baseUrl, modelId} — no secret, safe for the webview
```

`set_custom_endpoint` validates before persisting, the same way
`set_llm_api_key` validates a pasted key without ever surfacing the raw
value in the error:

- `base_url` parses as a URL (`reqwest::Url::parse` — already a transitive
  dependency via `reqwest`) with scheme `http` or `https` and a non-empty
  host. Anything else is rejected with an actionable message ("Enter a
  valid http:// or https:// URL.") rather than silently falling back —
  this is the one place in the notes-model settings where the value being
  validated is genuinely free text a user is actively typing, not a
  hand-edited `settings.json`.
- `model_id` is non-empty, bounded in length, and free of control
  characters — the same shape check `credentials::validate_key` already
  applies to a pasted API key, reused here since both values are about to
  travel in a JSON body header.

`parse()` (the defensive path for a value loaded from `settings.json`, e.g.
a downgrade or hand-edited file) keeps its existing behavior: a `Custom`
value always parses successfully as a *kind* — whether it can actually run
depends on `get_custom_endpoint()` and the stored key both being present,
checked at call time, not at parse time.

### API key storage

Reuses `credentials.rs`'s consolidated keychain item (one JSON map, one
Keychain/Credential-Manager entry, per the
`2026-09-24-single-keychain-api-key-item-design.md` refactor) rather than
adding a second credential item. The custom endpoint's key lives under a
reserved map key, `"custom"`, which is not a `RemoteProvider` variant (it
names no fixed host, so it doesn't belong in that closed enum) but shares
every other mechanism: the same `KeyMap`, `load_keys`/`save_keys`, and the
Windows Credential Manager size budget (`ensure_fits`) that already accounts
for the total serialized map, not per-key.

```rust
#[tauri::command]
pub fn set_custom_llm_key(key: String) -> Result<(), String>;
#[tauri::command]
pub fn has_custom_llm_key() -> Result<bool, String>;
#[tauri::command]
pub fn clear_custom_llm_key() -> Result<(), String>;
```

As with `get_api_key`, there is no command that returns the key's value to a
webview — Settings only ever learns whether one is stored.

### Notes generation and connection test

`transcribe.rs::generate_notes` (the `match` at lines 237-252) gains a third
arm:

```rust
crate::notes_model::NotesModelId::Custom => {
    let endpoint = crate::notes_model::get_custom_endpoint()
        .ok_or_else(|| "No custom endpoint configured — set one up in Settings.".to_string())?;
    let key = crate::credentials::get_custom_api_key()?
        .ok_or_else(|| "No API key stored for the custom endpoint — add one in Settings.".to_string())?;
    let transcript = std::fs::read_to_string(transcript_path)
        .map_err(|e| format!("read transcript: {e}"))?;
    crate::remote_notes::run_custom_notes(&transcript, &endpoint.base_url, &endpoint.model_id, &key).await
}
```

`remote_notes.rs` factors its existing `RemoteProvider::OpenAi` request
builder and `extract_text` arm into a shared helper keyed by base URL instead
of by provider, and adds:

```rust
pub async fn run_custom_notes(
    transcript: &str,
    base_url: &str,
    model_id: &str,
    api_key: &str,
) -> Result<NotesOutput, String>;
```

Same request shape (`{base_url}/v1/chat/completions`, bearer auth, the
existing `SYSTEM_PROMPT`), same timeout constants, same error taxonomy
(401/403 → "rejected the API key," timeout → named, connection failure →
named) — with messages naming "the custom endpoint" rather than a provider,
since there's no brand name to show. Same test-only base-URL override seam
is unnecessary here since the base URL is already a runtime value, not a
hardcoded one — tests point `run_custom_notes` at a loopback stub directly.

A new command backs "Test connection," reusing the same request builder
against live (not-yet-saved) form values rather than the stored config:

```rust
#[tauri::command]
pub async fn test_custom_notes_endpoint(
    base_url: String,
    model_id: String,
    key: String,
) -> Result<(), String>;
```

It sends one minimal chat-completion request (a short fixed test prompt, not
the notes system prompt) with the same timeouts as a real notes call, and
returns `Ok(())` on any 2xx response it can parse as the expected shape, or
the same actionable error strings `run_custom_notes` would produce. It does
not persist anything — Save is a separate, explicit step, matching how the
fixed providers' key entry already works.

### Settings UI

The "AI Models" table (`SettingsView.vue`, the `model-table` rendered from
`modelCatalog()`) gains one more row, "Custom endpoint," appended after the
three fixed remote entries. `modelCatalog()` (`src/modelCatalog.ts`) takes
the loaded `CustomEndpoint | null` and `hasCustomKey: boolean` as inputs (it
already isn't a compile-time constant in spirit — `SettingsView.vue` already
holds all the other row-completing state like `connectedProviders`) and
renders the row's `details`/name from the configured model id, falling back
to "Not configured" when there is none.

Unlike the fixed providers' single-field key prompt, this row's "+" opens a
three-field form: Base URL, Model identifier, API key, plus **Test
connection** and **Save** buttons (mirroring `keyProvider`/`keyInput`'s
existing inline-prompt pattern, extended to three `ref`s). Once configured
and keyed:

- The row's action icon becomes two: **Edit** (pencil) reopens the form
  pre-filled with the saved base URL and model id, and an empty key field
  with placeholder text "Leave blank to keep the saved key" — saving with an
  empty key field calls `set_custom_endpoint` but skips `set_custom_llm_key`,
  so an edit to just the model id never requires re-pasting the key. **Remove**
  (minus) clears the endpoint and the key together in one action (a
  confirmation dialog, matching `onRemoveRow`'s existing pattern) — unlike
  the fixed providers, where removing a key leaves the model registered,
  there is nothing left to keep once a singular custom endpoint's key is
  gone.
- Selecting the row as the active notes model still goes through the
  existing `onRowClick` → `rowUsable` → `setNotesModelSetting` path
  unchanged: `rowUsable` for this row is true once both the endpoint is
  configured and a key is stored, exactly mirroring `rowConnected` for the
  fixed three. Saving the form does **not** itself select the row — matching
  today's fixed-provider behavior, where connecting a key and selecting the
  model are two separate clicks.
- The permanent disclosure line reads "Notes for new recordings will be sent
  to `{baseUrl}`." — showing the actual configured host, not a provider
  brand name, since a homelab/DGX address is exactly the information a user
  needs to double check. When the saved URL's scheme is `http://`, an
  additional line appears: "This connection is not encrypted — the
  transcript and API key travel in plain text on your network." This is a
  warning, not a block: an unencrypted LAN call to a box the user controls is
  the primary use case in the issue, not a mistake to prevent.

## Cloud vs offline

Local-backend-only, same as the three fixed remote providers; the Ariso
backend is unaffected.

This is a second, and structurally different, exception to the Local
backend's "nothing leaves the machine" invariant
(`oats-security` item #10). The fixed three providers' hosts are chosen and
compiled by oats; this one's host is chosen by the user at runtime. That is
a materially larger trust boundary — a mistyped or malicious URL is sent a
real HTTP request with the transcript and, in the auth header, the user's
own key — and it must stay clearly distinct from `oats-security` item #8
(don't let the app's *own* API base be repointed): this is a wholly separate
setting, reachable only when Local is selected and the user explicitly
configures and selects the Custom row, and it never touches
`DEFAULT_API_BASE_URL` or anything Ariso-related. The per-endpoint disclosure
above exists precisely because this exception is now user-directed at an
arbitrary address rather than one of three names oats already vetted.

## Error handling

- **Bad URL at save time**: rejected before it reaches `settings.json`, with
  a message the user can act on ("Enter a valid http:// or https:// URL"),
  not a generic parse failure.
- **Test connection fails**: connection refused, DNS failure, and timeout
  each get a distinct, named message (reusing `remote_notes.rs`'s existing
  `transport_reason` categories); a non-2xx response reports the status
  code; a 2xx response that doesn't parse as a chat completion reports "the
  endpoint responded, but not in the expected format" — this is the
  likeliest real-world failure for a homemade endpoint and deserves its own
  message rather than folding into a generic error.
- **Remote call fails mid-meeting** (same as the existing fixed-provider
  behavior): `process_notes` sets `notes_error`, keeps the transcript, and
  "Regenerate notes" retries the same endpoint. No automatic fallback to the
  on-device model.
- **Key rejected (401/403)**: surfaced distinctly from a network failure, so
  the user knows to fix the key rather than just retry.
- **Endpoint or key missing/cleared at call time** (removed from Settings,
  or the settings.json value fails to parse): treated as "not configured" —
  fails fast with a clear error rather than sending an unauthenticated or
  malformed request.
- **Switching away from Custom mid-recording**: unaffected — an in-flight
  notes generation always finishes against the model selected when it
  started, same as today.

## Testing

- **Rust (`notes_model.rs`)**: URL validation accepts `http://10.0.1.20:8000`
  and `https://dgx.local:8443`, and rejects a non-http(s) scheme, a missing
  host, and an empty string; `Custom` round-trips through the persisted JSON
  shape (`{"kind":"custom"}`); `get_custom_endpoint`/`set_custom_endpoint`
  round-trip through `settings.json` and the process-global seed used by
  detached notes generation.
- **Rust (`credentials.rs`)**: the reserved `"custom"` map key coexists with
  the three `RemoteProvider` keys in the same consolidated item without
  collision; set/has/clear round-trip for the custom key; clearing it alone
  leaves the three providers' keys untouched.
- **Rust (`remote_notes.rs`)**: `run_custom_notes` against a loopback stub —
  request goes to `{base_url}/v1/chat/completions` with a bearer key and the
  transcript in the body; a 401 response produces a "rejected the API key"
  error without echoing the key; a malformed/unexpected response body
  produces the "not in the expected format" error rather than a panic;
  `test_custom_notes_endpoint` succeeds against a 200 stub and reports the
  same error taxonomy against 401/500/timeout/connection-refused stubs.
- **Rust (`transcribe.rs`)**: `generate_notes`'s new `Custom` arm fails fast
  and distinctly when the endpoint is unconfigured versus when the key is
  missing versus when both are present but the mocked HTTP call fails —
  extending the existing `Remote` failure tests' pattern.
- **Vitest (`SettingsView.test.ts`)**: the Custom row renders "Not
  configured" with no saved endpoint; the three-field form validates and
  calls `test_custom_notes_endpoint`; Save calls `set_custom_endpoint` and,
  only when the key field is non-empty, `set_custom_llm_key`; Edit pre-fills
  URL and model id but leaves the key field blank; Remove clears both
  through a confirmation dialog; the row becomes selectable
  (`rowUsable`/`onRowClick`) only once both the endpoint and the key are
  present; the `http://` disclosure line appears only for a non-https saved
  URL.
- **Manual**: point the custom endpoint at a real local OpenAI-compatible
  server (`vllm serve`, `ollama serve` with its OpenAI-compat routes, or
  similar) with a real model id, use "Test connection" to confirm success,
  save, select the row, record a short meeting, and use a network proxy or
  Console.app to confirm exactly one outbound request to that host carrying
  the transcript, with neither the transcript nor the key appearing in oats'
  own log output; then stop the server and confirm a subsequent recording's
  "Test connection" and notes generation both fail with the connection-
  refused message rather than hanging or crashing.

## Open questions

- **OpenAI-compatible chat completions as the only supported shape.** This
  spec assumes it because it's what vLLM/NIM/Ollama/text-generation-webui
  already speak, and it lets this reuse most of the existing `OpenAi`
  request path. Confirm that's an acceptable constraint rather than a design
  gap — a user whose self-hosted server exposes a different API (raw
  completions, a custom schema) isn't served by this increment.
- **Allowing `http://`, not just `https://`.** An on-prem box without a
  valid TLS certificate is the issue's own motivating case, so this spec
  allows it with an unencrypted-connection warning rather than blocking it.
  Confirm that's the right default versus requiring `https://` and pushing
  TLS termination onto the user (e.g. via a reverse proxy).
- **One custom endpoint versus a list.** The issue's wording reads singular;
  confirm that's sufficient for now, since supporting several would need a
  different row model (a list rather than one fixed slot) and a name/label
  per entry, which is a larger UI change than this spec's scope.
