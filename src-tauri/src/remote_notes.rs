//! Meeting notes from a provider's API, for a user who chose a remote model.
//!
//! This is the one place in the Local backend that talks to the network, and
//! only because the user picked a remote model and stored a key for it
//! (`oats-security` item #10). Two rules follow from that, and the tests in
//! this module enforce both:
//!
//! * the key travels in the request's auth header and nowhere else, and
//! * no error carries the prompt, the transcript, the response body, or the
//!   key — only a status and the provider's name.
//!
//! The three providers differ enough in auth, URL and response shape that each
//! gets its own small builder; they converge on the `NotesOutput` the sidecar
//! path already produces, so everything downstream is unchanged.

use crate::credentials::RemoteProvider;
use crate::transcribe::{NotesOutput, parse_notes_payload};
use serde_json::{Value, json};
use std::time::Duration;

/// A whole notes generation, including the model's thinking time. Far shorter
/// than the sidecar's 1800s: an HTTP call that hangs this long is broken.
const REMOTE_NOTES_TIMEOUT: Duration = Duration::from_secs(300);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Anthropic's API is versioned by header; this is the current stable version.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// The note format the vault renders and the sidecar already asks its on-device
/// model for (`ariso-stt/macos/Sources/ariso-stt/main.swift`). Both paths must
/// produce the same shape, and the flatness rules are not cosmetic:
/// `vault::render_action_items` rewrites every bullet under "Action Items" as a
/// dated task at any depth, so a model that groups them under category or owner
/// bullets turns those labels into checkboxes.
const SYSTEM_PROMPT: &str = "You are a meeting-notes assistant. You are given a meeting transcript and you write concise meeting notes in Markdown.

