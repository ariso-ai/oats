# Implement the custom remote LLM endpoint (issue #458)

Builds on `docs/superpowers/specs/2026-09-29-custom-llm-endpoint-design.md` (the
"design spec"), written for issue #456 and merged via PR #457. Issue #458 asks
for that design to actually be implemented, and adds one explicit acceptance
criterion the design spec left ambiguous: the API key must be allowed to be
empty. This document is the implementation spec: it inherits the design
spec's architecture in full and calls out only where this run's decisions
sharpen or amend it.

## Problem

The design spec already lays out the problem in detail (a fourth,
user-supplied Local-backend notes model: base URL + model id + API key,
speaking the OpenAI-compatible `/v1/chat/completions` shape). Nothing in that
problem statement changes here. What changes is scope of work: this is the
actual Rust + Vue implementation, not the design.

## Acceptance criteria (from issue #458, verbatim)

- New option to add a remote LLM model in the Settings window, supporting
  both a URL and an API key, **where the API key may be left empty**.
- It works with a `/v1/chat/completions` endpoint at the configured remote
  model's URL.

Every acceptance criterion from the design spec's own "Goal" section is also
carried forward unchanged (test connection, edit without re-pasting an
unchanged key, remove in one action, permanent disclosure + `http://`
warning) — see [Decisions](#decisions) for the one place they interact with
the empty-key requirement.

## Decisions

Autopilot ran non-interactively; no human answered these, so each took its
recommended default except where the issue's own acceptance criteria settled
it outright.

1. **Is the API key optional end-to-end, not just an empty string rejected
   with a friendlier message?** (default: yes — trusted comment N/A, but the
   issue body's acceptance criterion says "allow empty" outright, which
   settles this). A configured custom endpoint with no key is a first-class,
   fully-usable state: the row becomes selectable once base URL + model id
   are set, with no key required. At request time, no `Authorization` header
   is sent when no key is stored — many self-hosted OpenAI-compatible servers
   (`vllm serve`, bare `ollama`) run with no auth at all, and the issue's own
   motivating case (a homelab/DGX box) is exactly this.

   This **amends** the design spec's Settings UI section, which said
   `rowUsable` for the Custom row mirrors `rowConnected` for the fixed three
   ("true once both the endpoint is configured and a key is stored"). That
   sentence is superseded: `rowUsable` for Custom is true once the endpoint
   (base URL + model id) is configured, full stop.

2. **How does "leave blank to keep the saved key" coexist with "blank means
   no key"?** (default: a small explicit control, not overloading blank on
   its own). The design spec's edit story — leave the key field blank to
   avoid re-pasting an unchanged key — only makes sense when a key is
   already stored; once "no key" is a valid, common end state, blank can't
   mean both "unchanged" and "cleared." Resolution: the key field's blank
   value always means "no change to what's stored" (nothing → stays
   nothing; a stored key → stays as it is). A separate, small "Use no API
   key" action, shown only when editing an endpoint that currently has a
   key stored, clears the key explicitly on save. This keeps the common
   paths (fresh no-auth setup: leave blank, save; edit the model id on a
   keyed endpoint: leave blank, save, key untouched) a single blank field,
   and makes the uncommon path (revoke a previously-set key without
   removing the whole endpoint) one extra explicit click rather than an
   overloaded blank.
3. **Does "Test connection" require a key?** (default: no). It runs with
   whatever the form currently holds — if the key field is empty, it tests
   with no `Authorization` header, the same as a real notes call would with
   no key stored.
4. **Filename/placement of this spec.** (default: a new implementation spec
   referencing the design spec, per autopilot's instruction to save a spec
   for this run, rather than editing the already-merged design doc.)

## Design (inherits the design spec; deltas only)

Everything in the design spec's "Design" section applies as written, with
these adjustments:

### Persisted shape — unchanged

`NotesModelId::Custom` (field-less), `CustomEndpoint { base_url, model_id }`
in a new `customNotesEndpoint` settings.json key, exactly as specified.

### API key storage — key is optional

`credentials.rs` gains the reserved `"custom"` map key as specified, with one
change: the command surface treats "no key" as a normal, queryable state
rather than treating the row as unusable.

```rust
#[tauri::command]
pub fn set_custom_llm_key(key: String) -> Result<(), String>;
#[tauri::command]
pub fn has_custom_llm_key() -> Result<bool, String>;
#[tauri::command]
pub fn clear_custom_llm_key() -> Result<(), String>;
```

`set_custom_llm_key` still runs the pasted value through `validate_key`
(non-empty, bounded, no control characters) — this command is only ever
invoked with a non-empty key from the UI (see below); an empty string here is
still rejected the way it always has been, since "no key" is expressed by
*not calling* this command, not by calling it with `""`.

### Notes generation and connection test — `Option<&str>` key

`remote_notes::run_custom_notes` and `test_custom_notes_endpoint` take the
key as `Option<&str>` / `Option<String>` rather than a bare string:

```rust
pub async fn run_custom_notes(
    transcript: &str,
    base_url: &str,
    model_id: &str,
    api_key: Option<&str>,
) -> Result<NotesOutput, String>;

#[tauri::command]
pub async fn test_custom_notes_endpoint(
    base_url: String,
    model_id: String,
    key: Option<String>,
) -> Result<(), String>;
```

