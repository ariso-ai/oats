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
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let canned = response_body.to_string();
        let handle = tokio::spawn(async move {
            // Bounded: a test whose code never sends a request should fail, not hang.
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
            let body: serde_json::Value =
                serde_json::from_slice(&raw[head_end..head_end + len]).unwrap();

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
        });
        (base, handle)
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{
        SeenRequest, clear_base_url, install_crypto_provider, set_base_url, stub_provider,
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
}