Rules:
- Use only facts stated in the transcript. Never invent details, names, or speakers.
- The transcript labels speakers generically (e.g. \"Speaker 1\", \"Speaker 2\"). Do not invent any speaker or person who does not appear in the transcript.
- Use these level-2 (##) sections, in this order: Summary, Key Points, Decisions, Action Items. Use no other headings, and no sub-headings.
- \"Summary\" is 2-3 sentences describing what the meeting was about. The other sections are flat bullet lists: every bullet starts at the left margin, and no bullet is nested under another.
- Write one action item per bullet, stating the task. Never add bullets that group action items by theme or owner. Only attribute a task to a speaker if that exact speaker explicitly committed to it in the transcript; otherwise give the task with no owner.
- Omit any section that has no real content in the transcript (for example, if no decisions were made, leave out the Decisions section entirely). Never write placeholder text under a heading.

Reply with a single JSON object and nothing else: {\"title\": a short specific title for the meeting, \"notes\": the Markdown notes described above}. Do not wrap the JSON in a code fence.";

fn provider_base_url(provider: RemoteProvider) -> String {
    // Tests point this at a local stub. Deliberately `cfg(test)`: a release
    // build must not be repointable at another host (`oats-security` item #8).
    #[cfg(test)]
    if let Some(base) = test_base_url() {
        return base;
    }
    match provider {
        RemoteProvider::OpenAi => "https://api.openai.com".to_string(),
        RemoteProvider::Anthropic => "https://api.anthropic.com".to_string(),
        RemoteProvider::Gemini => "https://generativelanguage.googleapis.com".to_string(),
    }
}

#[cfg(test)]
fn test_base_url() -> Option<String> {
    testing::base_url()
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|e| format!("build http client: {e}"))
}

/// Generate notes for `transcript` with a provider's model.
///
/// `api_key` comes from the OS keychain at the call site, so this function
/// stays testable without touching the credential store.
pub async fn run_remote_notes(
    transcript: &str,
    provider: RemoteProvider,
    model_id: &str,
    api_key: &str,
) -> Result<NotesOutput, String> {
    let base = provider_base_url(provider);
    let client = client()?;
    let request = match provider {
        RemoteProvider::OpenAi => client
            .post(format!("{base}/v1/chat/completions"))
            .bearer_auth(api_key)
            .json(&json!({
                "model": model_id,
                "messages": [
                    { "role": "system", "content": SYSTEM_PROMPT },
                    { "role": "user", "content": transcript },
                ],
                "response_format": { "type": "json_object" },
            })),
        RemoteProvider::Anthropic => client
            .post(format!("{base}/v1/messages"))
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&json!({
                "model": model_id,
                "max_tokens": 8192,
                "system": SYSTEM_PROMPT,
                "messages": [{ "role": "user", "content": transcript }],
            })),
        RemoteProvider::Gemini => client
            .post(format!("{base}/v1beta/models/{model_id}:generateContent"))
            .header("x-goog-api-key", api_key)
            .json(&json!({
                "systemInstruction": { "parts": [{ "text": SYSTEM_PROMPT }] },
                "contents": [{ "role": "user", "parts": [{ "text": transcript }] }],
                "generationConfig": { "responseMimeType": "application/json" },
            })),
    };

    let name = provider_name(provider);
    let response = tokio::time::timeout(REMOTE_NOTES_TIMEOUT, request.send())
        .await
        .map_err(|_| {
            format!(
                "{name} did not answer within {}s",
                REMOTE_NOTES_TIMEOUT.as_secs()
            )
        })?
        // A transport error's Display can carry the full URL but never the body
        // or the key; still, report only the shape of the failure.
        .map_err(|e| format!("could not reach {name} ({})", transport_reason(&e)))?;

    let status = response.status();
    if !status.is_success() {
        // The body is the provider's, and can quote both the key and the
        // transcript back at us — never surface or log it.
        return Err(match status.as_u16() {
            401 | 403 => format!("{name} rejected the API key — check it in Settings."),
            429 => format!("{name} is rate-limiting this key; try again shortly."),
            code => format!("{name} returned HTTP {code}."),
        });
    }

    let body: Value = tokio::time::timeout(REMOTE_NOTES_TIMEOUT, response.json())
        .await
        .map_err(|_| format!("{name} stopped mid-answer"))?
        .map_err(|_| format!("{name} returned a response that could not be read as JSON"))?;

    let text = extract_text(provider, &body)
        .ok_or_else(|| format!("{name} returned no notes for this transcript"))?;
    let output = parse_notes_payload(&text);
    if output.notes.trim().is_empty() {
        return Err(format!("{name} returned empty notes"));
    }
    Ok(output)
}

/// A minimal chat-completion request against a base URL that is a runtime
/// value, not a hardcoded one — shared by `run_custom_notes` (the real notes
/// prompt) and `test_custom_notes_endpoint` (a short fixed test prompt).
async fn chat_completion(
    base_url: &str,
    model_id: &str,
    api_key: Option<&str>,
    transcript: &str,
    system_prompt: &str,
) -> Result<NotesOutput, String> {
    let client = client()?;
    let url = format!("{}v1/chat/completions", ensure_trailing_slash(base_url));
    let body = |disable_thinking: bool| {
        let mut body = json!({
            "model": model_id,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": transcript },
            ],
            "response_format": { "type": "json_object" },
        });
        if disable_thinking {
            // Thinking models (qwen3.5 on Ollama: 286s vs 9s for a 24s
            // meeting) reason for minutes, blow REMOTE_NOTES_TIMEOUT on a real
            // meeting, and with JSON mode on answer detached from their own
            // reasoning. Ollama reads `reasoning_effort`; llama.cpp, mlx_lm
            // and vLLM read the chat-template switch. Non-thinking models
            // ignore both.
            body["reasoning_effort"] = json!("none");
            body["chat_template_kwargs"] = json!({ "enable_thinking": false });
        }
        body
    };
    let send = |body: Value| {
        // No header at all when there is no key — never an empty bearer token,
        // which some servers would reject differently than "no auth attempted".
        let mut request = client.post(url.as_str()).json(&body);
        if let Some(key) = api_key {
            request = request.bearer_auth(key);
        }
        request.send()
    };

    let name = "the custom endpoint";
    let answer = |sent: Result<Result<reqwest::Response, reqwest::Error>, _>| {
        sent.map_err(|_| format!("{name} did not answer within {}s", REMOTE_NOTES_TIMEOUT.as_secs()))?
            .map_err(|e| format!("could not reach {name} ({})", transport_reason(&e)))
    };
    let mut response = answer(tokio::time::timeout(REMOTE_NOTES_TIMEOUT, send(body(true))).await)?;
    if response.status() == reqwest::StatusCode::BAD_REQUEST {
        // A strict server may reject the unfamiliar thinking switches; one
        // retry without them makes it no worse off than before they existed.
        // A 400 with another cause fails again and reports its own message.
        response = answer(tokio::time::timeout(REMOTE_NOTES_TIMEOUT, send(body(false))).await)?;
    }

    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => format!("{name} rejected the request — check the API key in Settings."),
            429 => format!("{name} is rate-limiting this key; try again shortly."),
            code => match error_message(response, api_key).await {
                // Unlike the fixed providers, a self-hosted server's 4xx is
                // usually a fixable typo (Ollama: "invalid model name"), so
                // its own explanation is worth showing.
                Some(message) => format!("{name} returned HTTP {code}: {message}"),
                None => format!("{name} returned HTTP {code}."),
            },
        });
    }

    let body: Value = tokio::time::timeout(REMOTE_NOTES_TIMEOUT, response.json())
        .await
        .map_err(|_| format!("{name} stopped mid-answer"))?
        .map_err(|_| format!("{name} responded, but not in the expected format"))?;

    let text = body["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| format!("{name} responded, but not in the expected format"))?;
    let output = parse_notes_payload(text);
    if output.notes.trim().is_empty() {
        return Err(format!("{name} returned empty notes"));
    }
    Ok(output)
}

/// Longest error message surfaced from a custom endpoint, in characters.
const MAX_ERROR_MESSAGE_CHARS: usize = 200;
/// Most of an error body read looking for that message.
const MAX_ERROR_BODY_BYTES: usize = 16 * 1024;

/// The server's own explanation from an OpenAI-shaped error body
/// (`{"error":{"message":...}}`, or `{"error":"..."}`), which Ollama,
/// llama.cpp, vLLM and mlx_lm all use. Only that parsed field is ever
/// returned — never a raw body, which could echo the transcript — and it is
/// stripped of control characters, has the key redacted, and is bounded.
async fn error_message(mut response: reqwest::Response, api_key: Option<&str>) -> Option<String> {
    let mut body = Vec::new();
    while body.len() < MAX_ERROR_BODY_BYTES {
        match tokio::time::timeout(REMOTE_NOTES_TIMEOUT, response.chunk()).await {
            Ok(Ok(Some(chunk))) => body.extend_from_slice(&chunk),
            _ => break,
        }
    }
    let json: Value = serde_json::from_slice(&body).ok()?;
    let raw = json["error"]["message"].as_str().or(json["error"].as_str())?;
    sanitize_error_message(raw, api_key)
}