The request builder calls `.bearer_auth(key)` only when `api_key.is_some()`;
otherwise it sends the request with no `Authorization` header at all (not an
empty bearer token, which some servers would reject differently than "no
header").

`transcribe.rs::generate_notes`'s new `Custom` arm reads the key with
`crate::credentials::get_custom_api_key()?` (returns `Option<String>`, same
as `get_api_key` does for the fixed providers today) and passes it straight
through — no "missing key" error, since a missing key is a valid
configuration, not a failure. Only a missing *endpoint* (base URL/model id)
is an error:

```rust
crate::notes_model::NotesModelId::Custom => {
    let endpoint = crate::notes_model::get_custom_endpoint()
        .ok_or_else(|| "No custom endpoint configured — set one up in Settings.".to_string())?;
    let key = crate::credentials::get_custom_api_key()?;
    let transcript = std::fs::read_to_string(transcript_path)
        .map_err(|e| format!("read transcript: {e}"))?;
    crate::remote_notes::run_custom_notes(&transcript, &endpoint.base_url, &endpoint.model_id, key.as_deref()).await
}
```

Everything else in the design spec's request/response/error-taxonomy section
(shared helper keyed by base URL, same timeouts, same 401/403/429/timeout/
connection-failure error shapes) applies unchanged — a 401/403 from a
no-key request is still surfaced as "the endpoint rejected the request,"
just without implying a key exists to check.

### Settings UI — key field optional, one new small control

The three-field form (Base URL, Model identifier, API key) keeps its shape.
Changes from the design spec:

- The API key field's placeholder reads "API key (optional — leave blank if
  the endpoint doesn't require one)" instead of implying it's required.
- `rowUsable`/the row's selectability: true once `get_custom_endpoint()`
  returns a value (base URL + model id both present) — **not** gated on a
  key. This is the direct implementation of acceptance criterion 1.
- The row's disclosure/status text distinguishes three states instead of
  two: not configured; configured with no key ("Notes for new recordings
  will be sent to `{baseUrl}`, with no API key."); configured with a key
  ("Notes for new recordings will be sent to `{baseUrl}`."). The `http://`
  plaintext warning from the design spec applies in both configured states.
- Editing an endpoint that currently has a key stored shows a small "Use no
  API key" text action next to the key field (only in that state); clicking
  it clears the field and arms an explicit "clear the key on save" intent
  distinct from "field is blank, leave the key as it is." Saving with the
  field simply left untouched (never focused, or focused and left empty
  without clicking that action) never touches the stored key.
- Save's key handling: non-empty field → `set_custom_llm_key`; field empty
  and "Use no API key" was clicked → `clear_custom_llm_key`; field empty and
  that action was not clicked → skip both calls (key, if any, is untouched).
- Test connection passes the form's current key field verbatim (empty string
  becomes `None` before the Rust call).

## Cloud vs offline

Unchanged from the design spec: Local-backend-only; Ariso is unaffected;
this is a second, user-directed exception to the offline privacy invariant,
gated behind an explicit Local-only setting the user must configure and
select.

## Error handling

Unchanged from the design spec, with one addition: a missing key is never an
error on its own, at either "Test connection" or real notes generation — only
a missing endpoint (no base URL/model id configured) is. A key-requiring
server that gets no `Authorization` header will itself return 401/403, which
surfaces through the existing "rejected the request" error path.

## Testing

All of the design spec's `notes_model.rs` / `credentials.rs` / `remote_notes.rs`
/ `transcribe.rs` / `SettingsView.test.ts` test coverage applies, adjusted for
the optional key:

- **`remote_notes.rs`**: `run_custom_notes` with `api_key: None` sends no
  `Authorization` header (assert on the stub's seen headers) and still
  succeeds against a 200 stub; with `Some(key)` it sends `Bearer {key}` as
  before.
- **`credentials.rs`**: `has_custom_llm_key` reflects presence/absence
  correctly; `get_custom_api_key` returns `None` when nothing is stored
  (not an error).
- **`transcribe.rs`**: the `Custom` arm's "missing endpoint" failure test
  from the design spec still applies; a new case covers "endpoint configured,
  no key stored" reaching `run_custom_notes` successfully (via a mocked HTTP
  call) rather than failing fast the way "missing key" does for the fixed
  providers.
- **`SettingsView.test.ts`**: saving with an empty key field and no prior key
  calls `set_custom_endpoint` but neither key command; the row becomes
  selectable with no key stored; the "Use no API key" action only renders
  when a key is currently stored, and clicking it plus Save calls
  `clear_custom_llm_key` instead of `set_custom_llm_key`.

The design spec's manual test plan (point at a real local OpenAI-compatible
server, confirm exactly one outbound request, confirm no key/transcript in
logs, confirm graceful failure once the server stops) is unaffected and still
the intended manual verification — this run cannot execute it directly (no
running app, no real local inference server available in CI), so it is
called out as unverified in the PR body, same as the design spec anticipated.

## Non-goals

Same as the design spec: no generic request/response schema, no multiple
custom endpoints, no changes to the three fixed providers or the on-device
model, no Ariso backend changes, no "test connection" retrofit onto the
fixed three, no capabilities changes.
