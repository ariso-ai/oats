# Local backend notes-model picker (issue #407)

## Problem

The Local backend's notes generation has exactly one model: on-device Gemma
(`gemma-3-1b-it-qat-4bit`, downloaded and run by `model_manager.rs` /
`transcribe.rs::run_notes` via the `ariso-stt notes` sidecar call). There is no
way to trade that off against a stronger model. A user on capable hardware who
wants better summaries has no lever; a user who wants notes handled by a
provider they already trust and pay for (OpenAI, Gemini, Anthropic) has no way
to opt in either. Every control for the Local backend already lives in
Settings' "On-device models" card (`SettingsView.vue:104-176`) — install
buttons for the speech model and the language model — so a model choice
belongs there too, not in the recording or meeting flows.

`shawnzhu`'s steer on the issue names the concrete shape: support additional
on-device models ("gemma 4, llama, qwen, etc, as long as it fits your
machine"), and separately, support remote models by letting the user supply
their own API key, stored in a system key store (macOS Keychain). That second
half is a deliberate, opt-in breach of the Local backend's "nothing leaves the
machine" guarantee (`oats-security` skill, item #10) for the one feature the
user explicitly chooses to route off-device — the design has to make that
trade-off legible, not paper over it.

Speech-model choice (issue #407's own text: "STT selection can follow in a
subsequent increment") stays out of this spec. So does the broader on-device
runtime work of actually adding llama.cpp/qwen-class model support — see
Non-goals.

## Goal

From Settings, with the Local backend selected, a user can:

- See which model currently generates notes (default: today's on-device
  Gemma).
- Switch to a different **on-device** model from a short, fixed list, download
  it if not already present, and have new notes generation use it.
- Switch to a **remote** model (OpenAI, Gemini, or Anthropic) by picking a
  provider + model and pasting an API key, which is stored in the OS keychain
  — never in `plugin-store`, never logged.
- See an explicit, permanent disclosure next to the remote option that
  choosing it sends this meeting's transcript to that provider.

Reviewable as: open Settings with Local selected, see a "Notes model" control
in the existing "On-device models" card defaulting to Gemma; switch it to a
second on-device model, confirm a download affordance appears and, once
installed, a newly recorded meeting's notes come from that model; switch to
"Anthropic — Claude Haiku", paste a key, confirm it's accepted and stored (not
visible in `settings.json` or Console.app logs), record a meeting, and confirm
notes generation makes exactly one HTTPS call to `api.anthropic.com` carrying
the transcript.

## Non-goals

- **STT/speech-model choice.** Explicitly deferred by the issue itself to a
  later increment. Nothing here touches `download_local_stt` or the speech
  picker.
- **Adding real llama.cpp/GGUF support for arbitrary on-device models.**
  Windows already runs the notes LLM through a GGUF file
  (`model_manager.rs`'s `windows_llm_bundles`); macOS runs Gemma through MLX
  inside the `ariso-stt` sidecar, which has no generic "load any GGUF" path
  today. This spec adds the picker, the per-model manifest, and the download
  plumbing for **one additional on-device model** as the increment's concrete
  deliverable (see [Design](#design) for which), and designs the manifest so a
  later spec can register more without redesigning storage — it does not
  attempt to ship "llama, qwen, etc." as a set in one PR.
- **A generic credential-proxy through Ariso's backend.** shawnzhu's comment
  is explicit: bring-your-own-key, stored client-side in the OS keychain. Oats
  never sees or forwards these keys through its own servers.
- **Per-meeting or per-recording model overrides.** One setting, centralized
  in Settings, applies to every subsequent local recording — matching "all
  configuration should be able to be done within the settings window only."
- **Retrying a remote call against a different provider automatically, or any
  cross-provider fallback chain.** See [Error handling](#error-handling) for
  the one fallback this spec does take a position on (remote → on-device).
- **Changing anything about the Ariso (cloud) backend.** Cloud recordings
  already generate notes server-side; this is Local-backend-only.

## Design

### Model registry

A small static registry (new `src-tauri/src/notes_model.rs`) replaces the
single hardcoded `LLM_MODEL_NAME` constant's role as "the" notes model:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum NotesModelId {
    /// One of the fixed on-device entries below.
    Local { id: String },
    /// A remote provider + model. `id` is one of a fixed per-provider list
    /// (e.g. "gpt-4.1-mini", "gemini-3.0-flash", "claude-haiku-4-5"), not
    /// free text — the UI is a dropdown, not a text field.
    Remote { provider: RemoteProvider, id: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RemoteProvider { OpenAi, Gemini, Anthropic }
```

On-device registry entries (id, display name, download bundle) start with the
two the increment ships:

- `gemma-3-1b-it-qat-4bit` — today's model, always present in the registry as
  the zero-download default.
- One additional on-device entry, sized for the same "fits on a laptop"
  bar Gemma-3-1B already clears (e.g. a small Qwen2.5-Instruct GGUF) — exact
  model TBD, see [Open questions](#open-questions). It downloads through the
  existing `download_model_bundles` machinery in `model_manager.rs` (new
  `ModelBundle` entries, same pinned-SHA256 + atomic-rename integrity model),
  landing under `<models>/llm/<id>/` alongside Gemma's directory rather than
  replacing it — a user can have both downloaded and swap between them without
  re-downloading.

Remote registry entries are a fixed `(provider, model id, display name)` list
seeded from the issue: OpenAI GPT (current small/large tier), Gemini Flash
(3.0/3.5/3.7), Anthropic Haiku and Sonnet (current versions). No free-text
model-id entry — an unpinned string reaching a `format!("https://api.../{id}")
URL is exactly the kind of input `oats-security` flags as untrusted.

### Persisted setting

`getBackendSetting`/`setBackendSetting`'s pattern (`src/tauri.ts:595-604`,
`plugin-store` `settings.json`) extends with a `notesModel` key: the selected
`NotesModelId` serialized as JSON. This is a preference, not a secret, so
plaintext `plugin-store` is fine for it (`oats-security` item #4) — only the
API key itself needs the keychain.

```ts
export async function getNotesModelSetting(): Promise<NotesModelId>;
export async function setNotesModelSetting(model: NotesModelId): Promise<void>;
```

Backed by two new `#[tauri::command]`s in `commands.rs` that read/write the
same `settings.json` store from the Rust side (`transcribe.rs::process_notes`
runs detached from any window and needs the current selection without an
`invoke` round trip back to a webview) — mirrors how `getBackendSetting`
already has a Rust-side equivalent read for backend-gated logic elsewhere in
`commands.rs`.

### API keys: OS keychain, one entry per provider

New `src-tauri/src/credentials.rs` wraps the [`keyring`](https://crates.io/crates/keyring)
crate (Security.framework on macOS, Credential Manager on Windows — this repo
already ships a Windows notes/STT path in `model_manager.rs`, so the storage
layer should too). Service name `ai.ariso.desktop`, one keychain entry per
provider (account `"llm-api-key:openai"` etc.), so switching models doesn't
require re-entering a key already saved for that provider.

```rust
#[tauri::command]
pub fn set_llm_api_key(provider: RemoteProvider, key: String) -> Result<(), String>;
#[tauri::command]
pub fn has_llm_api_key(provider: RemoteProvider) -> Result<bool, String>;
#[tauri::command]
pub fn clear_llm_api_key(provider: RemoteProvider) -> Result<(), String>;
```

Deliberately no `get_llm_api_key` reachable from the frontend: once saved, the
webview only ever needs to know *whether* a key is set (to render "Connected"
vs. an input box), never the key's value. The key is read back out of the
keychain only on the Rust side, at the moment a notes call is made.

### Notes generation: sidecar for local, direct HTTPS for remote

`transcribe.rs::process_notes` currently always calls `run_notes(transcript,
models)`, which shells out to `ariso-stt notes`. It now branches on the
persisted `NotesModelId`:

- **`Local { id }`**: unchanged shape, `run_notes` gains an `id` parameter so
  it passes `--model-dir <models>/llm/<id>/` instead of the hardcoded Gemma
  path; the sidecar contract (`{title, notes}` JSON on stdout) is unchanged.
- **`Remote { provider, id }`**: a new `run_remote_notes(transcript_text,
  provider, id)` in the same module builds a prompt equivalent to what the
  sidecar sends today (reusing `NOTES_TIMEOUT` as the request timeout) and
  calls that provider's chat-completion endpoint over `reqwest`, with the key
  read from `credentials::get_llm_api_key(provider)` and sent only in that
  request's `Authorization`/`x-api-key` header — never logged (`oats-security`
  item #11: transcript text and the key must not land in stdout/stderr, so
  this path logs neither the prompt nor the response body on error, only the
  HTTP status and provider name). The three providers' request/response shapes
  differ enough (OpenAI/Anthropic use different auth headers and JSON shapes,
  Gemini's is different again) that this is three small provider-specific
  request builders behind one `RemoteProvider` match, not a shared client.

Both paths converge back on the same `NotesOutput { title, notes }` that
`process_notes` already writes to the vault — no change to `storage::write_notes`,
`vault::write_note`, or anything downstream.

### Settings UI

`SettingsView.vue`'s existing "On-device models" card (`104-176`) gets a new
row above "Speech voice model" — "Notes model" — showing the current selection
and a picker (a simple `<select>`-style dropdown listing on-device entries
first, then a "Remote (sends transcript to provider)" group). Renaming the
card itself is out of scope; the row lives there because that's where every
other Local-backend model control already is.

- Picking an on-device entry not yet downloaded shows the same
  install/download-progress affordance the language-model row already has
  (`llmStatusText`/`onInstallLlm` pattern), parameterized by model id.
- Picking a remote entry reveals an inline API-key input (masked, paste-only
  UX — no "show key" toggle) plus a **permanent** disclosure line: "Notes for
  new recordings will be sent to {Provider}. Recording, transcription, and
  audio stay on this device." This is the disclosure item #1 from the prior
  `/shape` round's open questions — this spec's answer is yes, inline and
  persistent, not a one-time dismissible dialog, so the trade-off stays
  visible every time the user is in the picker.
- Once a key is saved, the input collapses to a "Connected — Change key /
  Remove" state (`has_llm_api_key` truth), consistent with not exposing the
  key value back to the webview.

### Capabilities

No new window, no new plugin capability. `keyring` and the remote-provider
`reqwest` calls are plain Rust code behind existing `#[tauri::command]`s,
same posture as `rename_local_speaker` needing no capabilities-file entry.

## Cloud vs offline

This is Local-backend-only; the Ariso (cloud) backend is unaffected and keeps
generating notes server-side regardless of this setting.

Within Local mode, this spec intentionally introduces the first network call
reachable from an otherwise-offline path — but only when the user has
explicitly selected a remote model and saved a key for it. The on-device
default (Gemma, or the new local model) makes zero network calls, same as
today. The Settings UI must make this switch legible (see the disclosure
above) precisely because `oats-security` item #10 treats any such call as a
breach of the "Local means nothing leaves the machine" invariant by default —
this feature is the one place in the app that invariant becomes conditional,
and only because the user turned it off for themselves.

## Error handling

- **Remote call fails mid-meeting** (auth error, rate limit, network loss,
  timeout): `process_notes` treats it exactly like today's sidecar failure
  path — `meta.notes_error` is set, the recording keeps its transcript, and
  the existing "Regenerate notes" action (`local.retryNotes`) retries against
  the same selected model. No automatic fallback to the on-device model on a
  single failure — a fallback would silently change which model produced a
  given note, which cuts against picking a model specifically for its output
  quality. If a remote model is unreachable, the user sees the same
  `notes_error` surface that already exists and can retry or switch models
  from Settings.
- **Key rejected by the provider (401/403)**: surfaced through the same
  `notes_error` path with the provider's error distinguishable from a generic
  network failure, so the user knows to fix the key in Settings rather than
  just retry.
- **Selected on-device model not downloaded when a recording finishes**:
  mirrors the existing Gemma gate — `local_models_ready` (or its successor)
  already blocks recording start until both models are ready
  (`commands::ensure_recording_allowed`); the notes-model download-required
  state is surfaced the same way for whichever model is currently selected.
- **Key deleted or keychain entry missing at call time** (user revoked it
  outside the app, or `clear_llm_api_key`): treated as "not configured" —
  `process_notes` fails fast with a clear `notes_error` rather than sending a
  request with no `Authorization` header.
- **Switching models mid-flight**: a recording already using `run_notes`/
  `run_remote_notes` for its notes generation is not interrupted by a setting
  change; the new selection applies to the next recording only, same
  semantics as the existing model-download flow (settings changes never
  reach into an in-progress notes generation).

## Testing

- **Rust (`credentials.rs`)**: set/has/clear round-trip against the keychain
  (the `keyring` crate ships a mock backend for CI — use it so tests don't
  touch the real macOS Keychain in CI); `has_llm_api_key` is `false` after
  `clear`; no key value ever appears in a `Debug`/error-formatted string
  (grep-style assertion on error messages).
- **Rust (`notes_model.rs`)**: `NotesModelId` (de)serializes to the expected
  JSON shape for both variants; the remote registry rejects an unknown
  `(provider, id)` pair before any network call is attempted.
- **Rust (`transcribe.rs`)**: `process_notes` branches correctly on a `Local`
  vs `Remote` selection (mock the sidecar call and the HTTP call respectively,
  as existing tests already mock `run_notes`); a remote HTTP failure produces
  the same `notes_error`-set, transcript-preserved outcome as today's sidecar
  failure test; the request body sent to a mocked remote endpoint never
  contains the API key in cleartext logs (only in the auth header).
- **Vitest (`SettingsView.test.ts`)**: the Notes model row renders only when
  `backend === 'local'`; selecting a remote provider shows the disclosure text
  and the key input; saving a key calls `set_llm_api_key` and flips the row to
  "Connected"; selecting an undownloaded on-device model shows the
  download/install affordance; the persisted selection round-trips through
  `getNotesModelSetting`/`setNotesModelSetting`.
- **Manual**: with Local selected, downloaded Gemma notes as today's baseline;
  switch to the new on-device model, download it, record a short meeting,
  confirm notes come from it; switch to a remote provider, paste a real key,
  record a meeting, and use a network proxy or Console.app to confirm exactly
  one outbound HTTPS request to that provider's API host with the transcript
  in the body and no key or transcript text in oats' own log output; remove
  the key and confirm the row reverts to "not configured" and a subsequent
  recording's notes generation fails with a clear, retryable error.

## Open questions

- **Which second on-device model ships in this increment, and can macOS
  actually run it?** Gemma runs through MLX inside the `ariso-stt` Swift
  sidecar; Windows already runs a GGUF notes model through a separate runtime.
  Adding a second macOS on-device option means either the sidecar gains a
  second MLX model path or macOS gets a llama.cpp-style GGUF runtime it
  doesn't have today — that's real sidecar work, not just a registry entry.
  Confirm the exact model and whether it's macOS+Windows from day one or
  Windows-first (mirroring how STT diarization already differs by platform in
  `model_manager.rs`).
- **Exact remote model IDs to ship.** The issue's list (OpenAI GPT, Gemini
  Flash 3.0/3.5/3.7, Anthropic Haiku/Sonnet) is illustrative; confirm the
  precise model identifiers to pin in the registry (e.g. which OpenAI tier)
  before implementation, since the picker is a fixed list, not free text.
- **Should a remote-model selection change anything visible outside
  Settings** — the tray, the recorder pill, or the meeting-detail notes
  header — to flag that a given meeting's notes came from a remote provider?
  This spec's default is no: the disclosure lives where the choice is made
  (Settings), and per-note provenance isn't tracked. Confirm that's
  sufficient, or whether `meta.json` should record which model generated a
  given note's notes for later display.