fn sanitize_error_message(raw: &str, api_key: Option<&str>) -> Option<String> {
    let mut text = raw.to_string();
    if let Some(key) = api_key.filter(|k| !k.is_empty()) {
        text = text.replace(key, "[redacted]");
    }
    let text = text
        .split(|c: char| c.is_control() || c.is_whitespace())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if text.is_empty() {
        return None;
    }
    if text.chars().count() <= MAX_ERROR_MESSAGE_CHARS {
        return Some(text);
    }
    let cut: String = text.chars().take(MAX_ERROR_MESSAGE_CHARS).collect();
    Some(format!("{cut}…"))
}

/// A base URL with exactly one trailing slash, so appending `v1/chat/...`
/// never produces a double slash regardless of whether the saved value ends
/// in one (`notes_model::validate_base_url` already normalizes this at save
/// time, but this stays defensive for a base URL passed in directly, e.g.
/// from a test or a future caller).
fn ensure_trailing_slash(base_url: &str) -> String {
    if base_url.ends_with('/') {
        base_url.to_string()
    } else {
        format!("{base_url}/")
    }
}

/// Generate notes for `transcript` against a user-configured OpenAI-compatible
/// endpoint. `api_key` is `None` when the user configured no key — many
/// self-hosted servers (`vllm serve`, bare `ollama`) run with no auth at all.
pub async fn run_custom_notes(
    transcript: &str,
    base_url: &str,
    model_id: &str,
    api_key: Option<&str>,
) -> Result<NotesOutput, String> {
    chat_completion(base_url, model_id, api_key, transcript, SYSTEM_PROMPT).await
}

/// A short fixed prompt for "Test connection" — cheap, and never the real
/// notes system prompt, so a test run never depends on transcript content.
const TEST_PROMPT: &str = "Reply with the single word: ok.";

/// What "Test connection" found: the model it tested with (the typed one, or
/// the server's first listed model when none was typed) and every model the
/// server listed, for Settings to offer as choices.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointTest {
    pub model_id: String,
    pub available_models: Vec<String>,
}

/// Listing is a quick metadata call, unlike a chat completion that may wait
/// on a model loading into memory.
const LIST_MODELS_TIMEOUT: Duration = Duration::from_secs(15);
/// Bounds a hostile or runaway listing.
const MAX_LISTED_MODELS: usize = 500;

/// The model ids a server offers via the OpenAI-compatible `GET /v1/models`,
/// which Ollama, llama.cpp's `llama-server`, `mlx_lm.server` and vLLM all
/// implement — so no server needs its own code path. `Ok(vec![])` means the
/// server answered but doesn't list models; an unreachable server or a
/// rejected key is an `Err`, since the chat call would fail the same way.
async fn list_models(base_url: &str, api_key: Option<&str>) -> Result<Vec<String>, String> {
    let client = client()?;
    let url = format!("{}v1/models", ensure_trailing_slash(base_url));
    let mut request = client.get(url);
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }

    let name = "the custom endpoint";
    let response = tokio::time::timeout(LIST_MODELS_TIMEOUT, request.send())
        .await
        .map_err(|_| format!("{name} did not answer within {}s", LIST_MODELS_TIMEOUT.as_secs()))?
        .map_err(|e| format!("could not reach {name} ({})", transport_reason(&e)))?;
    match response.status().as_u16() {
        401 | 403 => {
            return Err(format!("{name} rejected the request — check the API key in Settings."));
        }
        _ if !response.status().is_success() => return Ok(Vec::new()),
        _ => {}
    }
    let body: Value = match tokio::time::timeout(LIST_MODELS_TIMEOUT, response.json()).await {
        Ok(Ok(body)) => body,
        _ => return Ok(Vec::new()),
    };
    Ok(body["data"]
        .as_array()
        .map(|models| {
            models
                .iter()
                .filter_map(|m| m["id"].as_str())
                .filter_map(|id| crate::notes_model::validate_model_id(id).ok())
                .take(MAX_LISTED_MODELS)
                .collect()
        })
        .unwrap_or_default())
}

/// Confirm a not-yet-saved custom endpoint actually works, without persisting
/// anything — Save is a separate, explicit step. Asks the server which models
/// it has first, so a blank model id resolves to one it actually serves, then
/// reuses the exact request path `run_custom_notes` uses, so a passing test
/// genuinely predicts a passing real call.
#[tauri::command]
pub async fn test_custom_notes_endpoint(
    base_url: String,
    model_id: String,
    key: Option<String>,
) -> Result<EndpointTest, String> {
    // Same normalization Save applies, so a test and a save of the same
    // typed values reach the same server with the same request.
    let base_url = crate::notes_model::validate_base_url(&base_url)?;
    let key = normalize_optional_key(key)?;
    let available_models = list_models(&base_url, key.as_deref()).await?;
    let model_id = if model_id.trim().is_empty() {
        available_models.first().cloned().ok_or_else(|| {
            "Enter a model identifier — this server didn't list any models.".to_string()
        })?
    } else {
        crate::notes_model::validate_model_id(&model_id)?
    };

    chat_completion(
        &base_url,
        &model_id,
        key.as_deref(),
        "test connection",
        TEST_PROMPT,
    )
    .await
    .map_err(|e| {
        if available_models.is_empty() || available_models.contains(&model_id) {
            e
        } else {
            format!("{e} (available models: {})", available_models.join(", "))
        }
    })?;
    Ok(EndpointTest {
        model_id,
        available_models,
    })
}

/// A blank key means "no key" (the field is optional); anything else gets
/// the same shape check a saved key does.
fn normalize_optional_key(key: Option<String>) -> Result<Option<String>, String> {
    match key.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(k) => crate::credentials::validate_key(k).map(|k| Some(k.to_string())),
    }
}

fn provider_name(provider: RemoteProvider) -> &'static str {
    match provider {
        RemoteProvider::OpenAi => "OpenAI",
        RemoteProvider::Anthropic => "Anthropic",
        RemoteProvider::Gemini => "Gemini",
    }
}

fn transport_reason(e: &reqwest::Error) -> &'static str {
    if e.is_timeout() {
        "timed out"
    } else if e.is_connect() {
        "could not connect"
    } else {
        "network error"
    }
}

/// Pull the model's answer out of each provider's own response shape.
fn extract_text(provider: RemoteProvider, body: &Value) -> Option<String> {
    let text = match provider {
        RemoteProvider::OpenAi => body["choices"][0]["message"]["content"]
            .as_str()?
            .to_string(),
        RemoteProvider::Anthropic => body["content"]
            .as_array()?
            .iter()
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join(""),
        RemoteProvider::Gemini => body["candidates"][0]["content"]["parts"]
            .as_array()?
            .iter()
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join(""),
    };
    Some(text)
}

/// Test-only seam: a loopback stub of a provider's API, plus the base-URL
/// override that points this module at it. `cfg(test)` so a release build has
/// no way to be repointed at another host (`oats-security` item #8).
#[cfg(test)]
pub(crate) mod testing {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    static BASE_URL: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

    pub(crate) fn base_url() -> Option<String> {
        BASE_URL.read().ok().and_then(|g| g.clone())
    }

    pub(crate) fn set_base_url(base: &str) {
        if let Ok(mut g) = BASE_URL.write() {
            *g = Some(base.to_string());
        }
    }

    pub(crate) fn clear_base_url() {
        if let Ok(mut g) = BASE_URL.write() {
            *g = None;
        }
    }

    /// `main.rs` installs the process-wide rustls provider before any TLS use;
    /// tests never run `main`, so building a client would panic without this.
    pub(crate) fn install_crypto_provider() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
    }

    /// What the provider's server saw, so a test can assert on the request we
    /// actually put on the wire rather than on a mock's recollection of it.
    #[derive(Debug)]
    pub(crate) struct SeenRequest {
        pub(crate) target: String,
        pub(crate) headers: Vec<(String, String)>,
        pub(crate) body: serde_json::Value,
    }

    impl SeenRequest {
        pub(crate) fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.as_str())
        }
    }

    /// Serves exactly one request with the given status and body, then hands
    /// the request back.
    pub(crate) async fn stub_provider(
        status: u16,
        response_body: &str,
    ) -> (String, tokio::task::JoinHandle<SeenRequest>) {
        let (base, handle) = stub_sequence(&[(status, response_body)]).await;
        (base, tokio::spawn(async move { handle.await.unwrap().remove(0) }))
    }

    /// Like `stub_provider`, but answers one request per `(status, body)`
    /// pair, in order — for flows that list models before chatting.
    pub(crate) async fn stub_sequence(
        responses: &[(u16, &str)],
    ) -> (String, tokio::task::JoinHandle<Vec<SeenRequest>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let responses: Vec<(u16, String)> =
            responses.iter().map(|(s, b)| (*s, b.to_string())).collect();
        let handle = tokio::spawn(async move {
            let mut seen = Vec::new();
            for (status, canned) in responses {
                seen.push(serve_one(&listener, status, &canned).await);
            }
            seen
        });
        (base, handle)
    }

    async fn serve_one(listener: &TcpListener, status: u16, canned: &str) -> SeenRequest {
        let (mut socket, _) =
            tokio::time::timeout(std::time::Duration::from_secs(10), listener.accept())
                .await
                .expect("no request reached the provider stub")
                .unwrap();
        let mut raw = Vec::new();
        let mut buf = [0u8; 4096];
        let (head_end, len) = loop {
            let n = socket.read(&mut buf).await.unwrap();
            raw.extend_from_slice(&buf[..n]);
            if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&raw[..pos]).to_string();
                let len = head
                    .lines()
                    .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
                    .and_then(|l| l.split(':').nth(1)?.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                break (pos + 4, len);
            }
            assert!(n > 0, "client closed before sending a request");
        };
        while raw.len() < head_end + len {
            let n = socket.read(&mut buf).await.unwrap();
            assert!(n > 0, "client closed mid-body");
            raw.extend_from_slice(&buf[..n]);
        }
        let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
        let mut lines = head.lines();
        let target = lines
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or_default()
            .to_string();
        let headers = lines
            .filter_map(|l| {
                let (k, v) = l.split_once(':')?;
                Some((k.trim().to_string(), v.trim().to_string()))
            })
            .collect();
        // A GET (e.g. `/v1/models`) has no body.
        let body: serde_json::Value = if len == 0 {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&raw[head_end..head_end + len]).unwrap()
        };

        let resp = format!(
            "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {len}\r\nconnection: close\r\n\r\n{canned}",
            len = canned.len(),
            canned = canned
        );
        socket.write_all(resp.as_bytes()).await.unwrap();
        socket.flush().await.unwrap();
        SeenRequest {
            target,
            headers,
            body,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{
        SeenRequest, clear_base_url, install_crypto_provider, set_base_url, stub_provider,
        stub_sequence,
    };
    use super::*;
    use std::sync::OnceLock;

    /// The base-URL override is process-wide, so tests that use it run one at a
    /// time. Async-aware: the guard is held across the request's await points.
    fn lock() -> &'static tokio::sync::Mutex<()> {
        static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
    }

    async fn run_against(
        provider: RemoteProvider,
        model: &str,
        status: u16,
        response: &str,
    ) -> (Result<NotesOutput, String>, SeenRequest) {
        let _guard = lock().lock().await;
        install_crypto_provider();
        let (base, server) = stub_provider(status, response).await;
        set_base_url(&base);
        let result = run_remote_notes(
            "Alice: ship it.\nBob: agreed.",
            provider,
            model,
            "sk-secret",
        )
        .await;
        let seen = server.await.unwrap();
        clear_base_url();
        (result, seen)
    }

    #[tokio::test]
    async fn openai_is_called_with_a_bearer_key_and_the_transcript() {
        let (result, seen) = run_against(
            RemoteProvider::OpenAi,
            "gpt-5.1",
            200,
            r###"{"choices":[{"message":{"content":"{\"title\":\"Ship It\",\"notes\":\"## Summary\"}"}}]}"###,
        )
        .await;

        assert_eq!(seen.target, "/v1/chat/completions");
        assert_eq!(seen.header("authorization"), Some("Bearer sk-secret"));
        assert_eq!(seen.body["model"], "gpt-5.1");
        let sent = serde_json::to_string(&seen.body).unwrap();
        assert!(sent.contains("Alice: ship it."), "transcript was not sent");

        let out = result.unwrap();
        assert_eq!(out.title.as_deref(), Some("Ship It"));
        assert_eq!(out.notes, "## Summary");
    }

    #[tokio::test]
    async fn the_prompt_asks_for_the_note_shape_the_vault_renders() {
        // The vault turns every bullet under "Action Items" into a dated task,
        // at any depth (vault.rs::render_action_items), so a model that groups
        // them under category and owner bullets produces checkboxes like
        // "- [ ] **Owner: Speaker 1**". The remote prompt has to ask for the
        // same flat shape the sidecar asks for (ariso-stt main.swift).
        let (_, seen) = run_against(
            RemoteProvider::OpenAi,
            "gpt-5.1",
            200,
            r###"{"choices":[{"message":{"content":"{\"title\":\"T\",\"notes\":\"## Summary\"}"}}]}"###,
        )
        .await;

        let system = seen.body["messages"][0]["content"].as_str().unwrap();
        assert!(
            system.contains("level-2"),
            "prompt must pin the heading level: {system}"
        );
        for section in ["Summary", "Key Points", "Decisions", "Action Items"] {
            assert!(system.contains(section), "prompt omits {section}: {system}");
        }
        assert!(
            system.to_lowercase().contains("flat"),
            "prompt must forbid nested bullets: {system}"
        );
        assert!(
            system.to_lowercase().contains("one action item"),
            "prompt must ask for one task per bullet: {system}"
        );
    }

    #[tokio::test]
    async fn anthropic_is_called_with_its_own_auth_header_and_version() {
        let (result, seen) = run_against(
            RemoteProvider::Anthropic,
            "claude-haiku-4-5",
            200,
            r###"{"content":[{"type":"text","text":"{\"title\":\"Ship It\",\"notes\":\"## Summary\"}"}]}"###,
        )
        .await;

        assert_eq!(seen.target, "/v1/messages");
        // Anthropic authenticates with x-api-key, not a bearer token.
        assert_eq!(seen.header("x-api-key"), Some("sk-secret"));
        assert_eq!(seen.header("authorization"), None);
        assert_eq!(seen.header("anthropic-version"), Some("2023-06-01"));
        assert_eq!(seen.body["model"], "claude-haiku-4-5");

        assert_eq!(result.unwrap().notes, "## Summary");
    }

    #[tokio::test]
    async fn gemini_names_its_model_in_the_url_and_keys_off_a_header() {
        let (result, seen) = run_against(
            RemoteProvider::Gemini,
            "gemini-3.7-flash",
            200,
            r###"{"candidates":[{"content":{"parts":[{"text":"{\"title\":\"Ship It\",\"notes\":\"## Summary\"}"}]}}]}"###,
        )
        .await;

        assert_eq!(
            seen.target,
            "/v1beta/models/gemini-3.7-flash:generateContent"
        );
        assert_eq!(seen.header("x-goog-api-key"), Some("sk-secret"));
        assert_eq!(result.unwrap().notes, "## Summary");
    }

    #[tokio::test]
    async fn a_json_answer_wrapped_in_a_code_fence_still_parses() {
        let (result, _) = run_against(
            RemoteProvider::OpenAi,
            "gpt-5.1",
            200,
            r###"{"choices":[{"message":{"content":"```json\n{\"title\":\"T\",\"notes\":\"body\"}\n```"}}]}"###,
        )
        .await;

        let out = result.unwrap();
        assert_eq!(out.title.as_deref(), Some("T"));
        assert_eq!(out.notes, "body");
    }

    #[tokio::test]
    async fn prose_instead_of_json_is_kept_as_the_notes() {
        // Same tolerance the sidecar contract has: better a titleless note than
        // a failed one.
        let (result, _) = run_against(
            RemoteProvider::OpenAi,
            "gpt-5.1",
            200,
            r###"{"choices":[{"message":{"content":"## Summary\n- shipped"}}]}"###,
        )
        .await;

        let out = result.unwrap();
        assert_eq!(out.title, None);
        assert_eq!(out.notes, "## Summary\n- shipped");
    }

    #[tokio::test]
    async fn a_rejected_key_says_so_without_quoting_it() {
        let (result, _) = run_against(
            RemoteProvider::OpenAi,
            "gpt-5.1",
            401,
            r###"{"error":{"message":"Incorrect API key provided: sk-secret"}}"###,
        )
        .await;

        let err = result.unwrap_err();
        assert!(err.to_lowercase().contains("key"), "unhelpful error: {err}");
        // Neither the key nor the provider's echo of it may reach a log.
        assert!(!err.contains("sk-secret"), "error leaked the key: {err}");
    }

    #[tokio::test]
    async fn a_server_error_reports_the_status_and_nothing_else() {
        let (result, _) = run_against(
            RemoteProvider::Anthropic,
            "claude-haiku-4-5",
            500,
            r###"{"error":"transcript said Alice: ship it."}"###,
        )
        .await;

        let err = result.unwrap_err();
        assert!(err.contains("500"), "status missing from: {err}");
        // The body can quote the transcript back at us; it must not be logged.
        assert!(!err.contains("Alice"), "error leaked the transcript: {err}");
    }

    #[tokio::test]
    async fn an_empty_answer_is_an_error_not_an_empty_note() {
        let (result, _) = run_against(
            RemoteProvider::OpenAi,
            "gpt-5.1",
            200,
            r###"{"choices":[{"message":{"content":"   "}}]}"###,
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn a_custom_endpoint_with_a_key_sends_a_bearer_header() {
        install_crypto_provider();
        let (base, server) = stub_provider(
            200,
            r###"{"choices":[{"message":{"content":"{\"title\":\"T\",\"notes\":\"## Summary\"}"}}]}"###,
        )
        .await;

        let result = run_custom_notes(
            "Alice: ship it.",
            &base,
            "Qwen2.5-72B-Instruct",
            Some("sk-custom"),
        )
        .await;
        let seen = server.await.unwrap();

        assert_eq!(seen.target, "/v1/chat/completions");
        assert_eq!(seen.header("authorization"), Some("Bearer sk-custom"));
        assert_eq!(seen.body["model"], "Qwen2.5-72B-Instruct");
        assert_eq!(result.unwrap().notes, "## Summary");
    }

    #[tokio::test]
    async fn a_custom_endpoint_with_no_key_sends_no_authorization_header() {
        install_crypto_provider();
        let (base, server) = stub_provider(
            200,
            r###"{"choices":[{"message":{"content":"{\"title\":\"T\",\"notes\":\"## Summary\"}"}}]}"###,
        )
        .await;

        let result = run_custom_notes("Alice: ship it.", &base, "Qwen2.5-72B-Instruct", None).await;
        let seen = server.await.unwrap();

        // No header at all — never an empty bearer token, which some servers
        // would reject differently than "no auth attempted".
        assert_eq!(seen.header("authorization"), None);
        assert!(result.unwrap().notes.contains("Summary"));
    }

    #[tokio::test]
    async fn a_custom_endpoint_rejecting_the_key_says_so_without_quoting_it() {
        install_crypto_provider();
        let (base, server) = stub_provider(401, r###"{"error":"nope: sk-custom"}"###).await;

        let result = run_custom_notes("Alice: ship it.", &base, "m", Some("sk-custom")).await;
        let _ = server.await.unwrap();

        let err = result.unwrap_err();
        assert!(!err.contains("sk-custom"), "error leaked the key: {err}");
    }

    #[tokio::test]
    async fn a_custom_endpoints_malformed_response_gets_its_own_message() {
        install_crypto_provider();
        let (base, server) = stub_provider(200, r###"{"unexpected": "shape"}"###).await;

        let result = run_custom_notes("Alice: ship it.", &base, "m", None).await;
        let _ = server.await.unwrap();

        let err = result.unwrap_err();
        assert!(
            err.to_lowercase().contains("expected format") || err.to_lowercase().contains("no notes"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn a_custom_endpoints_error_message_is_surfaced() {
        install_crypto_provider();
        // Ollama's actual answer to a model id with a space in it.
        let body =
            r###"{"error":{"message":"invalid model name","type":"invalid_request_error","param":null,"code":null}}"###;
        // Twice: a 400 is retried once without the thinking switches.
        let (base, server) = stub_sequence(&[(400, body), (400, body)]).await;

        let result = run_custom_notes("Alice: ship it.", &base, "qwen 3.5", None).await;
        let _ = server.await.unwrap();

        let err = result.unwrap_err();
        assert!(err.contains("HTTP 400"), "{err}");
        assert!(err.contains("invalid model name"), "{err}");
    }

    #[tokio::test]
    async fn a_surfaced_error_message_never_quotes_the_key_and_is_bounded() {
        install_crypto_provider();
        let body = json!({
            "error": { "message": format!("bad key sk-custom\n{}", "x".repeat(2000)) }
        })
        .to_string();
        let (base, server) = stub_sequence(&[(400, &body), (400, &body)]).await;

        let result = run_custom_notes("Alice: ship it.", &base, "m", Some("sk-custom")).await;
        let _ = server.await.unwrap();

        let err = result.unwrap_err();
        assert!(!err.contains("sk-custom"), "error leaked the key: {err}");
        assert!(!err.contains('\n'), "error kept a control character: {err}");
        assert!(err.chars().count() < 400, "error was not bounded: {} chars", err.chars().count());
    }

    #[tokio::test]
    async fn a_custom_endpoints_unstructured_error_body_is_not_quoted() {
        install_crypto_provider();
        // Only a parsed `error.message` is surfaced — never a raw body, which
        // could echo the transcript back.
        let (base, server) = stub_provider(500, "Alice: ship it. <html>boom</html>").await;

        let result = run_custom_notes("Alice: ship it.", &base, "m", None).await;
        let _ = server.await.unwrap();

        let err = result.unwrap_err();
        assert!(err.contains("HTTP 500"), "{err}");
        assert!(!err.contains("Alice"), "error quoted the body: {err}");
    }

    const NOTES_OK: &str =
        r###"{"choices":[{"message":{"content":"{\"title\":\"T\",\"notes\":\"## Summary\"}"}}]}"###;

    #[tokio::test]
    async fn a_custom_endpoint_request_turns_thinking_off() {
        install_crypto_provider();
        let (base, server) = stub_sequence(&[(200, NOTES_OK)]).await;

        let result = run_custom_notes("Alice: ship it.", &base, "qwen3.5:9b", None).await;
        let seen = server.await.unwrap();

        assert!(result.is_ok(), "{result:?}");
        // Ollama's switch, and the chat-template one llama.cpp, mlx_lm and vLLM read.
        assert_eq!(seen[0].body["reasoning_effort"], "none");
        assert_eq!(seen[0].body["chat_template_kwargs"]["enable_thinking"], false);
    }

    #[tokio::test]
    async fn a_server_rejecting_the_thinking_switches_is_retried_without_them() {
        install_crypto_provider();
        let (base, server) = stub_sequence(&[
            (400, r###"{"error":{"message":"unknown field reasoning_effort"}}"###),
            (200, NOTES_OK),
        ])
        .await;

        let result = run_custom_notes("Alice: ship it.", &base, "m", None).await;
        let seen = server.await.unwrap();

        assert_eq!(result.unwrap().notes, "## Summary");
        assert_eq!(seen.len(), 2);
        assert!(seen[1].body.get("reasoning_effort").is_none(), "{}", seen[1].body);
        assert!(seen[1].body.get("chat_template_kwargs").is_none(), "{}", seen[1].body);
        assert_eq!(seen[1].body["messages"], seen[0].body["messages"]);
    }

    #[tokio::test]
    async fn a_400_that_persists_without_the_switches_reports_the_retrys_error() {
        install_crypto_provider();
        let (base, server) = stub_sequence(&[
            (400, r###"{"error":{"message":"first"}}"###),
            (400, r###"{"error":{"message":"invalid model name"}}"###),
        ])
        .await;

        let result = run_custom_notes("Alice: ship it.", &base, "qwen 3.5", None).await;
        let seen = server.await.unwrap();

        assert_eq!(seen.len(), 2);
        let err = result.unwrap_err();
        assert!(err.contains("HTTP 400: invalid model name"), "{err}");
    }

    #[tokio::test]
    async fn only_a_400_is_retried() {
        install_crypto_provider();
        // One response only: a retry would find nothing listening.
        let (base, server) = stub_sequence(&[(500, r###"{"error":{"message":"boom"}}"###)]).await;

        let result = run_custom_notes("Alice: ship it.", &base, "m", None).await;
        let _ = server.await.unwrap();

        let err = result.unwrap_err();
        assert!(err.contains("HTTP 500: boom"), "{err}");
    }

    const MODELS: &str = r###"{"object":"list","data":[{"id":"qwen3.5:9b","object":"model"},{"id":"gemma4:26b","object":"model"}]}"###;
    const CHAT_OK: &str = r###"{"choices":[{"message":{"content":"ok"}}]}"###;

    #[tokio::test]
    async fn test_connection_with_a_blank_model_uses_the_first_model_the_server_lists() {
        install_crypto_provider();
        let (base, server) = stub_sequence(&[(200, MODELS), (200, CHAT_OK)]).await;

        let result = test_custom_notes_endpoint(base, "  ".to_string(), None).await;
        let seen = server.await.unwrap();

        let result = result.unwrap();
        assert_eq!(result.model_id, "qwen3.5:9b");
        assert_eq!(result.available_models, vec!["qwen3.5:9b", "gemma4:26b"]);
        assert_eq!(seen[0].target, "/v1/models");
        assert_eq!(seen[1].target, "/v1/chat/completions");
        assert_eq!(seen[1].body["model"], "qwen3.5:9b");
    }

    #[tokio::test]
    async fn test_connection_keeps_a_typed_model_and_still_reports_the_list() {
        install_crypto_provider();
        let (base, server) = stub_sequence(&[(200, MODELS), (200, CHAT_OK)]).await;

        let result = test_custom_notes_endpoint(base, "gemma4:26b".to_string(), None).await;
        let seen = server.await.unwrap();

        let result = result.unwrap();
        assert_eq!(result.model_id, "gemma4:26b");
        assert_eq!(result.available_models.len(), 2);
        assert_eq!(seen[1].body["model"], "gemma4:26b");
    }

    #[tokio::test]
    async fn test_connection_names_the_available_models_when_a_typed_one_fails() {
        install_crypto_provider();
        let (base, server) = stub_sequence(&[
            (200, MODELS),
            (400, r###"{"error":{"message":"invalid model name"}}"###),
            (400, r###"{"error":{"message":"invalid model name"}}"###),
        ])
        .await;

        let result = test_custom_notes_endpoint(base, "qwen 3.5".to_string(), None).await;
        let _ = server.await.unwrap();

        let err = result.unwrap_err();
        assert!(err.contains("invalid model name"), "{err}");
        assert!(err.contains("qwen3.5:9b, gemma4:26b"), "{err}");
    }

    #[tokio::test]
    async fn test_connection_falls_back_to_a_typed_model_when_the_server_cannot_list() {
        install_crypto_provider();
        let (base, server) = stub_sequence(&[(404, "404 page not found"), (200, CHAT_OK)]).await;

        let result = test_custom_notes_endpoint(base, "m".to_string(), None).await;
        let seen = server.await.unwrap();

        let result = result.unwrap();
        assert_eq!(result.model_id, "m");
        assert!(result.available_models.is_empty());
        assert_eq!(seen[1].body["model"], "m");
    }

    #[tokio::test]
    async fn test_connection_asks_for_a_model_when_it_is_blank_and_none_are_listed() {
        install_crypto_provider();
        // Only the listing is served: no chat request may follow.
        let (base, server) = stub_sequence(&[(404, "404 page not found")]).await;

        let result = test_custom_notes_endpoint(base, "".to_string(), None).await;
        let _ = server.await.unwrap();

        let err = result.unwrap_err();
        assert!(err.contains("Enter a model identifier"), "{err}");
    }

    #[tokio::test]
    async fn test_connection_normalizes_its_inputs_the_way_save_does() {
        install_crypto_provider();
        let (base, server) = stub_sequence(&[(200, MODELS), (200, CHAT_OK)]).await;

        // Whitespace and a pasted `/v1` path, as a user might type them.
        let result = test_custom_notes_endpoint(
            format!("  {base}/v1  "),
            " qwen3.5:9b ".to_string(),
            Some("  sk-custom  ".to_string()),
        )
        .await;
        let seen = server.await.unwrap();

        assert!(result.is_ok(), "{result:?}");
        assert_eq!(seen[0].target, "/v1/models");
        assert_eq!(seen[0].header("authorization"), Some("Bearer sk-custom"));
        assert_eq!(seen[1].target, "/v1/chat/completions");
        assert_eq!(seen[1].body["model"], "qwen3.5:9b");
        assert_eq!(seen[1].header("authorization"), Some("Bearer sk-custom"));
    }

    #[tokio::test]
    async fn test_connection_treats_a_blank_key_as_no_key() {
        install_crypto_provider();
        let (base, server) = stub_sequence(&[(200, MODELS), (200, CHAT_OK)]).await;

        let result = test_custom_notes_endpoint(base, "m".to_string(), Some("  ".to_string())).await;
        let seen = server.await.unwrap();

        assert!(result.is_ok(), "{result:?}");
        assert_eq!(seen[0].header("authorization"), None);
        assert_eq!(seen[1].header("authorization"), None);
    }

    #[tokio::test]
    async fn test_connection_reports_a_rejected_key() {
        install_crypto_provider();
        let (base, server) = stub_sequence(&[(401, r###"{"error":"no"}"###)]).await;

        let result = test_custom_notes_endpoint(base, "m".to_string(), Some("sk-bad".to_string())).await;
        let _ = server.await.unwrap();

        let err = result.unwrap_err();
        assert!(err.to_lowercase().contains("rejected"), "{err}");
    }

    #[tokio::test]
    async fn test_connection_names_a_connection_failure() {
        install_crypto_provider();
        // Nothing listening on this port: a real connection failure, not a stub.
        let result =
            test_custom_notes_endpoint("http://127.0.0.1:1".to_string(), "m".to_string(), None).await;

        let err = result.unwrap_err();
        assert!(err.to_lowercase().contains("could not reach"), "{err}");
    }
}
