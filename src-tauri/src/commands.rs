use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use tauri_plugin_store::StoreExt;
use url::Url;

const APP_USER_AGENT: &str = "ArisoDesktop/0.2.1";

// Tauri's window lookup and construction are separate operations. These locks
// make each singleton's check-and-build boundary atomic when native menu,
// tray, and webview requests arrive together.
static SETTINGS_WINDOW_CREATION: std::sync::Mutex<()> = std::sync::Mutex::new(());
static LIBRARY_WINDOW_CREATION: std::sync::Mutex<()> = std::sync::Mutex::new(());
// Serializes each temporary Windows z-order presentation with its deferred
// cleanup. The generation lets a cleanup detect that a newer presentation
// superseded it, while the mutex prevents either side from changing the
// topmost state between that validation and the corresponding window calls.
#[cfg(target_os = "windows")]
static LIBRARY_PRESENTATION_GENERATION: std::sync::Mutex<u64> = std::sync::Mutex::new(0);

pub(crate) fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .build()
        .expect("failed to build HTTP client")
}

#[cfg(all(feature = "prod-api", feature = "dev-api"))]
compile_error!("Features `prod-api` and `dev-api` are mutually exclusive");

#[cfg(feature = "prod-api")]
const DEFAULT_API_BASE_URL: &str = "https://api.ari.ariso.ai";
#[cfg(feature = "dev-api")]
const DEFAULT_API_BASE_URL: &str = "https://api-dev.ari.ariso.ai";
#[cfg(not(any(feature = "prod-api", feature = "dev-api")))]
const DEFAULT_API_BASE_URL: &str = "http://localhost:4000";

// Public Pusher client key. dev-api and local both use the dev key, so this
// gates on prod-api only (unlike WEB_APP_BASE_URL's three-way split).
#[cfg(feature = "prod-api")]
pub(crate) const PUSHER_KEY: &str = "ec77b8bc7dc9ff463c13";
#[cfg(not(feature = "prod-api"))]
pub(crate) const PUSHER_KEY: &str = "39d990870841a6b478cc";

pub(crate) const PUSHER_CLUSTER: &str = "us2";

// Public Sentry DSN for opt-in diagnostics (`diagnosticsEnabled` in
// settings.json). A DSN is not a secret — it only grants event ingestion.
// Only prod-api builds ship one so dev/local runs never pollute the project;
// see `sentry_dsn()` for the development escape hatch.
// The DSN belongs to the `oats` project (platform: JavaScript → Vue); both the
// macOS and Windows builds report to it and are told apart by the `os` tag.
#[cfg(feature = "prod-api")]
const DEFAULT_SENTRY_DSN: &str = "https://ee14ea66d04a590f3b4375c1ef651e36@o4510868247216128.ingest.us.sentry.io/4511831828856832";
// Dev builds stay silent unless ARISO_DESKTOP_SENTRY_DSN points somewhere.
#[cfg(not(feature = "prod-api"))]
const DEFAULT_SENTRY_DSN: &str = "";

/// Production builds report to the baked DSN only. Like `api_base_url`, the
/// endpoint is fixed inside the signed app so it cannot be redirected.
#[cfg(feature = "prod-api")]
pub(crate) fn sentry_dsn() -> String {
    DEFAULT_SENTRY_DSN.to_string()
}

/// Development builds default to an empty DSN (diagnostics stay a no-op) but
/// allow pointing at a scratch Sentry project while working on instrumentation.
#[cfg(not(feature = "prod-api"))]
pub(crate) fn sentry_dsn() -> String {
    std::env::var("ARISO_DESKTOP_SENTRY_DSN")
        .ok()
        .filter(|dsn| !dsn.trim().is_empty())
        .map(|dsn| dsn.trim().to_string())
        .unwrap_or_else(|| DEFAULT_SENTRY_DSN.to_string())
}

#[cfg(feature = "prod-api")]
const DEFAULT_WEB_APP_BASE_URL: &str = "https://web.ari.ariso.ai";
#[cfg(feature = "dev-api")]
const DEFAULT_WEB_APP_BASE_URL: &str = "https://web-dev.ari.ariso.ai";
#[cfg(not(any(feature = "prod-api", feature = "dev-api")))]
const DEFAULT_WEB_APP_BASE_URL: &str = "http://localhost:5173";

/// Resolve the production API origin from the baked binary constant. Production
/// builds intentionally ignore environment overrides so deployment endpoints
/// cannot be changed outside the signed app.
#[cfg(feature = "prod-api")]
pub(crate) fn api_base_url() -> String {
    DEFAULT_API_BASE_URL.to_string()
}

/// Resolve the API origin used by desktop-native HTTP calls in development.
/// Non-production launchers can point the app at an isolated Agents dev stack.
#[cfg(not(feature = "prod-api"))]
pub(crate) fn api_base_url() -> String {
    std::env::var("ARISO_DESKTOP_API_BASE_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_API_BASE_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

/// Resolve the production web origin from the baked binary constant. Production
/// builds intentionally ignore environment overrides to keep deep links fixed.
#[cfg(feature = "prod-api")]
pub(crate) fn web_app_base_url() -> String {
    DEFAULT_WEB_APP_BASE_URL.to_string()
}

/// Resolve the browser-facing web origin used for deep links in development.
/// Keeping this separate from the API origin matches the Agents dev.sh Caddy /api routing.
#[cfg(not(feature = "prod-api"))]
pub(crate) fn web_app_base_url() -> String {
    std::env::var("ARISO_DESKTOP_WEB_APP_BASE_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_WEB_APP_BASE_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

const STORE_PATH: &str = "session.json";
const SESSION_KEY: &str = "session_token";
const SETTINGS_PATH: &str = "settings.json";

/// Read the active runtime backend ("ariso" | "local"); defaults to "ariso".
pub(crate) fn active_backend(app: &tauri::AppHandle) -> String {
    app.store(SETTINGS_PATH)
        .ok()
        .and_then(|s| s.get("backend"))
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "ariso".to_string())
}

/// Whether both on-device models are downloaded for the Local backend. Resolves
/// the models root; treats an unresolvable root as "not ready" so recording is
/// gated rather than crashing.
pub(crate) fn local_models_ready() -> bool {
    match crate::storage::ariso_root() {
        Ok(root) => crate::model_manager::local_models_ready(&root),
        Err(_) => false,
    }
}

/// Surface the (pre-created) Settings window and emit `tray://show-model-prompt`
/// so its on-device-models section auto-starts the missing downloads. Shared by
/// every recording entry point that gates on Local model readiness.
pub(crate) fn surface_model_download(app: &tauri::AppHandle) {
    let _ = open_settings_window(app);
    let _ = app.emit("tray://show-model-prompt", ());
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SignInResult {
    pub success: Option<bool>,
    #[serde(rename = "sessionToken")]
    pub session_token: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct SessionResult {
    #[serde(rename = "sessionToken")]
    pub session_token: String,
}

#[derive(Serialize, Deserialize)]
pub struct ApiResponse {
    pub status: u16,
    pub data: serde_json::Value,
}

#[derive(Deserialize)]
struct PrepareStateResponse {
    #[serde(rename = "redirectUrl")]
    redirect_url: String,
}

pub(crate) fn get_session_token(app: &tauri::AppHandle) -> Option<String> {
    let store = app.store(STORE_PATH).ok()?;
    store
        .get(SESSION_KEY)
        .and_then(|v| v.as_str().map(String::from))
}

/// Validate the stored session against the API. Clears the stored token
/// if the server reports it as invalid so subsequent checks return false
/// and the UI can prompt the user to sign in again.
pub async fn is_session_valid(app: &tauri::AppHandle) -> bool {
    let token = match get_session_token(app) {
        Some(t) if !t.is_empty() => t,
        _ => return false,
    };

    let client = http_client();
    let response = match client
        .get(format!("{}/auth/session", api_base_url()))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json")
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return false,
    };

    if response.status().is_success() {
        return true;
    }

    let _ = clear_session_token(app);
    false
}

fn set_session_token(app: &tauri::AppHandle, token: &str) -> Result<(), String> {
    let store = app.store(STORE_PATH).map_err(|e| e.to_string())?;
    store.set(SESSION_KEY, serde_json::json!(token));
    store.save().map_err(|e| e.to_string())
}

pub(crate) fn clear_session_token(app: &tauri::AppHandle) -> Result<(), String> {
    let store = app.store(STORE_PATH).map_err(|e| e.to_string())?;
    store.delete(SESSION_KEY);
    store.save().map_err(|e| e.to_string())
}

/// Error string emitted when a pending browser sign-in is canceled. The
/// frontend treats this value as a silent cancel, not a failure to display.
pub(crate) const SIGN_IN_CANCELED: &str = "Sign-in canceled";

/// How long the loopback listener waits for the browser to deliver the
/// magic-link token before the flow fails with a retryable error.
const SIGN_IN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// At most one browser sign-in flow is pending at a time; starting a new one
/// (or canceling) aborts the previous listener task. The attempt is tracked
/// from the moment `google_sign_in` is invoked — before the prepare-state
/// request, not just once the loopback listener task exists — so a cancel
/// that arrives while prepare-state is in flight still has something to
/// cancel instead of being silently dropped.
/// Which browser flow owns the slot. Cancel must resolve the frontend promise
/// that is actually waiting, and the two flows listen on different events.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum BrowserFlow {
    SignIn,
    CalendarConnect,
}

impl BrowserFlow {
    fn result_event(self) -> &'static str {
        match self {
            BrowserFlow::SignIn => "oauth-result",
            BrowserFlow::CalendarConnect => "calendar-connect-result",
        }
    }

    /// The loopback success page for this flow. Connect runs while the user is
    /// already signed in, so it must not claim a sign-in happened; it also
    /// stays neutral about the grant, because the browser hop reaches this page
    /// whether or not the user left Calendar ticked on the consent screen.
    fn callback_ok_response(self) -> &'static str {
        match self {
            BrowserFlow::SignIn => CALLBACK_SIGNED_IN_RESPONSE,
            BrowserFlow::CalendarConnect => CALLBACK_DONE_RESPONSE,
        }
    }
}

struct PendingSignIn {
    id: u64,
    flow: BrowserFlow,
    handle: Option<tauri::async_runtime::JoinHandle<()>>,
}

static SIGN_IN_ATTEMPT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static PENDING_SIGN_IN: std::sync::Mutex<Option<PendingSignIn>> = std::sync::Mutex::new(None);

fn abort_pending_handle(handle: tauri::async_runtime::JoinHandle<()>) {
    if !handle.inner().is_finished() {
        handle.abort();
    }
}

/// Register a new attempt and take the slot, aborting whatever attempt (with
/// or without a listener task yet) was previously in it. Returns the new
/// attempt's id, checked at each subsequent await via `sign_in_attempt_active`.
fn begin_sign_in_attempt(flow: BrowserFlow) -> u64 {
    let id = SIGN_IN_ATTEMPT.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    let mut slot = PENDING_SIGN_IN.lock().unwrap();
    if let Some(prev) = slot.take() {
        if let Some(handle) = prev.handle {
            abort_pending_handle(handle);
        }
    }
    *slot = Some(PendingSignIn {
        id,
        flow,
        handle: None,
    });
    id
}

/// Whether `id` is still the active attempt, i.e. it hasn't been superseded
/// by a newer `google_sign_in` call or dropped by `cancel_google_sign_in`.
fn sign_in_attempt_active(id: u64) -> bool {
    matches!(&*PENDING_SIGN_IN.lock().unwrap(), Some(p) if p.id == id)
}

/// Attach the loopback listener task to attempt `id` now that it exists. If
/// the attempt was superseded or canceled while spawning, the slot no longer
/// matches — abort the just-spawned task immediately rather than leaking it.
fn attach_sign_in_handle(id: u64, handle: tauri::async_runtime::JoinHandle<()>) {
    let mut slot = PENDING_SIGN_IN.lock().unwrap();
    match slot.as_mut() {
        Some(p) if p.id == id => p.handle = Some(handle),
        _ => abort_pending_handle(handle),
    }
}

/// Cancel whatever browser attempt is pending, if any. Returns the flow that
/// was cancelled so the caller emits on the event its waiter is listening to —
/// cancelling a calendar connect with an `oauth-result` would leave the
/// frontend promise hanging until the timeout.
fn abort_pending_sign_in() -> Option<BrowserFlow> {
    match PENDING_SIGN_IN.lock().unwrap().take() {
        Some(p) => {
            if let Some(handle) = p.handle {
                abort_pending_handle(handle);
            }
            Some(p.flow)
        }
        None => None,
    }
}

/// Atomically retire attempt `id` — clear the slot only if it still holds
/// `id` — and report whether it did. Used right before a result is emitted so
/// a cancel that arrives after the listener task's final await (too late for
/// `JoinHandle::abort` to stop, since abort only takes effect at an await
/// point) still prevents the stale result from being published, and so a
/// naturally completed attempt doesn't linger in the slot for a later
/// `cancel_google_sign_in` to emit a bogus cancellation over.
fn retire_sign_in_attempt(id: u64) -> bool {
    let mut slot = PENDING_SIGN_IN.lock().unwrap();
    match slot.as_ref() {
        Some(p) if p.id == id => {
            *slot = None;
            true
        }
        _ => false,
    }
}

/// Owns the slot for the stretch between `begin_sign_in_attempt` and the
/// moment the loopback listener task takes over. Every fallible step in that
/// prologue — bind, prepare-state, URL validation, opening the browser — would
/// otherwise leave the attempt parked in the slot on the way out, and a later
/// `cancel_google_sign_in` would emit a result for a flow that already died.
/// Call `release` once the listener task exists; until then, any early return
/// retires the attempt on drop.
struct SignInAttemptGuard(Option<u64>);

impl SignInAttemptGuard {
    fn begin(flow: BrowserFlow) -> Self {
        Self(Some(begin_sign_in_attempt(flow)))
    }

    fn id(&self) -> u64 {
        self.0.expect("attempt id read after release")
    }

    /// Hand the attempt to the listener task: it now owns retiring the slot,
    /// so dropping this guard must no longer do it.
    fn release(mut self) -> u64 {
        self.0.take().expect("attempt released twice")
    }
}

impl Drop for SignInAttemptGuard {
    fn drop(&mut self) {
        if let Some(id) = self.0 {
            retire_sign_in_attempt(id);
        }
    }
}

/// Constant-time-ish nonce check: comparing SHA-256 digests instead of the
/// raw strings keeps a local process from recovering the nonce byte-by-byte
/// through comparison timing.
fn nonce_matches(candidate: &str, expected: &str) -> bool {
    use sha2::{Digest, Sha256};
    Sha256::digest(candidate.as_bytes()) == Sha256::digest(expected.as_bytes())
}

/// 32 hex chars from the OS RNG. Binds the loopback callback to this sign-in
/// attempt so nothing else on the machine can forge a token delivery.
fn random_nonce() -> Result<String, String> {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).map_err(|e| format!("RNG unavailable: {e}"))?;
    Ok(hex::encode(buf))
}

/// The web-app path passed to `/oauth2/prepare-state` as `redirect`. The API's
/// sign-in callback recognizes this shape and redirects the magic-link token
/// to `http://127.0.0.1:<port>/callback` instead of the web magic-link page.
fn desktop_auth_redirect(port: u16, nonce: &str) -> String {
    format!("/desktop-auth?callback_port={port}&nonce={nonce}")
}

/// The auth URL handed to the default browser comes from server data (the
/// identity provider's own consent URL, e.g. accounts.google.com — its host
/// isn't known ahead of time, so https is allowed generally). The plain-http
/// loopback exception is for local dev API builds only; production must
/// never open a local HTTP auth URL.
fn validate_browser_auth_url(url: &Url) -> Result<(), String> {
    match url.scheme() {
        "https" => Ok(()),
        #[cfg(not(feature = "prod-api"))]
        "http" if matches!(url.host_str(), Some("localhost") | Some("127.0.0.1")) => Ok(()),
        other => Err(format!("refusing to open auth URL with scheme {other}")),
    }
}

/// What a loopback callback delivered. Sign-in returns a magic-link token;
/// the Workspace connect hop returns only a status marker and no secret.
#[derive(Debug, PartialEq)]
pub(crate) enum LoopbackDelivery {
    Token(String),
    Status(String),
}

/// Parse an HTTP request line (`GET /callback?token=…&nonce=… HTTP/1.1`, or
/// `?status=…&nonce=…` for the connect hop) from the loopback listener.
/// Returns `(delivery, nonce)` for well-formed callbacks.
fn parse_loopback_callback(request_line: &str) -> Option<(LoopbackDelivery, String)> {
    let mut parts = request_line.split_whitespace();
    if parts.next() != Some("GET") {
        return None;
    }
    let target = parts.next()?;
    let url = Url::parse(&format!("http://127.0.0.1{target}")).ok()?;
    if url.path() != "/callback" {
        return None;
    }
    let mut token = None;
    let mut status = None;
    let mut nonce = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "token" if !value.is_empty() => token = Some(value.into_owned()),
            "status" if !value.is_empty() => status = Some(value.into_owned()),
            "nonce" if !value.is_empty() => nonce = Some(value.into_owned()),
            _ => {}
        }
    }
    // A token always wins: a callback carrying one is a sign-in delivery even
    // if it also carries a status, so a status parameter can never downgrade
    // sign-in into the token-less path.
    let delivery = match (token, status) {
        (Some(token), _) => LoopbackDelivery::Token(token),
        (None, Some(status)) => LoopbackDelivery::Status(status),
        (None, None) => return None,
    };
    Some((delivery, nonce?))
}

// Loopback responses. The success page must never echo the token, and its
// wording must match the flow that opened the browser — see
// `BrowserFlow::callback_ok_response`.
macro_rules! callback_ok_response {
    ($heading:literal, $body:literal) => {
        concat!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n",
            "<!doctype html><html><head><title>oats</title></head>",
            "<body style=\"font-family:-apple-system,system-ui,sans-serif;background:#f5f5f7;",
            "display:flex;align-items:center;justify-content:center;height:100vh;margin:0\">",
            "<div style=\"text-align:center\"><h2>",
            $heading,
            "</h2><p>",
            $body,
            "</p></div></body></html>"
        )
    };
}

const CALLBACK_SIGNED_IN_RESPONSE: &str = callback_ok_response!(
    "You&rsquo;re signed in",
    "You can close this tab and return to oats."
);

const CALLBACK_DONE_RESPONSE: &str = callback_ok_response!(
    "You can close this tab",
    "oats has the result. Return to the app to continue."
);

const CALLBACK_NOT_FOUND_RESPONSE: &str =
    "HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";

/// Accept loopback connections until one delivers `/callback` with a token and
/// the expected nonce. Requests that don't match get a 404 and don't consume
/// the listener, so stray or forged hits can't terminate a pending sign-in.
async fn accept_loopback_callback(
    listener: &tokio::net::TcpListener,
    expected_nonce: &str,
    flow: BrowserFlow,
) -> Result<LoopbackDelivery, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    loop {
        let (mut conn, _) = listener.accept().await.map_err(|e| e.to_string())?;

        // Bound per-connection work so a stalled client can't hold the loop
        // (connections are handled serially; a real browser delivers the
        // request line in milliseconds).
        let handled = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            let mut buf = vec![0u8; 8192];
            let mut len = 0;
            while len < buf.len() {
                let n = match conn.read(&mut buf[len..]).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                len += n;
                if buf[..len].windows(2).any(|w| w == b"\r\n") {
                    break;
                }
            }
            let request_line = String::from_utf8_lossy(&buf[..len])
                .lines()
                .next()
                .unwrap_or_default()
                .to_string();

            let matched = parse_loopback_callback(&request_line)
                .filter(|(_, nonce)| nonce_matches(nonce, expected_nonce))
                .map(|(delivery, _)| delivery);

            let response = if matched.is_some() {
                flow.callback_ok_response()
            } else {
                CALLBACK_NOT_FOUND_RESPONSE
            };
            let _ = conn.write_all(response.as_bytes()).await;
            let _ = conn.shutdown().await;
            matched
        })
        .await;

        if let Ok(Some(delivery)) = handled {
            return Ok(delivery);
        }
    }
}

/// Drive the post-browser half of sign-in: wait for the loopback callback,
/// exchange the magic-link token for a session, and report via `oauth-result`.
/// Emits on `window` (the webview that started this attempt) rather than
/// broadcasting the session token to every window. Retires `attempt_id`
/// immediately before emitting so a cancel that arrives too late to abort
/// this task (abort only takes effect at an await point, and there are none
/// left after the callback resolves) still suppresses the stale result.
async fn run_browser_sign_in(
    attempt_id: u64,
    window: tauri::WebviewWindow,
    listener: tokio::net::TcpListener,
    nonce: String,
) {
    let result = match tokio::time::timeout(
        SIGN_IN_TIMEOUT,
        accept_loopback_callback(&listener, &nonce, BrowserFlow::SignIn),
    )
    .await
    {
        Ok(Ok(LoopbackDelivery::Token(token))) => {
            exchange_token_for_session(window.app_handle(), &token).await
        }
        // Sign-in requires a magic-link token; a status-only callback on
        // this attempt's nonce is not one.
        Ok(Ok(LoopbackDelivery::Status(_))) => SignInResult {
            success: None,
            session_token: None,
            error: Some("Sign-in callback carried no token".into()),
        },
        Ok(Err(err)) => SignInResult {
            success: None,
            session_token: None,
            error: Some(err),
        },
        Err(_) => SignInResult {
            success: None,
            session_token: None,
            error: Some("Sign-in timed out — please try again".into()),
        },
    };
    if retire_sign_in_attempt(attempt_id) {
        let _ = window.emit(BrowserFlow::SignIn.result_event(), result);
    }
}

/// Initiates Google OAuth sign-in in the user's default browser (native
/// webviews break passkeys and are blocked by identity providers). A loopback
/// listener bound before the flow starts receives the magic-link token from
/// the API's desktop redirect, and the token is exchanged for a session.
#[tauri::command]
pub async fn google_sign_in(window: tauri::WebviewWindow) -> Result<SignInResult, String> {
    use tauri_plugin_opener::OpenerExt;

    // Register this attempt before any await — a cancel that arrives while
    // prepare-state is in flight still has something in the slot to cancel,
    // instead of being silently dropped because no listener task exists yet.
    // This also supersedes any still-pending attempt (e.g. the user closed
    // the browser tab and clicked Sign in again). The guard retires it again
    // if any step below fails before the listener task takes ownership.
    let attempt = SignInAttemptGuard::begin(BrowserFlow::SignIn);
    let attempt_id = attempt.id();

    // Bind before prepare-state so the advertised port is already ours.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| format!("bind loopback listener: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let nonce = random_nonce()?;

    // Step 1: Get the OAuth redirect URL from the API. The backend expands
    // these service names into Google scopes and owns credential persistence;
    // `redirect` tells its sign-in callback to deliver the magic-link token to
    // our loopback listener instead of the web app.
    let client = http_client();
    let response = client
        .post(format!("{}/oauth2/prepare-state", api_base_url()))
        .header(CONTENT_TYPE, "application/json")
        .json(&serde_json::json!({
            "integration": "google-signin",
            "scopes": ["calendar-readonly"],
            "newUserSignupIntent": "personal_unless_domain_autojoin",
            "redirect": desktop_auth_redirect(port, &nonce),
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !sign_in_attempt_active(attempt_id) {
        return Ok(SignInResult {
            success: None,
            session_token: None,
            error: Some(SIGN_IN_CANCELED.into()),
        });
    }

    if !response.status().is_success() {
        let status = response.status().as_u16();
        return Ok(SignInResult {
            success: None,
            session_token: None,
            error: Some(format!("API returned {status}")),
        });
    }

    let body: PrepareStateResponse = response.json().await.map_err(|e| e.to_string())?;

    if !sign_in_attempt_active(attempt_id) {
        return Ok(SignInResult {
            success: None,
            session_token: None,
            error: Some(SIGN_IN_CANCELED.into()),
        });
    }

    // Step 2: Hand the auth URL to the default browser.
    let auth_url = Url::parse(&body.redirect_url).map_err(|e| e.to_string())?;
    validate_browser_auth_url(&auth_url)?;

    if !sign_in_attempt_active(attempt_id) {
        return Ok(SignInResult {
            success: None,
            session_token: None,
            error: Some(SIGN_IN_CANCELED.into()),
        });
    }

    window
        .opener()
        .open_url(body.redirect_url, None::<&str>)
        .map_err(|e| e.to_string())?;

    // Step 3: Wait for the browser to hit the loopback callback. If this
    // attempt was superseded or canceled while the browser was opening,
    // `attach_sign_in_handle` aborts the task instead of letting it run.
    let attempt_id = attempt.release();
    let handle =
        tauri::async_runtime::spawn(run_browser_sign_in(attempt_id, window, listener, nonce));
    attach_sign_in_handle(attempt_id, handle);

    // Return immediately — the frontend listens for the "oauth-result" event
    Ok(SignInResult {
        success: None,
        session_token: None,
        error: None,
    })
}

/// Abort a pending browser sign-in (the user gave up waiting). Resolves the
/// frontend's pending `oauth-result` wait with the silent-cancel error.
#[tauri::command]
pub async fn cancel_google_sign_in(window: tauri::WebviewWindow) -> Result<(), String> {
    match abort_pending_sign_in() {
        Some(BrowserFlow::SignIn) => {
            let _ = window.emit(
                BrowserFlow::SignIn.result_event(),
                SignInResult {
                    success: None,
                    session_token: None,
                    error: Some(SIGN_IN_CANCELED.into()),
                },
            );
        }
        Some(BrowserFlow::CalendarConnect) => {
            let _ = window.emit(
                BrowserFlow::CalendarConnect.result_event(),
                calendar_connect_error(SIGN_IN_CANCELED),
            );
        }
        None => {}
    }
    Ok(())
}

/// Outcome of the Workspace connect hop. `status` mirrors the marker the API's
/// callback put on the loopback URL: "connected", or "no_calendar_scope" when
/// the user unticked Calendar on the granular-consent screen.
#[derive(Clone, Serialize)]
pub struct CalendarConnectResult {
    pub status: Option<String>,
    pub error: Option<String>,
}

fn calendar_connect_error(message: impl Into<String>) -> CalendarConnectResult {
    CalendarConnectResult {
        status: None,
        error: Some(message.into()),
    }
}

/// Wait for the connect callback and report via `calendar-connect-result`.
/// Mirrors `run_browser_sign_in`, but nothing secret crosses the loopback —
/// only the status marker — so there is no token to exchange.
async fn run_calendar_connect(
    attempt_id: u64,
    window: tauri::WebviewWindow,
    listener: tokio::net::TcpListener,
    nonce: String,
) {
    let result = match tokio::time::timeout(
        SIGN_IN_TIMEOUT,
        accept_loopback_callback(&listener, &nonce, BrowserFlow::CalendarConnect),
    )
    .await
    {
        Ok(Ok(LoopbackDelivery::Status(status))) => CalendarConnectResult {
            status: Some(status),
            error: None,
        },
        // A token on this attempt's nonce means the sign-in callback landed
        // here. Never exchange it — this flow already holds a session, and
        // the token belongs to the sign-in attempt that minted the nonce.
        Ok(Ok(LoopbackDelivery::Token(_))) => {
            calendar_connect_error("Unexpected sign-in callback during calendar connect")
        }
        Ok(Err(err)) => calendar_connect_error(err),
        Err(_) => calendar_connect_error("Connecting Calendar timed out — please try again"),
    };
    if retire_sign_in_attempt(attempt_id) {
        let _ = window.emit(BrowserFlow::CalendarConnect.result_event(), result);
    }
}

/// Second hop of desktop Google auth: acquire Calendar read access through the
/// authenticated Workspace connect flow, which sets `include_granted_scopes`
/// and is therefore additive — it can never narrow an existing grant the way
/// sign-in would. Only run when `/desktop/google-calendar-status` reports
/// Calendar missing.
#[tauri::command]
pub async fn connect_google_calendar(
    window: tauri::WebviewWindow,
) -> Result<CalendarConnectResult, String> {
    use tauri_plugin_opener::OpenerExt;

    let app = window.app_handle().clone();

    // Offline mode must make no network calls at all. Calendar sync is an
    // Ariso-cloud feature, so this is unreachable in Local by design.
    if active_backend(&app) == "local" {
        return Err("Calendar connect is unavailable on the Local backend".into());
    }

    let Some(session_token) = get_session_token(&app) else {
        return Err("Not signed in".into());
    };

    // As in `google_sign_in`: the guard hands the slot back if any step below
    // fails before the listener task takes ownership of the attempt.
    let attempt = SignInAttemptGuard::begin(BrowserFlow::CalendarConnect);
    let attempt_id = attempt.id();

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| format!("bind loopback listener: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let nonce = random_nonce()?;

    // Authenticated, unlike sign-in's prepare-state: the API resolves the
    // caller's Google Workspace MCP server row from the session.
    let client = http_client();
    let response = client
        .post(format!("{}/oauth2/prepare-state", api_base_url()))
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {session_token}"))
        .json(&serde_json::json!({
            "integration": "googleWorkspace",
            "scopes": ["calendar-readonly"],
            "redirect": desktop_auth_redirect(port, &nonce),
        }))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !sign_in_attempt_active(attempt_id) {
        return Ok(calendar_connect_error(SIGN_IN_CANCELED));
    }

    if !response.status().is_success() {
        let status = response.status().as_u16();
        return Ok(calendar_connect_error(format!("API returned {status}")));
    }

    let body: PrepareStateResponse = response.json().await.map_err(|e| e.to_string())?;

    if !sign_in_attempt_active(attempt_id) {
        return Ok(calendar_connect_error(SIGN_IN_CANCELED));
    }

    let auth_url = Url::parse(&body.redirect_url).map_err(|e| e.to_string())?;
    validate_browser_auth_url(&auth_url)?;

    if !sign_in_attempt_active(attempt_id) {
        return Ok(calendar_connect_error(SIGN_IN_CANCELED));
    }

    window
        .opener()
        .open_url(body.redirect_url, None::<&str>)
        .map_err(|e| e.to_string())?;

    let attempt_id = attempt.release();
    let handle =
        tauri::async_runtime::spawn(run_calendar_connect(attempt_id, window, listener, nonce));
    attach_sign_in_handle(attempt_id, handle);

    // The frontend listens for "calendar-connect-result".
    Ok(CalendarConnectResult {
        status: None,
        error: None,
    })
}

async fn exchange_token_for_session(
    app: &tauri::AppHandle,
    token: &str,
) -> SignInResult {
    let client = http_client();

    let response = match client
        .get(format!("{}/auth/check", api_base_url()))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return SignInResult {
                success: None,
                session_token: None,
                error: Some(e.to_string()),
            };
        }
    };

    if !response.status().is_success() {
        let status = response.status().as_u16();
        return SignInResult {
            success: None,
            session_token: None,
            error: Some(format!("Auth check failed: {status}")),
        };
    }

    // Extract session_token from Set-Cookie headers
    let mut session_token = String::new();
    for value in response.headers().get_all("set-cookie") {
        if let Ok(cookie_str) = value.to_str() {
            if cookie_str.starts_with("session_token=") {
                if let Some(val) = cookie_str
                    .strip_prefix("session_token=")
                    .and_then(|s| s.split(';').next())
                {
                    session_token = val.to_string();
                }
            }
        }
    }

    if !session_token.is_empty() {
        if let Err(e) = set_session_token(app, &session_token) {
            return SignInResult {
                success: None,
                session_token: None,
                error: Some(e),
            };
        }
    }

    SignInResult {
        success: Some(true),
        session_token: Some(session_token),
        error: None,
    }
}

/// Check if there is a valid existing session
#[tauri::command]
pub async fn check_session(app: tauri::AppHandle) -> Result<Option<SessionResult>, String> {
    let token = match get_session_token(&app) {
        Some(t) => t,
        None => return Ok(None),
    };

    let client = http_client();
    let response = client
        .get(format!("{}/auth/session", api_base_url()))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Ok(None);
    }

    Ok(Some(SessionResult {
        session_token: token,
    }))
}

/// Clear the stored session
#[tauri::command]
pub async fn sign_out(app: tauri::AppHandle) -> Result<(), String> {
    clear_session_token(&app)
}

/// Proxy API requests with authentication
#[tauri::command]
pub async fn api_request(
    app: tauri::AppHandle,
    method: String,
    path: String,
    body: Option<serde_json::Value>,
) -> Result<ApiResponse, String> {
    let token = get_session_token(&app).unwrap_or_default();
    let client = http_client();
    let url = format!("{}{}", api_base_url(), path);

    let mut request = match method.to_uppercase().as_str() {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "PATCH" => client.patch(&url),
        "DELETE" => client.delete(&url),
        _ => return Err(format!("Unsupported HTTP method: {method}")),
    };

    request = request
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json");

    if let Some(body) = body {
        if method.to_uppercase() != "GET" {
            request = request.json(&body);
        }
    }

    let response = request.send().await.map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let data: serde_json::Value = response
        .json()
        .await
        .unwrap_or(serde_json::Value::Null);

    Ok(ApiResponse { status, data })
}

/// Upload a file via multipart/form-data with authentication
#[tauri::command]
pub async fn upload_file(
    app: tauri::AppHandle,
    path: String,
    file_data: Vec<u8>,
    file_name: String,
    fields: std::collections::HashMap<String, String>,
) -> Result<ApiResponse, String> {
    let token = get_session_token(&app).unwrap_or_default();
    let client = http_client();
    let url = format!("{}{}", api_base_url(), path);

    let file_part = reqwest::multipart::Part::bytes(file_data)
        .file_name(file_name)
        .mime_str("audio/mpeg")
        .map_err(|e| e.to_string())?;

    let mut form = reqwest::multipart::Form::new().part("file", file_part);

    for (key, value) in fields {
        form = form.text(key, value);
    }

    let response = client
        .post(&url)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .multipart(form)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status().as_u16();
    let data: serde_json::Value = response.json().await.unwrap_or(serde_json::Value::Null);

    Ok(ApiResponse { status, data })
}

#[tauri::command]
pub async fn set_tray_recording(app: tauri::AppHandle, is_recording: bool, is_paused: bool) -> Result<(), String> {
    let state = app.state::<crate::recording_state::RecordingState>();
    if is_recording {
        // Mark capture before redrawing the tray so the title refresh sees the
        // active recording and clears the countdown text in the menu bar.
        state.mark_capture_active();
    } else {
        state.clear();
        let _ = app.emit("recording://state", false);
    }
    crate::tray::set_menu(&app, is_recording, is_paused);
    Ok(())
}

#[tauri::command]
pub async fn create_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    open_settings_window(&app)
}

/// Show or create the one Settings window. The creation lock protects the
/// check/build pair so simultaneous native launchers cannot register duplicate
/// webviews under the same label.
pub(crate) fn open_settings_window(app: &tauri::AppHandle) -> Result<(), String> {
    let _creation = SETTINGS_WINDOW_CREATION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.unminimize();
        win.show().map_err(|e: tauri::Error| e.to_string())?;
        win.set_focus().map_err(|e: tauri::Error| e.to_string())?;
        return Ok(());
    }

    let win = crate::window_style::settings_window_builder(app)
        .build()
        .map_err(|e| e.to_string())?;
    if let Err(e) = crate::window_style::install_settings_window_behavior(&win) {
        let _ = win.destroy();
        return Err(e.to_string());
    }
    win.show().map_err(|e| e.to_string())?;
    win.set_focus().map_err(|e| e.to_string())?;

    Ok(())
}

/// Reject a vault path that isn't absolute. Split out so it is unit-testable
/// without an AppHandle.
pub(crate) fn validate_vault_path(path: &str) -> Result<(), String> {
    if !std::path::Path::new(path).is_absolute() {
        return Err("Vault path must be an absolute directory.".to_string());
    }
    Ok(())
}

/// The active local vault directory (resolved absolute path), for Settings.
#[tauri::command]
pub fn get_vault_dir() -> Result<String, String> {
    Ok(crate::vault::vault_root()?.to_string_lossy().into_owned())
}

/// Point the local backend at a new vault directory. Treats it as a fresh,
/// independent store: no existing data is copied. Rejected while recording.
#[tauri::command]
pub fn set_vault_dir(app: tauri::AppHandle, path: String) -> Result<(), String> {
    if app
        .state::<crate::recording_state::RecordingState>()
        .is_active()
    {
        return Err("Can't change the vault while recording.".to_string());
    }
    validate_vault_path(&path)?;
    let dir = std::path::PathBuf::from(&path);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create vault dir: {e}"))?;
    let previous = crate::vault::current_vault_override();
    crate::vault::set_vault_override(dir);
    // Run all fallible steps; on any failure, restore the previous vault state
    // so the process and the persisted setting stay in sync.
    let result = (|| -> Result<(), String> {
        crate::vault::ensure_vault()?;
        let store = app.store(SETTINGS_PATH).map_err(|e| e.to_string())?;
        store.set("vaultDir", serde_json::json!(path));
        store.save().map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        crate::vault::restore_vault_override(previous);
        return result;
    }
    let _ = app.emit("vault://changed", ());
    Ok(())
}

/// Open the dedicated first-run onboarding window. It is separate from Settings
/// so a fresh install can explain sign-in before the main preferences surface.
#[tauri::command]
pub async fn create_onboarding_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    // Focus if already exists
    if let Some(win) = app.get_webview_window("onboarding") {
        win.show().map_err(|e: tauri::Error| e.to_string())?;
        win.set_focus().map_err(|e: tauri::Error| e.to_string())?;
        return Ok(());
    }

    WebviewWindowBuilder::new(&app, "onboarding", WebviewUrl::App("/#/onboarding".into()))
        .title("Welcome to oats")
        .inner_size(450.0, 600.0)
        .resizable(false)
        .center()
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Build the waveform window's route, appending the optional `localAppendId`
/// value, the `forceNew` flag, plus the `auto` and `pillHidden` query flags.
/// Kept pure so the wiring is unit-testable.
fn waveform_url(
    meeting_id: Option<i64>,
    auto: bool,
    pill_hidden: bool,
    local_append_id: Option<&str>,
    force_new: bool,
) -> String {
    let mut url = match meeting_id {
        Some(id) => format!("/#/waveform?meetingId={id}"),
        None => "/#/waveform".to_string(),
    };
    let mut push = |part: &str| {
        url.push_str(if url.contains('?') { "&" } else { "?" });
        url.push_str(part);
    };
    if let Some(id) = local_append_id {
        push(&format!("localAppendId={id}"));
    }
    if force_new {
        push("forceNew=1");
    }
    if auto {
        push("auto=1");
    }
    if pill_hidden {
        push("pillHidden=1");
    }
    url
}

/// How long a queued recording waits for the incumbent pill to stand down
/// before the request is dropped. The recorder answers `recorder://yield` from
/// an idle post-upload state, so this only needs to cover event delivery plus
/// the window close; a pill that is genuinely busy never answers at all.
const YIELD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Bound on waiting for the `"waveform"` label to be free after the incumbent
/// pill's `Destroyed` event, before re-opening in its place.
const YIELD_REOPEN_POLLS: u32 = 20;
const YIELD_REOPEN_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);

/// Payload for the `recording://start-failed` event emitted when a queued
/// recording's yield request expires (#320).
///
/// The incumbent pill refuses to stand down in two states the user must not
/// have confused: `capture_active()` is set while audio is actually being
/// recorded and cleared on stop, before the upload runs — so telling someone
/// mid-recording to "wait for the upload to finish" describes a wait that never
/// ends. The frontend turns `reason` into user-facing copy and names the meeting
/// when it can resolve `meeting_id`; only it has meeting titles.
fn yield_expired_payload(capturing: bool, meeting_id: Option<i64>) -> serde_json::Value {
    serde_json::json!({
        "reason": if capturing { "capturing" } else { "uploading" },
        "meetingId": meeting_id,
    })
}

/// Shared helper to open the waveform recording window. Used by the
/// `start_recording_window` command, the tray (Local backend path), and the
/// auto mic monitor. `auto` adds `auto=1` to the URL and tags the shared
/// `RecordingState` as an auto recording. `force_new` adds `forceNew=1`,
/// telling the recorder to skip the 5-minute auto-append window and always
/// dock to a brand-new recording id.
///
/// Only one recorder pill exists at a time. When one is already up this
/// negotiates a handoff rather than no-opping: see the `recorder://yield`
/// exchange below.
pub(crate) fn open_waveform_window(
    app: &tauri::AppHandle,
    meeting_id: Option<i64>,
    local_append_id: Option<String>,
    force_new: bool,
    auto: bool,
) -> Result<(), String> {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

    if let Some(picker) = app.get_webview_window("meeting-picker") {
        let _ = picker.close();
    }
    if let Some(existing) = app.get_webview_window("waveform") {
        // Backfill the lifecycle claim for a registered window that still owns
        // capture, upload, retry, or its native close transition.
        let state = app.state::<crate::recording_state::RecordingState>();
        let _ = state.try_claim_window();
        let _ = existing.set_focus();
        // The pill outlives capture: it lingers through the upload and stays up
        // indefinitely on a failed one so the user can retry. Such a stale pill
        // must not silently swallow a new recording (#313), so ask it to stand
        // down and re-open once it's actually gone (see the Destroyed handler
        // below). It refuses while still capturing or uploading — then the
        // queued request expires and the focus above is all that happens. Tell
        // the frontend which recording is in the way, and in which state, so the
        // user gets an accurate explanation (#320) instead of a launch spinner
        // that hangs forever.
        let token = state.queue_reopen(meeting_id, local_append_id, force_new, auto);
        let _ = app.emit("recorder://yield", ());
        let app_for_expiry = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(YIELD_TIMEOUT).await;
            let state = app_for_expiry.state::<crate::recording_state::RecordingState>();
            if state.expire_reopen(token) {
                // Read the blocking state only after the request is confirmed
                // dropped, so the reason describes the pill that outlasted the
                // handshake rather than whatever it was doing 3s ago.
                let payload =
                    yield_expired_payload(state.capture_active(), state.active_meeting_id());
                let _ = app_for_expiry.emit("recording://start-failed", payload);
            }
        });
        return Ok(());
    }
    let recording_state = app.state::<crate::recording_state::RecordingState>();
    if !recording_state.try_claim_window() {
        // Another launcher is between its preflight check and native window
        // registration. Treat this request as idempotent instead of creating
        // a second recorder pill.
        if let Some(existing) = app.get_webview_window("waveform") {
            let _ = existing.set_focus();
        }
        return Ok(());
    }
    // Born hidden (painted-empty) when the meetings window already owns the
    // recorder UI, so the pill never flashes over it. The window is still
    // created visible for getUserMedia; only its painting is suppressed.
    let pill_hidden = !crate::recorder_pill::should_show_now(app);
    let url = waveform_url(meeting_id, auto, pill_hidden, local_append_id.as_deref(), force_new);
    let win = match WebviewWindowBuilder::new(app, "waveform", WebviewUrl::App(url.into()))
        .title("")
        // Fixed size: room for the expanded pill plus its CSS shadow. The pill
        // itself is anchored to the bottom and grows upward within this window.
        .inner_size(
            crate::recorder_pill::PILL_W,
            crate::recorder_pill::PILL_H,
        )
        // Born visible even when the library's embedded strip is the real UI:
        // WebKit won't resolve getUserMedia for a hidden window, so the pill
        // must stay on screen until capture starts. The visibility watcher
        // hides it then (set_tray_recording marks capture active).
        // Throttling is disabled so the hidden webview keeps recording and
        // broadcasting recorder://state.
        .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled)
        .decorations(false)
        .always_on_top(true)
        .resizable(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .build()
    {
        Ok(win) => win,
        Err(error) => {
            recording_state.release_window_claim();
            return Err(error.to_string());
        }
    };

    // Capture may stop before upload/close completes. Keep the one-window
    // claim until this native window is actually destroyed.
    let app_for_event = app.clone();
    win.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            let state = app_for_event.state::<crate::recording_state::RecordingState>();
            state.clear();
            state.release_window_claim();
            let _ = app_for_event.emit("recording://state", false);
            // Same path also restores the idle tray menu and icon; otherwise a
            // force-closed recorder leaves the menu bar claiming we are still
            // recording. A queued re-open re-arms the recording menu itself
            // once the new pill is up.
            crate::tray::set_menu(&app_for_event, false, false);
            // A recording was requested while this pill still held the window
            // slot and it has now stood down, so honor that request.
            if let Some(next) = state.take_reopen() {
                let app_for_reopen = app_for_event.clone();
                tauri::async_runtime::spawn(async move {
                    // This event and Tauri's own deregistration of the window
                    // race. Wait for the label to actually be free before
                    // re-opening.
                    let mut freed = false;
                    for _ in 0..YIELD_REOPEN_POLLS {
                        if app_for_reopen.get_webview_window("waveform").is_none() {
                            freed = true;
                            break;
                        }
                        tokio::time::sleep(YIELD_REOPEN_POLL_INTERVAL).await;
                    }
                    // Give up rather than re-open into a still-registered
                    // label: that would rediscover the dying window, emit a
                    // second yield nothing is left to answer, and leave a
                    // queued request that only expires — or worse, gets
                    // honored later by an unrelated close.
                    if !freed {
                        eprintln!(
                            "waveform: window still registered {}ms after Destroyed; dropping the queued re-open",
                            (YIELD_REOPEN_POLLS as u128) * YIELD_REOPEN_POLL_INTERVAL.as_millis()
                        );
                        return;
                    }
                    // Window creation must happen on the main thread.
                    let app_main = app_for_reopen.clone();
                    let _ = app_for_reopen.run_on_main_thread(move || {
                        if let Err(error) = open_waveform_window(
                            &app_main,
                            next.meeting_id,
                            next.local_append_id,
                            next.force_new,
                            next.auto,
                        ) {
                            eprintln!("waveform: re-open after yield failed: {error}");
                        }
                    });
                });
            }
        }
    });

    // The application menu is useful on normal Windows windows, but inheriting
    // it here adds a File/Edit/View/Window strip to an otherwise frameless
    // recorder pill. Remove it on this window only.
    #[cfg(target_os = "windows")]
    if let Err(error) = win.remove_menu() {
        let _ = win.close();
        return Err(format!("failed to remove waveform window menu: {error}"));
    }

    // When the pill is the visible recording UI (the meetings window is hidden,
    // minimized, or closed), dock it to the right edge of the primary screen
    // rather than leaving it at the OS default spot. When the meetings window
    // owns the UI the pill is painted-empty, so its position doesn't matter —
    // the watcher re-docks it later if the meetings window is minimized.
    if !pill_hidden {
        crate::recorder_pill::dock_to_right_edge(&win);
    }

    let source = if auto {
        crate::recording_state::RecordingSource::Auto
    } else {
        crate::recording_state::RecordingSource::Manual
    };
    app.state::<crate::recording_state::RecordingState>()
        .set(source, meeting_id);
    let _ = app.emit("recording://state", true);

    crate::tray::set_menu(app, true, false);

    // Show the pill only while the library window (with its embedded
    // recorder strip) can't be seen — minimized or closed.
    crate::recorder_pill::spawn_watcher(app);

    // Tell every window (the library in particular) which meeting the new
    // recording is attached to, so it can surface that meeting immediately.
    let _ = app.emit(
        "recording://started",
        serde_json::json!({ "meetingId": meeting_id }),
    );
    Ok(())
}

/// Gate recording on a valid session for the Ariso backend. The Local backend
/// needs no auth but is gated on both on-device models being downloaded;
/// otherwise the Settings window is surfaced and the attempt aborts. When the
/// Ariso user is signed out, surface the (pre-created) Settings window and emit
/// `tray://show-sign-in-prompt` so its sign-in banner appears, then report
/// `false` so the caller aborts. Mirrors the tray's session gate so every
/// recording entry point behaves identically.
async fn ensure_recording_allowed(app: &tauri::AppHandle) -> bool {
    if active_backend(app) == "local" {
        // Local needs no auth, but both on-device models must be ready. When
        // they aren't, surface Settings (which auto-starts the downloads) and
        // abort this recording attempt.
        if local_models_ready() {
            return true;
        }
        surface_model_download(app);
        return false;
    }
    if is_session_valid(app).await {
        return true;
    }
    let _ = open_settings_window(app);
    let _ = app.emit("tray://show-sign-in-prompt", ());
    false
}

/// Open the waveform recording window, optionally attaching to an existing
/// meeting id. Closes the meeting-picker window if present and flips the
/// tray menu to the recording state. `force_new` (default `false` when
/// omitted) requests a brand-new local recording that skips the 5-minute
/// auto-append window.
#[tauri::command]
pub async fn start_recording_window(
    app: tauri::AppHandle,
    meeting_id: Option<i64>,
    local_append_id: Option<String>,
    force_new: Option<bool>,
) -> Result<(), String> {
    if !ensure_recording_allowed(&app).await {
        return Err("sign-in required".to_string());
    }
    open_waveform_window(&app, meeting_id, local_append_id, force_new.unwrap_or(false), false)
}

/// Show/focus the meeting-picker window, building it if absent. Shared by the
/// tray (Ariso path) and the `open_meeting_picker` command so both open the
/// picker identically.
pub(crate) fn open_meeting_picker_window(
    app: &tauri::AppHandle,
    default_meeting_id: Option<i64>,
) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    if let Some(picker) = app.get_webview_window("meeting-picker") {
        let _ = picker.show();
        let _ = picker.set_focus();
        return Ok(());
    }
    let route = match default_meeting_id {
        Some(id) => format!("/#/meeting-picker?defaultMeetingId={id}"),
        None => "/#/meeting-picker".to_string(),
    };
    WebviewWindowBuilder::new(app, "meeting-picker", WebviewUrl::App(route.into()))
        .title("Select a meeting")
        .inner_size(400.0, 500.0)
        .resizable(false)
        .center()
        .skip_taskbar(true)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Open (or focus) the meeting-picker window. Invoked by the library's
/// start-recording button for picker-using backends.
#[tauri::command]
pub async fn open_meeting_picker(
    app: tauri::AppHandle,
    default_meeting_id: Option<i64>,
) -> Result<(), String> {
    if !ensure_recording_allowed(&app).await {
        return Err("sign-in required".to_string());
    }
    open_meeting_picker_window(&app, default_meeting_id)
}

/// PUT binary data to a presigned URL (bypasses CORS via native HTTP client)
#[tauri::command]
pub async fn put_presigned(
    url: String,
    data: Vec<u8>,
    content_type: String,
) -> Result<u16, String> {
    let client = http_client();
    let response = client
        .put(&url)
        .header(CONTENT_TYPE, content_type.as_str())
        .body(data)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    Ok(response.status().as_u16())
}

#[derive(Serialize)]
pub struct DesktopConfig {
    #[serde(rename = "pusherKey")]
    pub pusher_key: String,
    #[serde(rename = "pusherCluster")]
    pub pusher_cluster: String,
    #[serde(rename = "webAppBaseUrl")]
    pub web_app_base_url: String,
    /// Empty when this build has no diagnostics endpoint; the frontend treats
    /// that as "diagnostics unavailable" and never initializes Sentry.
    #[serde(rename = "sentryDsn")]
    pub sentry_dsn: String,
}

/// Returns build-baked client config (Pusher key/cluster, web app base URL,
/// Sentry DSN).
#[tauri::command]
pub fn get_desktop_config() -> DesktopConfig {
    DesktopConfig {
        pusher_key: PUSHER_KEY.to_string(),
        pusher_cluster: PUSHER_CLUSTER.to_string(),
        web_app_base_url: web_app_base_url(),
        sentry_dsn: sentry_dsn(),
    }
}

#[tauri::command]
pub fn list_local_recordings() -> Result<Vec<crate::storage::RecordingSummary>, String> {
    let root = crate::vault::meta_root()?;
    let mut summaries = crate::storage::list_recordings(&root)?;
    // Overlay vault note presence (a user may have deleted a vault note; a new
    // recording's note lives only in the vault, not as ari-note.md).
    let vault_notes = crate::vault::scan_vault()?;
    let vault_root = crate::vault::vault_root()?;
    for s in &mut summaries {
        if vault_notes.contains_key(&s.id) {
            s.has_note = true;
        }
        if let Some(af) = &s.audio_file {
            // New recording: audio lives in the vault; reflect real existence
            // (a user may have deleted the attachment in Obsidian).
            s.has_audio = crate::vault::audio_path(&vault_root, af).is_file();
        }
        // Legacy (audio_file None): has_audio already reflects recording.mp3.
    }
    Ok(summaries)
}

/// Lightweight status for a single local recording, used by the detail panel's
/// generation poller. Reads only that recording's `meta.json` and probes its
/// two artifact files, deriving the AI-notes state the list summary omits.
#[tauri::command]
pub fn local_recording_status(
    id: String,
) -> Result<crate::storage::RecordingStatusView, String> {
    crate::storage::validate_recording_id(&id)?;
    let dir = recording_dir(&id)?;
    let meta = crate::storage::read_meta(&dir)?;
    let has_note =
        dir.join("ari-note.md").is_file() || crate::vault::find_note(&id)?.is_some();
    let has_transcript = dir.join("transcript.md").is_file();
    let notes_status = crate::storage::derive_notes_status(has_note, meta.notes_error.as_deref());
    Ok(crate::storage::RecordingStatusView {
        status: meta.status,
        has_transcript,
        has_note,
        notes_status,
    })
}

/// Non-binary arguments of [`buffer_pending_audio`], carried in the
/// `x-oats-meta` header because the audio itself occupies the request body.
#[derive(serde::Deserialize)]
pub struct BufferPendingAudioArgs {
    pub meta: crate::storage::PendingUploadMeta,
}

/// Buffer a stopped Ariso recording's mp3 + metadata on disk before the upload
/// attempt, keyed by its ISO `created_at`. Returns the sanitized id.
///
/// Takes the mp3 as a raw IPC body for the same reason as
/// `local_finalize_recording` — see `raw_ipc` for the measurements.
#[tauri::command]
pub fn buffer_pending_audio(request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let audio = crate::raw_ipc::body_bytes(&request)?;
    let BufferPendingAudioArgs { meta } = crate::raw_ipc::meta(&request)?;
    let root = crate::storage::ariso_root()?;
    // Borrowed straight through to the write — no copy of the mp3.
    crate::storage::write_pending_audio(&root, &meta, audio)
}

/// Remove the buffered mp3 for `created_at` (idempotent). Called after a
/// confirmed upload and on explicit dismiss of a failed one.
#[tauri::command]
pub fn discard_pending_audio(created_at: String) -> Result<(), String> {
    let root = crate::storage::ariso_root()?;
    crate::storage::discard_pending_audio(&root, &created_at)
}

/// List buffered pending uploads (oldest-first) for the Library's resume UI.
#[tauri::command]
pub fn list_pending_uploads() -> Result<Vec<crate::storage::PendingUploadMeta>, String> {
    let root = crate::storage::ariso_root()?;
    crate::storage::list_pending_uploads(&root)
}

/// Concatenate the given pending uploads (chronological key order) into a
/// single mp3, returned as raw bytes for re-upload. Bounded by MAX_AUDIO_BYTES.
#[tauri::command]
pub fn combine_pending_audio(
    created_at_keys: Vec<String>,
) -> Result<tauri::ipc::Response, String> {
    let root = crate::storage::ariso_root()?;
    let bytes = crate::storage::combine_pending_audio(&root, &created_at_keys, MAX_AUDIO_BYTES)?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Reveal a buffered pending upload in the OS file manager (Finder on macOS,
/// Explorer on Windows). If the mp3 is gone, still open `pending-uploads/`
/// itself so the button always lands the user in the right folder.
#[tauri::command]
pub fn reveal_pending_upload(app: tauri::AppHandle, created_at: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    let root = crate::storage::ariso_root()?;
    let audio = crate::storage::pending_audio_path(&root, &created_at)?;
    if audio.is_file() {
        return app
            .opener()
            .reveal_item_in_dir(&audio)
            .map_err(|e| e.to_string());
    }

    let dir = crate::storage::pending_uploads_dir(&root);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create pending-uploads dir: {e}"))?;
    app.opener()
        .open_path(dir.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// The buffer folder as actually resolved: `$HOME/.ariso` on macOS,
/// `%USERPROFILE%\.ariso` on Windows, or whatever `ARISO_ROOT` overrides it to.
/// The upload-failure recovery copy names this instead of guessing a POSIX
/// `~/...` path that is wrong on Windows and under the override.
#[tauri::command]
pub fn pending_uploads_path() -> Result<String, String> {
    let root = crate::storage::ariso_root()?;
    Ok(crate::storage::pending_uploads_dir(&root)
        .to_string_lossy()
        .into_owned())
}

/// Resolve a recording's directory under `<vault>/.oats/recordings/<id>`,
/// guarding against path traversal. Ids are normally sanitized timestamps
/// (e.g. `2026-06-02T14-30-05Z`), so the guard never rejects legitimate ids.
fn recording_dir(id: &str) -> Result<std::path::PathBuf, String> {
    // Reject ids that could escape the recordings dir. `:` is blocked too so a
    // Windows drive-relative form (e.g. `C:foo`) can never slip past the guard.
    if id.is_empty()
        || id.contains('/')
        || id.contains('\\')
        || id.contains(':')
        || id.contains("..")
    {
        return Err(format!("invalid recording id: {id}"));
    }
    let root = crate::vault::meta_root()?;
    Ok(crate::storage::recordings_dir(&root).join(id))
}

/// Map an openable file `kind` to its on-disk filename. Only `note` and
/// `transcript` are valid; anything else is an error.
fn note_or_transcript_filename(kind: &str) -> Result<&'static str, String> {
    match kind {
        "note" => Ok("ari-note.md"),
        "transcript" => Ok("transcript.md"),
        other => Err(format!("invalid recording file kind: {other}")),
    }
}

/// Upper bound on the audio we'll load into memory for playback. The whole file
/// is read into RAM and copied across IPC into a JS Blob, so this guards against
/// OOM on a corrupt or pathologically large file. ~1 GB is far above any real
/// meeting recording (mp3 at this app's bitrate is well under 1 MB/min).
const MAX_AUDIO_BYTES: u64 = 1024 * 1024 * 1024;

/// Resolve and read a recording's audio bytes. New recordings store audio in
/// the vault (`meta.audio_file` names the attachment); legacy recordings
/// (pre-vault, `audio_file: None`) keep it at `<recording_dir>/recording.mp3`.
/// Bounded by `MAX_AUDIO_BYTES` to avoid loading a pathologically large file.
fn read_recording_audio_bytes(id: &str) -> Result<Vec<u8>, String> {
    crate::storage::validate_recording_id(id)?;
    let dir = recording_dir(id)?;
    let meta = crate::storage::read_meta(&dir)?;
    let path = match meta.audio_file {
        Some(audio_file) => crate::vault::audio_path(&crate::vault::vault_root()?, &audio_file),
        None => dir.join("recording.mp3"),
    };
    let size = std::fs::metadata(&path)
        .map_err(|e| format!("read recording audio: {e}"))?
        .len();
    if size > MAX_AUDIO_BYTES {
        return Err(format!("recording audio too large to play: {size} bytes"));
    }
    std::fs::read(&path).map_err(|e| format!("read recording audio: {e}"))
}

/// Read the raw bytes of a recording's audio, returned as a raw binary IPC
/// response so the frontend can build a Blob URL for an `<audio>` element
/// (avoids JSON-array bloat from a `Vec<u8>` return).
#[tauri::command]
pub fn read_recording_audio(id: String) -> Result<tauri::ipc::Response, String> {
    Ok(tauri::ipc::Response::new(read_recording_audio_bytes(&id)?))
}

/// Meeting ids are numeric on the Ariso backend; rejecting anything else also
/// keeps the id from smuggling path segments into the URL below.
fn validate_meeting_id(id: &str) -> Result<(), String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("invalid meeting id: {id}"));
    }
    Ok(())
}

/// Clip transcript ids are `randomUUID()`s (or the sentinel `"legacy"`, which
/// callers route to the no-arg endpoint instead). Allow only ascii-alphanumerics
/// and dashes so a caller can't smuggle path segments or a query string into the
/// URL below.
fn validate_transcript_id(id: &str) -> Result<(), String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(format!("invalid transcript id: {id}"));
    }
    Ok(())
}

/// Build the meeting-audio URL, validating ids first. `None` transcript id →
/// the whole-meeting/legacy endpoint; `Some(id)` → the per-clip endpoint.
fn meeting_audio_url(
    base: &str,
    meeting_id: &str,
    transcript_id: Option<&str>,
) -> Result<String, String> {
    validate_meeting_id(meeting_id)?;
    match transcript_id {
        Some(tid) => {
            validate_transcript_id(tid)?;
            Ok(format!("{base}/meeting-notes/{meeting_id}/audio/{tid}"))
        }
        None => Ok(format!("{base}/meeting-notes/{meeting_id}/audio")),
    }
}

/// Fetch a meeting's recorded audio from the Ariso API as raw bytes (the
/// endpoint streams the file directly). Non-200 responses become an error
/// whose message is prefixed with the HTTP status so the frontend can map
/// 404 to a "no audio" state.
#[tauri::command]
pub async fn fetch_meeting_audio(
    app: tauri::AppHandle,
    meeting_id: String,
    transcript_id: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    let url = meeting_audio_url(&api_base_url(), &meeting_id, transcript_id.as_deref())?;
    let token = get_session_token(&app).unwrap_or_default();
    let client = http_client();

    // Bound the request so a stalled upstream/TCP connection can't hang the
    // command indefinitely; reqwest's builder has no default timeout.
    let mut response = client
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status().as_u16();
    if status != 200 {
        return Err(format!("{status}: audio fetch failed"));
    }
    if let Some(len) = response.content_length() {
        if len > MAX_AUDIO_BYTES {
            return Err(format!("meeting audio too large to play: {len} bytes"));
        }
    }
    // Stream the body and enforce MAX_AUDIO_BYTES as we go — buffering the
    // whole response first can blow memory if content_length is absent or wrong.
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        let next_len = bytes.len() as u64 + chunk.len() as u64;
        if next_len > MAX_AUDIO_BYTES {
            return Err(format!("meeting audio too large to play: {next_len} bytes"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(tauri::ipc::Response::new(bytes))
}

/// Upper bound on a diarized speaker's voice sample. These are a few seconds of
/// one voice, cut server-side — orders of magnitude below a whole recording, so
/// this cap is deliberately much tighter than `MAX_AUDIO_BYTES`.
const MAX_VOICE_SAMPLE_BYTES: u64 = 16 * 1024 * 1024;

/// Build the voice-sample URL, validating the meeting id first. `speaker_index`
/// is a `u32` rather than a string precisely so it cannot carry a path segment
/// or a query string into the URL.
fn speaker_audio_url(
    base: &str,
    meeting_id: &str,
    speaker_index: u32,
) -> Result<String, String> {
    validate_meeting_id(meeting_id)?;
    Ok(format!(
        "{base}/meeting-notes/{meeting_id}/speakers/{speaker_index}/audio"
    ))
}

/// Fetch one diarized speaker's voice sample from the Ariso API as raw bytes.
/// Samples are stored per meeting and addressed by diarization index, so they
/// are playable before the speaker has been assigned to anyone.
///
/// Non-200 responses become an error prefixed with the HTTP status, so the
/// frontend can treat 404 ("no sample stored for this speaker") as an ordinary
/// empty state rather than a failure worth reporting.
#[tauri::command]
pub async fn fetch_speaker_audio(
    app: tauri::AppHandle,
    meeting_id: String,
    speaker_index: u32,
) -> Result<tauri::ipc::Response, String> {
    let url = speaker_audio_url(&api_base_url(), &meeting_id, speaker_index)?;
    let token = get_session_token(&app).unwrap_or_default();
    let client = http_client();

    let mut response = client
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status().as_u16();
    if status != 200 {
        return Err(format!("{status}: voice sample fetch failed"));
    }
    if let Some(len) = response.content_length() {
        if len > MAX_VOICE_SAMPLE_BYTES {
            return Err(format!("voice sample too large to play: {len} bytes"));
        }
    }
    // Enforce the cap while streaming too: `content_length` is absent for a
    // chunked response and is not something we want to trust regardless.
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        let next_len = bytes.len() as u64 + chunk.len() as u64;
        if next_len > MAX_VOICE_SAMPLE_BYTES {
            return Err(format!("voice sample too large to play: {next_len} bytes"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(tauri::ipc::Response::new(bytes))
}

/// Upper bound on a note/transcript markdown file we'll read into memory for
/// in-app rendering. These are plain text; 16 MB is far above any real note or
/// transcript and just guards against a pathological/corrupt file.
const MAX_TEXT_BYTES: u64 = 16 * 1024 * 1024;

/// Read a recording's `ari-note.md` or `transcript.md` as UTF-8 so the
/// frontend can render it inline. Returns `Ok(None)` when the file doesn't
/// exist yet (a normal "not generated" state), distinct from a read error.
/// `kind` must be `"note"` or `"transcript"`.
#[tauri::command]
pub fn read_recording_file(id: String, kind: String) -> Result<Option<String>, String> {
    crate::storage::validate_recording_id(&id)?;
    if kind == "note" {
        // Vault note (new recordings) first, then legacy ~/.ariso/ari-note.md.
        if let Some(body) = crate::vault::read_note(&id)? {
            return Ok(Some(body));
        }
        let legacy = recording_dir(&id)?.join("ari-note.md");
        return match std::fs::read_to_string(&legacy) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("read recording file: {e}")),
        };
    }
    let filename = note_or_transcript_filename(&kind)?;
    let path = recording_dir(&id)?.join(filename);
    // Only a genuine "not found" means the file hasn't been generated yet;
    // surface permission/IO errors instead of masking them as `Ok(None)`.
    let size = match std::fs::metadata(&path) {
        Ok(m) => m.len(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("read recording file: {e}")),
    };
    if size > MAX_TEXT_BYTES {
        return Err(format!("recording file too large to read: {size} bytes"));
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("read recording file: {e}"))?;
    Ok(Some(text))
}

/// Open a recording's `ari-note.md` or `transcript.md` in the OS default app.
/// `kind` must be `"note"` or `"transcript"`.
#[tauri::command]
pub fn open_recording_file(app: tauri::AppHandle, id: String, kind: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    crate::storage::validate_recording_id(&id)?;
    if kind == "note" {
        let path = match crate::vault::find_note(&id)? {
            Some(p) => p,
            None => recording_dir(&id)?.join("ari-note.md"),
        };
        if !path.exists() {
            return Err(format!("recording file not found: {}", path.display()));
        }
        return app
            .opener()
            .open_path(path.to_string_lossy().into_owned(), None::<&str>)
            .map_err(|e| e.to_string());
    }

    let filename = note_or_transcript_filename(&kind)?;
    let path = recording_dir(&id)?.join(filename);
    if !path.exists() {
        return Err(format!("recording file not found: {}", path.display()));
    }
    app.opener()
        .open_path(path.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Copy a recording's `ari-note.md` or `transcript.md` to a user-picked path.
/// The save-a-copy sibling of `open_recording_file`: same id/kind validation,
/// but the bytes land wherever the caller's save dialog pointed.
///
/// `kind` must be `"note"` or `"transcript"`; `dest` must be absolute. The
/// caller chooses *where* the bytes go but never *what* they are — the source
/// is always a file inside the vault, named by a validated recording id.
#[tauri::command]
pub fn copy_recording_file(id: String, kind: String, dest: String) -> Result<(), String> {
    crate::storage::validate_recording_id(&id)?;
    if !std::path::Path::new(&dest).is_absolute() {
        return Err("Destination path must be absolute.".to_string());
    }
    let src = if kind == "note" {
        // Mirror `open_recording_file`'s resolution: vault note (new
        // recordings) first, then the legacy `ari-note.md` fallback.
        match crate::vault::find_note(&id)? {
            Some(p) => p,
            None => recording_dir(&id)?.join("ari-note.md"),
        }
    } else {
        // Transcripts have no vault accessor; `recording_dir(id)/transcript.md`
        // is the single source of truth.
        recording_dir(&id)?.join(note_or_transcript_filename(&kind)?)
    };
    if !src.exists() {
        return Err(format!("recording file not found: {}", src.display()));
    }

    // `fs::copy(p, p)` opens the destination with truncate(true) on the same inode, so it
    // zeroes the file and still returns Ok(0) — saving onto the vault's own transcript.md
    // would destroy the only copy and report success. `canonicalize` resolves symlinks and
    // hardlinks; it fails harmlessly on `dest` in the normal case where the file is new.
    if let (Ok(src_real), Ok(dest_real)) =
        (src.canonicalize(), std::path::Path::new(&dest).canonicalize())
    {
        if src_real == dest_real {
            return Err(
                "Pick a destination outside the vault — that path is the transcript itself."
                    .to_string(),
            );
        }
    }

    std::fs::copy(&src, &dest)
        .map(|_| ())
        .map_err(|e| format!("copy export file: {e}"))
}

/// Convert a web rect's top-left Y to an AppKit view's bottom-left Y.
/// `view_height` is the content view's height in points; `y`/`height` are the
/// button rect in CSS points (CSS px == AppKit points, so no DPR scaling).
#[cfg(target_os = "macos")]
fn flip_y(view_height: f64, y: f64, height: f64) -> f64 {
    view_height - (y + height)
}

/// Maximum local recording title length, in characters. Mirrors the frontend
/// limit (the UI validates first; this is defense in depth).
const MAX_TITLE_CHARS: usize = 40;

/// Rename a local recording by updating `title` in its `meta.json`. The folder
/// id stays immutable; serde_json escapes quotes/special characters natively.
#[tauri::command]
pub fn rename_local_recording(id: String, title: String) -> Result<(), String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("title must not be empty".to_string());
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(format!("title must be {MAX_TITLE_CHARS} characters or fewer"));
    }
    let dir = recording_dir(&id)?;
    let mut meta = crate::storage::read_meta(&dir)?;
    meta.title = title.to_string();
    // A user rename makes the title intentional — never auto-retitle again.
    meta.title_is_default = false;
    // Propagate the rename into the vault: rename the note file + audio
    // attachment to match the new title (preserving the note body), and store
    // the new attachment name. Legacy recordings (no `audio_file`) have their
    // note/audio under `~/.ariso` with fixed names, so nothing to propagate.
    if let Some(old_audio_file) = meta.audio_file.clone() {
        let new_audio_file =
            crate::vault::rename_recording_artifacts(&id, &meta.created_at, &old_audio_file, title)?;
        meta.audio_file = Some(new_audio_file);
    }
    crate::storage::write_meta(&dir, &meta)
}

/// Read the user-authored local note artifact used by the Library editor.
/// Missing notes return an empty string so a fresh recording can autosave into
/// `user-note.md` without affecting generated Overview content.
#[tauri::command]
pub fn read_recording_note(id: String) -> Result<String, String> {
    let path = recording_dir(&id)?.join("user-note.md");
    let size = match std::fs::metadata(&path) {
        Ok(m) => m.len(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(e) => return Err(format!("read recording note: {e}")),
    };
    if size > MAX_TEXT_BYTES {
        return Err(format!("recording note too large to read: {size} bytes"));
    }
    match std::fs::read_to_string(&path) {
        Ok(contents) => Ok(contents),
        Err(e) => Err(format!("read recording note: {e}")),
    }
}

/// Persist user-authored in-meeting notes to `user-note.md` beside the
/// recording. Generated meeting notes use `ari-note.md`, keeping Overview
/// visibility independent from My note autosaves.
#[tauri::command]
pub fn write_recording_note(id: String, markdown: String) -> Result<(), String> {
    let size = markdown.as_bytes().len() as u64;
    if size > MAX_TEXT_BYTES {
        return Err(format!("recording note too large to write: {size} bytes"));
    }
    let dir = recording_dir(&id)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("create recording note dir: {e}"))?;
    crate::storage::write_atomic(&dir.join("user-note.md"), markdown.as_bytes())
}

/// Read the user-authored My-note title sidecar (`user-note-title.txt`). Kept in
/// its own artifact beside `user-note.md` so the editable title round-trips
/// without touching the hand-written `meta` format. Missing files return an
/// empty string, matching `read_recording_note` for fresh recordings.
#[tauri::command]
pub fn read_recording_note_title(id: String) -> Result<String, String> {
    let path = recording_dir(&id)?.join("user-note-title.txt");
    match std::fs::read_to_string(&path) {
        Ok(contents) => Ok(contents),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("read recording note title: {e}")),
    }
}

/// Persist the user-authored My-note title to `user-note-title.txt` beside the
/// recording, mirroring `write_recording_note` so title and body share the same
/// autosave path.
#[tauri::command]
pub fn write_recording_note_title(id: String, title: String) -> Result<(), String> {
    let dir = recording_dir(&id)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("create recording note dir: {e}"))?;
    crate::storage::write_atomic(&dir.join("user-note-title.txt"), title.as_bytes())
}

/// Return the meeting id the active recording is attached to, if any. The
/// library window queries this on mount so it can re-select the attached
/// meeting after being closed/reopened mid-recording — the `recording://started`
/// event is one-shot and the new window would otherwise miss it.
#[tauri::command]
pub async fn get_active_recording_meeting_id(app: tauri::AppHandle) -> Option<i64> {
    app.state::<crate::recording_state::RecordingState>()
        .active_meeting_id()
}

#[tauri::command]
pub async fn create_library_window(app: tauri::AppHandle) -> Result<(), String> {
    open_library_window(&app)
}

/// Make the Meetings window visible and put it at the front of the user's
/// current workspace. Windows can reject foreground activation while a native
/// tray menu is still dismissing, so briefly enter the topmost band and retry
/// focus after the menu has had time to close. The window is then returned to
/// normal z-order; this is a raise operation, not permanent always-on-top.
fn present_library_window(win: &tauri::WebviewWindow) -> Result<(), String> {
    win.unminimize().map_err(|e| e.to_string())?;
    win.show().map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    {
        let mut presentation_generation = LIBRARY_PRESENTATION_GENERATION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        win.set_always_on_top(true).map_err(|e| e.to_string())?;
        if let Err(error) = win.set_focus() {
            eprintln!("Initial Meetings window focus was deferred: {error}");
        }

        *presentation_generation = presentation_generation.wrapping_add(1);
        let generation = *presentation_generation;
        drop(presentation_generation);

        let win = win.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let presentation_generation = LIBRARY_PRESENTATION_GENERATION
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if *presentation_generation != generation {
                // Superseded by a newer presentation call; let its own
                // deferred task own the cleanup instead.
                return;
            }
            if let Err(error) = win.set_focus() {
                eprintln!("Failed to focus Meetings window after tray dismissal: {error}");
            }
            if let Err(error) = win.set_always_on_top(false) {
                eprintln!("Failed to restore normal Meetings window z-order: {error}, retrying");
                if let Err(retry_error) = win.set_always_on_top(false) {
                    eprintln!(
                        "Failed to restore normal Meetings window z-order after retry: {retry_error}"
                    );
                }
            }
        });
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    win.set_focus().map_err(|e| e.to_string())
}

/// Open (or focus) the meetings library window. Shared by the
/// `create_library_window` command and the macOS dock-icon Reopen handler.
pub(crate) fn open_library_window(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
    let _creation = LIBRARY_WINDOW_CREATION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // The library window has no hide-on-close handler, so it is destroyed on
    // close and recreated (with fresh data) on the next open. This branch only
    // fires if it is opened again while still visible — just focus it.
    if let Some(win) = app.get_webview_window("library") {
        return present_library_window(&win);
    }
    // Overlay title bar (with the native title hidden) lets the web content
    // extend under the traffic lights, so the in-app panel toggle can sit on
    // the same row, just to the right of them.
    #[cfg(target_os = "macos")]
    let builder =
        WebviewWindowBuilder::new(app, "library", WebviewUrl::App("/#/library".into()))
            .title("Meetings")
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true);
    // Tauri's overlay title-bar style is macOS-only. Its supported Windows
    // custom-titlebar path is an undecorated window plus webview controls;
    // shadow(true) restores the Windows 11 border, rounded corners and shadow.
    #[cfg(target_os = "windows")]
    let builder =
        WebviewWindowBuilder::new(app, "library", WebviewUrl::App("/#/library".into()))
            .title("Meetings")
            .decorations(false)
            .shadow(true);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let builder =
        WebviewWindowBuilder::new(app, "library", WebviewUrl::App("/#/library".into()))
            .title("Meetings");
    let win = builder
        .inner_size(900.0, 600.0)
        .resizable(true)
        .center()
        .skip_taskbar(true)
        .build()
        .map_err(|e| e.to_string())?;
    present_library_window(&win)
}

#[derive(serde::Deserialize)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub struct ShareAnchor {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Open the native macOS share sheet over `text`, anchored to the button rect
/// (`anchor`, in CSS points relative to the window's content view).
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn share_text_native(
    window: tauri::WebviewWindow,
    text: String,
    anchor: ShareAnchor,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("nothing to share".to_string());
    }
    let window_for_main = window.clone();

    window
        .run_on_main_thread(move || {
            use objc2::rc::Retained;
            use objc2::runtime::AnyObject;
            use objc2::AnyThread;
            use objc2_app_kit::{NSSharingServicePicker, NSWindow};
            use objc2_foundation::{NSArray, NSPoint, NSRect, NSRectEdge, NSSize, NSString};

            // SAFETY: ns_window() is fetched on the main thread immediately
            // before use, so the NSWindow* is valid for this closure's lifetime.
            // AppKit requires this to run on the main thread.
            unsafe {
                let ns_window_ptr = match window_for_main.ns_window() {
                    Ok(ptr) => ptr as *const NSWindow,
                    Err(_) => return,
                };
                let ns_window = &*ns_window_ptr;
                let Some(content_view) = ns_window.contentView() else {
                    return;
                };
                let view_height = content_view.bounds().size.height;

                let ns_text = NSString::from_str(&text);
                let item: Retained<AnyObject> = Retained::into_super(ns_text).into();
                let items = NSArray::from_retained_slice(&[item]);
                let picker = NSSharingServicePicker::initWithItems(
                    NSSharingServicePicker::alloc(),
                    &items,
                );

                let appkit_y = flip_y(view_height, anchor.y, anchor.height);
                let rect = NSRect::new(
                    NSPoint::new(anchor.x, appkit_y),
                    NSSize::new(anchor.width.max(1.0), anchor.height.max(1.0)),
                );
                picker.showRelativeToRect_ofView_preferredEdge(
                    rect,
                    &content_view,
                    NSRectEdge::MinY,
                );
            }
        })
        .map_err(|e| format!("run_on_main_thread: {e}"))
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
/// Keeps the IPC command registered on every desktop target while making the
/// unsupported boundary explicit. Capability-aware UI should hide this path;
/// the error remains defense in depth for stale or direct callers.
pub fn share_text_native(_text: String, _anchor: ShareAnchor) -> Result<(), String> {
    Err("native share is only supported on macOS".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // A recording requested while the pill still holds the window slot is
    // dropped after the yield timeout. What the user is told about it must match
    // the state the pill is actually in (#320).
    #[test]
    fn yield_expiry_distinguishes_a_recording_pill_from_an_uploading_one() {
        assert_eq!(yield_expired_payload(true, Some(42))["reason"], "capturing");
        assert_eq!(yield_expired_payload(false, Some(42))["reason"], "uploading");
    }

    #[test]
    fn yield_expiry_names_the_meeting_holding_the_recorder() {
        assert_eq!(yield_expired_payload(false, Some(42))["meetingId"], 42);
    }

    #[test]
    fn yield_expiry_reports_an_ad_hoc_recording_with_no_meeting() {
        // Local ad-hoc recordings carry no meeting id; the frontend falls back
        // to unnamed copy rather than inventing one.
        assert!(yield_expired_payload(true, None)["meetingId"].is_null());
    }

    #[test]
    fn desktop_auth_redirect_encodes_port_and_nonce() {
        assert_eq!(
            desktop_auth_redirect(51234, "abc123"),
            "/desktop-auth?callback_port=51234&nonce=abc123"
        );
    }

    #[test]
    fn random_nonce_is_32_hex_chars_and_unique() {
        let a = random_nonce().unwrap();
        let b = random_nonce().unwrap();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn validate_browser_auth_url_allows_https_and_local_http_only() {
        let ok = |s: &str| validate_browser_auth_url(&Url::parse(s).unwrap());
        assert!(ok("https://accounts.google.com/o/oauth2/v2/auth?x=1").is_ok());
        assert!(ok("http://localhost:4000/oauth").is_ok());
        assert!(ok("http://127.0.0.1:4000/oauth").is_ok());
        assert!(ok("http://evil.example.com/oauth").is_err());
        assert!(ok("file:///etc/passwd").is_err());
    }

    #[test]
    fn sign_in_attempt_cancel_during_prepare_is_not_dropped() {
        let id = begin_sign_in_attempt(BrowserFlow::SignIn);
        assert!(sign_in_attempt_active(id));

        // Cancel arrives before the loopback listener task exists (still
        // awaiting prepare-state) — it must still find something to cancel
        // instead of silently no-oping.
        assert!(abort_pending_sign_in().is_some());
        assert!(!sign_in_attempt_active(id));

        // A cancel with nothing pending is a no-op, not an error.
        assert!(abort_pending_sign_in().is_none());
    }

    #[test]
    fn cancel_reports_the_flow_that_was_pending() {
        // Cancel must resolve the promise that is actually waiting: the two
        // flows listen on different events, so emitting an oauth-result over a
        // pending calendar connect would hang the frontend until the timeout.
        begin_sign_in_attempt(BrowserFlow::SignIn);
        assert!(abort_pending_sign_in() == Some(BrowserFlow::SignIn));

        begin_sign_in_attempt(BrowserFlow::CalendarConnect);
        assert!(abort_pending_sign_in() == Some(BrowserFlow::CalendarConnect));

        assert_eq!(BrowserFlow::SignIn.result_event(), "oauth-result");
        assert_eq!(
            BrowserFlow::CalendarConnect.result_event(),
            "calendar-connect-result"
        );
    }

    #[test]
    fn retire_sign_in_attempt_only_clears_the_matching_slot() {
        let id = begin_sign_in_attempt(BrowserFlow::SignIn);

        // A superseded attempt's retire is a no-op — its slot is already gone
        // (or belongs to a newer attempt), so it must not clear the winner's
        // state or report success.
        let superseded_id = begin_sign_in_attempt(BrowserFlow::SignIn);
        assert!(!retire_sign_in_attempt(id));
        assert!(sign_in_attempt_active(superseded_id));

        // Retiring the current attempt clears it and reports true exactly
        // once; a second retire (e.g. a late cancel racing the emit) is a
        // no-op instead of clearing a newer attempt that took the slot.
        assert!(retire_sign_in_attempt(superseded_id));
        assert!(!sign_in_attempt_active(superseded_id));
        assert!(!retire_sign_in_attempt(superseded_id));
    }

    #[test]
    fn attempt_guard_retires_the_slot_when_the_prologue_fails() {
        // Every fallible step between begin and the listener task's spawn —
        // bind, prepare-state, URL validation, open_url — used to leave the
        // attempt parked in the slot, so a later cancel emitted a result for
        // a flow that had already failed.
        let id = {
            let attempt = SignInAttemptGuard::begin(BrowserFlow::CalendarConnect);
            let id = attempt.id();
            assert!(sign_in_attempt_active(id));
            id // guard drops here, as it would on an early `?` return
        };
        assert!(!sign_in_attempt_active(id));
        assert!(abort_pending_sign_in().is_none());
    }

    #[test]
    fn released_attempt_survives_the_guard_going_out_of_scope() {
        // Once the listener task owns the attempt, the guard must keep its
        // hands off: retiring here would strand the frontend's waiter.
        let id = {
            let attempt = SignInAttemptGuard::begin(BrowserFlow::SignIn);
            attempt.release()
        };
        assert!(sign_in_attempt_active(id));
        assert!(abort_pending_sign_in() == Some(BrowserFlow::SignIn));
    }

    #[test]
    fn attempt_guard_drop_never_retires_a_newer_attempt() {
        // A superseded prologue failing late must not clear the winner's slot.
        let stale = SignInAttemptGuard::begin(BrowserFlow::SignIn);
        let winner = begin_sign_in_attempt(BrowserFlow::CalendarConnect);
        drop(stale);
        assert!(sign_in_attempt_active(winner));
        assert!(retire_sign_in_attempt(winner));
    }

    #[tokio::test]
    async fn sign_in_attempt_out_of_order_completion_supersedes_cleanly() {
        let first_id = begin_sign_in_attempt(BrowserFlow::SignIn);
        // A second attempt starts (e.g. the user retried) before the first's
        // prepare-state call returned, superseding it before it ever got a
        // listener handle.
        let second_id = begin_sign_in_attempt(BrowserFlow::SignIn);
        assert!(!sign_in_attempt_active(first_id));
        assert!(sign_in_attempt_active(second_id));

        // The first attempt's prepare-state call eventually resolves and
        // spawns its listener task anyway — attaching it must abort it
        // immediately rather than letting a superseded flow run unattended.
        let stale_handle =
            tauri::async_runtime::spawn(async {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            });
        attach_sign_in_handle(first_id, stale_handle);
        assert!(sign_in_attempt_active(second_id));

        // The second attempt's own listener task attaches normally.
        let handle = tauri::async_runtime::spawn(async {});
        attach_sign_in_handle(second_id, handle);
        assert!(sign_in_attempt_active(second_id));

        assert!(abort_pending_sign_in().is_some());
        assert!(!sign_in_attempt_active(second_id));
    }

    #[test]
    fn parse_loopback_callback_extracts_token_and_nonce() {
        assert_eq!(
            parse_loopback_callback("GET /callback?token=tok123&nonce=n456 HTTP/1.1"),
            Some((LoopbackDelivery::Token("tok123".into()), "n456".into()))
        );
        // Percent-encoded values are decoded.
        assert_eq!(
            parse_loopback_callback("GET /callback?token=a%2Bb&nonce=n HTTP/1.1"),
            Some((LoopbackDelivery::Token("a+b".into()), "n".into()))
        );
    }

    #[test]
    fn parse_loopback_callback_accepts_token_less_status_delivery() {
        // The Workspace connect hop delivers no token, only a status marker.
        assert_eq!(
            parse_loopback_callback("GET /callback?status=connected&nonce=n456 HTTP/1.1"),
            Some((LoopbackDelivery::Status("connected".into()), "n456".into()))
        );
        // Granular consent lets the user untick Calendar and still complete.
        assert_eq!(
            parse_loopback_callback("GET /callback?status=no_calendar_scope&nonce=n HTTP/1.1"),
            Some((
                LoopbackDelivery::Status("no_calendar_scope".into()),
                "n".into()
            ))
        );
    }

    #[test]
    fn parse_loopback_callback_prefers_token_over_status() {
        // A status parameter must never downgrade a sign-in delivery into the
        // token-less path, or a forged status could strand the exchange.
        assert_eq!(
            parse_loopback_callback("GET /callback?token=tok&status=connected&nonce=n HTTP/1.1"),
            Some((LoopbackDelivery::Token("tok".into()), "n".into()))
        );
    }

    #[test]
    fn parse_loopback_callback_rejects_malformed_requests() {
        // Wrong method, wrong path, or missing/empty params never match.
        assert_eq!(parse_loopback_callback("POST /callback?token=t&nonce=n HTTP/1.1"), None);
        assert_eq!(parse_loopback_callback("GET /favicon.ico HTTP/1.1"), None);
        assert_eq!(parse_loopback_callback("GET /callback?token=t HTTP/1.1"), None);
        assert_eq!(parse_loopback_callback("GET /callback?nonce=n HTTP/1.1"), None);
        assert_eq!(parse_loopback_callback("GET /callback?token=&nonce=n HTTP/1.1"), None);
        // A status delivery still requires the nonce, and an empty status is
        // no more acceptable than an empty token.
        assert_eq!(parse_loopback_callback("GET /callback?status=connected HTTP/1.1"), None);
        assert_eq!(parse_loopback_callback("GET /callback?status=&nonce=n HTTP/1.1"), None);
        assert_eq!(parse_loopback_callback(""), None);
    }

    #[tokio::test]
    async fn accept_loopback_callback_ignores_forged_hits_and_returns_token() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let accept = tokio::spawn(async move {
            accept_loopback_callback(&listener, "goodnonce", BrowserFlow::SignIn).await
        });

        let send = |req: String| async move {
            let mut conn = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            conn.write_all(req.as_bytes()).await.unwrap();
            let mut resp = String::new();
            conn.read_to_string(&mut resp).await.unwrap();
            resp
        };

        // A forged callback (wrong nonce) gets a 404 and must not consume the
        // listener; the real callback afterwards still succeeds.
        let forged = send("GET /callback?token=stolen&nonce=wrong HTTP/1.1\r\n\r\n".into()).await;
        assert!(forged.starts_with("HTTP/1.1 404"));

        let real = send("GET /callback?token=tok123&nonce=goodnonce HTTP/1.1\r\n\r\n".into()).await;
        assert!(real.starts_with("HTTP/1.1 200"));
        // The success page never echoes the token.
        assert!(!real.contains("tok123"));
        // Sign-in is the one flow whose page may claim a sign-in happened.
        assert!(real.contains("re signed in"));

        assert_eq!(
            accept.await.unwrap().unwrap(),
            LoopbackDelivery::Token("tok123".into())
        );
    }

    #[tokio::test]
    async fn accept_loopback_callback_returns_token_less_status_delivery() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let accept = tokio::spawn(async move {
            accept_loopback_callback(&listener, "goodnonce", BrowserFlow::CalendarConnect).await
        });

        let mut conn = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        conn.write_all(b"GET /callback?status=connected&nonce=goodnonce HTTP/1.1\r\n\r\n")
            .await
            .unwrap();
        let mut resp = String::new();
        conn.read_to_string(&mut resp).await.unwrap();
        assert!(resp.starts_with("HTTP/1.1 200"));
        // The connect hop runs while the user is already signed in, so its page
        // must not report a sign-in, and it must stay neutral about the grant —
        // the browser lands here even when Calendar was left unticked.
        assert!(!resp.contains("signed in"));
        assert!(!resp.contains("Calendar"));
        assert!(resp.contains("You can close this tab"));

        assert_eq!(
            accept.await.unwrap().unwrap(),
            LoopbackDelivery::Status("connected".into())
        );
    }

    #[test]
    fn get_vault_dir_returns_resolved_path() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        crate::vault::clear_vault_override();
        assert_eq!(
            get_vault_dir().unwrap(),
            tmp.path().join("vault").to_string_lossy().into_owned()
        );
        crate::vault::set_vault_override(tmp.path().join("custom"));
        assert_eq!(
            get_vault_dir().unwrap(),
            tmp.path().join("custom").to_string_lossy().into_owned()
        );
        crate::vault::clear_vault_override();
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[test]
    fn set_vault_dir_rejects_relative_path() {
        // Relative paths are rejected before any app/store access, so no AppHandle
        // is needed to exercise this branch. Extract the validation into a helper.
        assert!(super::validate_vault_path("relative/dir").is_err());
        let absolute = tempfile::tempdir().unwrap();
        assert!(
            super::validate_vault_path(absolute.path().to_str().unwrap()).is_ok()
        );
    }

    #[test]
    fn waveform_url_appends_flags_with_correct_separators() {
        assert_eq!(waveform_url(None, false, false, None, false), "/#/waveform");
        assert_eq!(waveform_url(Some(42), false, false, None, false), "/#/waveform?meetingId=42");
        assert_eq!(waveform_url(None, true, false, None, false), "/#/waveform?auto=1");
        assert_eq!(waveform_url(None, true, true, None, false), "/#/waveform?auto=1&pillHidden=1");
        assert_eq!(waveform_url(None, false, true, None, false), "/#/waveform?pillHidden=1");
        assert_eq!(
            waveform_url(Some(7), true, true, None, false),
            "/#/waveform?meetingId=7&auto=1&pillHidden=1"
        );
        // Local continue: the append target id rides on the URL like the flags.
        assert_eq!(
            waveform_url(None, false, false, Some("2026-06-02T10-00-00Z"), false),
            "/#/waveform?localAppendId=2026-06-02T10-00-00Z"
        );
        assert_eq!(
            waveform_url(None, true, false, Some("abc"), false),
            "/#/waveform?localAppendId=abc&auto=1"
        );
    }

    #[test]
    fn waveform_url_appends_force_new_flag() {
        // force_new rides on the URL the same way localAppendId does.
        assert_eq!(waveform_url(None, false, false, None, true), "/#/waveform?forceNew=1");
        assert_eq!(
            waveform_url(None, true, true, None, true),
            "/#/waveform?forceNew=1&auto=1&pillHidden=1"
        );
        // Both localAppendId and forceNew can theoretically be present (an
        // explicit continue still wins in finalize_core_with_target); the URL
        // just carries whatever flags it's given.
        assert_eq!(
            waveform_url(Some(7), false, false, Some("2026-06-02T10-00-00Z"), true),
            "/#/waveform?meetingId=7&localAppendId=2026-06-02T10-00-00Z&forceNew=1"
        );
    }

    #[test]
    fn note_or_transcript_filename_maps_known_kinds() {
        assert_eq!(note_or_transcript_filename("note").unwrap(), "ari-note.md");
        assert_eq!(
            note_or_transcript_filename("transcript").unwrap(),
            "transcript.md"
        );
    }

    #[test]
    fn note_or_transcript_filename_rejects_unknown_kind() {
        assert!(note_or_transcript_filename("").is_err());
        assert!(note_or_transcript_filename("audio").is_err());
        assert!(note_or_transcript_filename("note.md").is_err());
    }

    #[test]
    fn recording_dir_rejects_traversal_ids() {
        // These guards are pure (no env read), so no ARISO_ROOT needed.
        assert!(recording_dir("").is_err());
        assert!(recording_dir("..").is_err());
        assert!(recording_dir("../foo").is_err());
        assert!(recording_dir("a/b").is_err());
        assert!(recording_dir("a\\b").is_err());
        assert!(recording_dir("C:foo").is_err());
        assert!(recording_dir("foo/../bar").is_err());
    }

    #[test]
    fn recording_dir_accepts_normal_id() {
        let tmp = tempfile::tempdir().unwrap();
        // `recording_dir` reads ARISO_ROOT; set it for this test. The other
        // `recording_dir` tests only exercise the pre-env guard branch, so
        // they don't depend on this value.
        // SAFETY: env mutation requires `--test-threads=1` so no concurrent
        // env access races with these calls (same convention as transcribe).
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let id = "2026-06-02T14-30-05Z";
        let dir = recording_dir(id).unwrap();
        assert_eq!(dir, crate::storage::recordings_dir(&crate::vault::meta_root().unwrap()).join(id));
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[test]
    fn read_recording_file_note_prefers_vault_body() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: env mutation requires `--test-threads=1` so no concurrent
        // env access races with these calls (same convention as transcribe).
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let root = crate::vault::meta_root().unwrap();
        let id = "2026-06-02T10-00-00Z";
        let dir = crate::storage::create_recording_dir(&root, id).unwrap();
        let mut meta = test_meta(id);
        meta.audio_file = Some("clip.mp3".into());
        crate::storage::write_meta(&dir, &meta).unwrap();
        crate::vault::write_note("2026-06-02 clip", &meta, "clip.mp3", "vault body").unwrap();

        assert_eq!(
            read_recording_file(id.into(), "note".into())
                .unwrap()
                .as_deref(),
            Some("vault body")
        );
        // Transcript still reads ~/.ariso.
        std::fs::write(dir.join("transcript.md"), b"tscript").unwrap();
        assert_eq!(
            read_recording_file(id.into(), "transcript".into())
                .unwrap()
                .as_deref(),
            Some("tscript")
        );
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[test]
    fn read_recording_file_note_falls_back_to_legacy_file() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: env mutation requires `--test-threads=1` so no concurrent
        // env access races with these calls (same convention as transcribe).
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let root = crate::vault::meta_root().unwrap();
        let id = "2026-06-02T11-00-00Z";
        let dir = crate::storage::create_recording_dir(&root, id).unwrap();
        let meta = test_meta(id);
        crate::storage::write_meta(&dir, &meta).unwrap();
        // No vault note written; only the legacy on-disk file exists.
        std::fs::write(dir.join("ari-note.md"), b"legacy note").unwrap();

        assert_eq!(
            read_recording_file(id.into(), "note".into())
                .unwrap()
                .as_deref(),
            Some("legacy note")
        );
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[test]
    fn read_recording_file_note_missing_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: env mutation requires `--test-threads=1` so no concurrent
        // env access races with these calls (same convention as transcribe).
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let root = crate::vault::meta_root().unwrap();
        let id = "2026-06-02T12-00-00Z";
        let dir = crate::storage::create_recording_dir(&root, id).unwrap();
        let meta = test_meta(id);
        crate::storage::write_meta(&dir, &meta).unwrap();
        // Neither a vault note nor a legacy file exists yet.

        assert_eq!(read_recording_file(id.into(), "note".into()).unwrap(), None);
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    /// The recovery copy in PendingUploads.vue renders this verbatim, so it has
    /// to be the real resolved folder rather than a POSIX-shaped `~/...` guess
    /// that is wrong on Windows and under the override.
    #[test]
    fn pending_uploads_path_reports_the_resolved_dir() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: env mutation requires `--test-threads=1` so no concurrent
        // env access races with these calls (same convention as transcribe).
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        let path = pending_uploads_path().unwrap();

        assert_eq!(
            std::path::PathBuf::from(&path),
            tmp.path().join("pending-uploads")
        );
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    fn test_meta(id: &str) -> crate::storage::RecordingMeta {
        crate::storage::RecordingMeta {
            id: id.into(),
            title: "Old".into(),
            created_at: "2026-06-02T14:30:05Z".into(),
            duration_seconds: 1,
            status: crate::storage::RecordingStatus::Done,
            language: None,
            participants: vec![],
            model_version: None,
            error: None,
            notes_error: None,
            last_clip_end_at: None,
            audio_file: None,
            notes_written: None,
            title_is_default: false,
        }
    }

    #[test]
    fn rename_clears_title_is_default() {
        // SAFETY: command tests run with --test-threads=1 (see plan conventions),
        // so the process-wide ARISO_ROOT mutation below has no concurrent writer.
        let tmp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let id = "2026-06-02T14-30-05Z";
        let dir = crate::storage::create_recording_dir(&crate::vault::meta_root().unwrap(), id).unwrap();
        let mut meta = test_meta(id);
        meta.title_is_default = true;
        meta.audio_file = None; // legacy path: skip vault propagation
        crate::storage::write_meta(&dir, &meta).unwrap();

        rename_local_recording(id.to_string(), "My Real Title".to_string()).unwrap();

        let after = crate::storage::read_meta(&dir).unwrap();
        unsafe { std::env::remove_var("ARISO_ROOT"); }
        assert_eq!(after.title, "My Real Title");
        assert!(!after.title_is_default);
    }

    #[test]
    fn read_recording_audio_resolves_vault_then_legacy() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: see above.
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let root = crate::vault::meta_root().unwrap();

        // New recording: audio in the vault via meta.audio_file.
        let id_new = "2026-06-02T10-00-00Z";
        let dir_new = crate::storage::create_recording_dir(&root, id_new).unwrap();
        let mut meta_new = test_meta(id_new);
        meta_new.audio_file = Some("clip.mp3".into());
        crate::storage::write_meta(&dir_new, &meta_new).unwrap();
        crate::vault::write_audio("clip.mp3", b"newbytes").unwrap();
        assert_eq!(read_recording_audio_bytes(id_new).unwrap(), b"newbytes");
        // The public command wraps the same resolution logic; smoke-test it too.
        assert!(read_recording_audio(id_new.into()).is_ok());

        // Legacy recording: no audio_file, audio in ~/.ariso.
        let id_old = "2026-06-01T10-00-00Z";
        let dir_old = crate::storage::create_recording_dir(&root, id_old).unwrap();
        crate::storage::write_meta(&dir_old, &test_meta(id_old)).unwrap();
        std::fs::write(dir_old.join("recording.mp3"), b"oldbytes").unwrap();
        assert_eq!(read_recording_audio_bytes(id_old).unwrap(), b"oldbytes");
        assert!(read_recording_audio(id_old.into()).is_ok());

        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[test]
    fn rename_local_recording_updates_title_in_meta() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: env mutation requires `--test-threads=1` so no concurrent
        // env access races with these calls (same convention as transcribe).
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let id = "2026-06-02T14-30-05Z";
        let dir = crate::storage::create_recording_dir(&crate::vault::meta_root().unwrap(), id).unwrap();
        crate::storage::write_meta(&dir, &test_meta(id)).unwrap();

        // Quotes round-trip through meta.json (serde escapes them); whitespace
        // is trimmed before saving. Only `title` changes.
        rename_local_recording(id.to_string(), "  Team sync \"Q2\"  ".to_string()).unwrap();

        let meta = crate::storage::read_meta(&dir).unwrap();
        assert_eq!(meta.title, "Team sync \"Q2\"");
        assert_eq!(meta.created_at, "2026-06-02T14:30:05Z");
        assert_eq!(meta.status, crate::storage::RecordingStatus::Done);
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[test]
    fn rename_local_recording_propagates_to_vault() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: env mutation requires `--test-threads=1`.
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let id = "2026-06-02T14-30-05Z";
        let dir = crate::storage::create_recording_dir(&crate::vault::meta_root().unwrap(), id).unwrap();
        let mut meta = test_meta(id);
        meta.audio_file = Some("2026-06-02 Old.mp3".into());
        crate::storage::write_meta(&dir, &meta).unwrap();
        crate::vault::write_audio("2026-06-02 Old.mp3", b"aud").unwrap();
        crate::vault::write_note("2026-06-02 Old", &meta, "2026-06-02 Old.mp3", "kept body").unwrap();

        rename_local_recording(id.to_string(), "Q2 Sync".to_string()).unwrap();

        // meta.audio_file now points at the renamed attachment.
        let meta2 = crate::storage::read_meta(&dir).unwrap();
        assert_eq!(meta2.title, "Q2 Sync");
        assert_eq!(meta2.audio_file.as_deref(), Some("2026-06-02 Q2 Sync.mp3"));
        // Vault note + attachment renamed; old gone; body preserved; still found by oats_id.
        let root = crate::vault::vault_root().unwrap();
        assert!(crate::vault::audio_path(&root, "2026-06-02 Q2 Sync.mp3").is_file());
        assert!(!crate::vault::audio_path(&root, "2026-06-02 Old.mp3").exists());
        assert!(!crate::vault::note_path(&root, "2026-06-02 Old").exists());
        assert_eq!(crate::vault::read_note(id).unwrap().as_deref(), Some("kept body"));
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[test]
    fn rename_local_recording_rejects_empty_title() {
        // Validation runs before any filesystem access, so no ARISO_ROOT needed.
        assert!(rename_local_recording("any-id".to_string(), "".to_string()).is_err());
        assert!(rename_local_recording("any-id".to_string(), "   ".to_string()).is_err());
    }

    #[test]
    fn rename_local_recording_rejects_over_limit_title_but_allows_40() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: see above.
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let id = "2026-06-02T14-30-05Z";
        let dir = crate::storage::create_recording_dir(&crate::vault::meta_root().unwrap(), id).unwrap();
        crate::storage::write_meta(&dir, &test_meta(id)).unwrap();

        // 41 characters is rejected without touching the file.
        assert!(rename_local_recording(id.to_string(), "x".repeat(41)).is_err());
        assert_eq!(crate::storage::read_meta(&dir).unwrap().title, "Old");

        // Exactly 40 characters saves.
        rename_local_recording(id.to_string(), "x".repeat(40)).unwrap();
        assert_eq!(crate::storage::read_meta(&dir).unwrap().title, "x".repeat(40));
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[test]
    fn rename_local_recording_rejects_missing_recording() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: see above.
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let res = rename_local_recording("2026-06-02T14-30-05Z".to_string(), "New".to_string());
        assert!(res.is_err());
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[test]
    fn meeting_id_must_be_digits_only() {
        assert!(validate_meeting_id("123").is_ok());
        assert!(validate_meeting_id("").is_err());
        assert!(validate_meeting_id("12/audio").is_err());
        assert!(validate_meeting_id("abc").is_err());
        assert!(validate_meeting_id("12 ").is_err());
    }

    #[test]
    fn meeting_audio_url_builds_legacy_and_per_clip() {
        let base = "https://api.example.com";
        assert_eq!(
            meeting_audio_url(base, "42", None).unwrap(),
            "https://api.example.com/meeting-notes/42/audio"
        );
        assert_eq!(
            meeting_audio_url(base, "42", Some("3f8c1e2a-0000-4aaa-8bbb-1234567890ab")).unwrap(),
            "https://api.example.com/meeting-notes/42/audio/3f8c1e2a-0000-4aaa-8bbb-1234567890ab"
        );
    }

    #[test]
    fn speaker_audio_url_addresses_a_diarization_index() {
        let base = "https://api.example.com";
        assert_eq!(
            speaker_audio_url(base, "42", 0).unwrap(),
            "https://api.example.com/meeting-notes/42/speakers/0/audio"
        );
        assert_eq!(
            speaker_audio_url(base, "42", 7).unwrap(),
            "https://api.example.com/meeting-notes/42/speakers/7/audio"
        );
    }

    #[test]
    fn speaker_audio_url_rejects_a_non_numeric_meeting_id() {
        let base = "https://api.example.com";
        assert!(speaker_audio_url(base, "../secret", 0).is_err());
        assert!(speaker_audio_url(base, "42/audio", 0).is_err());
        assert!(speaker_audio_url(base, "", 0).is_err());
    }

    #[test]
    fn meeting_audio_url_rejects_injection() {
        let base = "https://api.example.com";
        assert!(meeting_audio_url(base, "42", Some("../secret")).is_err());
        assert!(meeting_audio_url(base, "42", Some("a/b")).is_err());
        assert!(meeting_audio_url(base, "42", Some("a?x=1")).is_err());
        assert!(meeting_audio_url(base, "not-numeric", None).is_err());
    }

    #[test]
    fn recording_note_roundtrips_markdown() {
        let tmp = tempfile::tempdir().unwrap();
        // Note commands resolve through ARISO_ROOT, so this test follows the
        // same serial test command requirement as the recording-dir tests.
        unsafe {
            std::env::set_var("ARISO_ROOT", tmp.path());
        }

        let id = "2026-06-02T14-30-05Z";
        std::fs::create_dir_all(crate::storage::recordings_dir(&crate::vault::meta_root().unwrap()).join(id)).unwrap();
        assert_eq!(read_recording_note(id.into()).unwrap(), "");
        write_recording_note(id.into(), "# Note\n- point".into()).unwrap();
        let saved = read_recording_note(id.into()).unwrap();
        assert_eq!(saved, "# Note\n- point");
        assert!(crate::storage::recordings_dir(&crate::vault::meta_root().unwrap()).join(id).join("user-note.md").is_file());

        unsafe {
            std::env::remove_var("ARISO_ROOT");
        }
    }

    #[test]
    fn recording_note_title_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("ARISO_ROOT", tmp.path());
        }

        let id = "2026-06-02T14-30-05Z";
        std::fs::create_dir_all(crate::storage::recordings_dir(&crate::vault::meta_root().unwrap()).join(id)).unwrap();
        // Missing sidecar reads as empty so a fresh recording has no title yet.
        assert_eq!(read_recording_note_title(id.into()).unwrap(), "");
        write_recording_note_title(id.into(), "Kickoff sync".into()).unwrap();
        let saved = read_recording_note_title(id.into()).unwrap();
        assert_eq!(saved, "Kickoff sync");
        assert!(crate::storage::recordings_dir(&crate::vault::meta_root().unwrap())
            .join(id)
            .join("user-note-title.txt")
            .is_file());

        unsafe {
            std::env::remove_var("ARISO_ROOT");
        }
    }

    #[test]
    fn recording_note_read_rejects_oversized_file() {
        let tmp = tempfile::tempdir().unwrap();
        // Note commands resolve through ARISO_ROOT, so this test follows the
        // same serial test command requirement as the recording-dir tests.
        unsafe {
            std::env::set_var("ARISO_ROOT", tmp.path());
        }

        let id = "2026-06-02T14-30-05Z";
        let dir = crate::storage::recordings_dir(&crate::vault::meta_root().unwrap()).join(id);
        std::fs::create_dir_all(&dir).unwrap();
        let file = std::fs::File::create(dir.join("user-note.md")).unwrap();
        file.set_len(MAX_TEXT_BYTES + 1).unwrap();

        let err = read_recording_note(id.into()).unwrap_err();
        assert!(err.contains("recording note too large to read"));

        unsafe {
            std::env::remove_var("ARISO_ROOT");
        }
    }

    #[test]
    fn recording_note_write_rejects_oversized_markdown() {
        let tmp = tempfile::tempdir().unwrap();
        // Note commands resolve through ARISO_ROOT, so this test follows the
        // same serial test command requirement as the recording-dir tests.
        unsafe {
            std::env::set_var("ARISO_ROOT", tmp.path());
        }

        let id = "2026-06-02T14-30-05Z";
        let markdown = "x".repeat((MAX_TEXT_BYTES + 1) as usize);
        let err = write_recording_note(id.into(), markdown).unwrap_err();
        assert!(err.contains("recording note too large to write"));
        assert!(!crate::storage::recordings_dir(&crate::vault::meta_root().unwrap()).join(id).exists());

        unsafe {
            std::env::remove_var("ARISO_ROOT");
        }
    }

    #[test]
    fn local_recording_status_reports_derived_notes_status() {
        // SAFETY: command tests run with --test-threads=1 (see plan conventions),
        // so the process-wide ARISO_ROOT mutation below has no concurrent writer.
        let tmp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }

        let id = "2026-06-02T14-30-05Z";
        let dir = crate::storage::create_recording_dir(&crate::vault::meta_root().unwrap(), id).unwrap();
        let mut meta = crate::storage::RecordingMeta {
            id: id.into(),
            title: "T".into(),
            created_at: "2026-06-02T14:30:05Z".into(),
            duration_seconds: 5,
            status: crate::storage::RecordingStatus::Done,
            language: None,
            participants: vec![],
            model_version: None,
            error: None,
            notes_error: None,
            last_clip_end_at: None,
            audio_file: None,
            notes_written: None,
            title_is_default: false,
        };
        crate::storage::write_meta(&dir, &meta).unwrap();
        std::fs::write(dir.join("transcript.md"), b"t").unwrap();

        // Transcript present, note absent, no error -> notes pending.
        let view = local_recording_status(id.to_string()).unwrap();
        assert_eq!(view.status, crate::storage::RecordingStatus::Done);
        assert!(view.has_transcript);
        assert!(!view.has_note);
        assert_eq!(view.notes_status, crate::storage::NotesStatus::Pending);

        // Record a notes failure -> notes failed.
        meta.notes_error = Some("boom".into());
        crate::storage::write_meta(&dir, &meta).unwrap();
        let view = local_recording_status(id.to_string()).unwrap();
        assert_eq!(view.notes_status, crate::storage::NotesStatus::Failed);

        // Write the note file -> notes ready (note presence wins over the error).
        std::fs::write(dir.join("ari-note.md"), b"n").unwrap();
        let view = local_recording_status(id.to_string()).unwrap();
        assert!(view.has_note);
        assert_eq!(view.notes_status, crate::storage::NotesStatus::Ready);

        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[test]
    fn local_recording_status_rejects_bad_id() {
        assert!(local_recording_status("../escape".to_string()).is_err());
    }

    #[test]
    fn list_local_recordings_marks_has_note_from_vault() {
        // SAFETY: command tests run with --test-threads=1 (see plan conventions),
        // so the process-wide ARISO_ROOT mutation below has no concurrent writer.
        let tmp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let root = crate::vault::meta_root().unwrap();
        let id = "2026-06-02T10-00-00Z";
        let dir = crate::storage::create_recording_dir(&root, id).unwrap();
        let mut meta = test_meta(id);
        meta.audio_file = Some("clip.mp3".into());
        crate::storage::write_meta(&dir, &meta).unwrap();
        crate::vault::write_note("2026-06-02 clip", &meta, "clip.mp3", "b").unwrap();

        let list = list_local_recordings().unwrap();
        assert!(list.iter().find(|s| s.id == id).unwrap().has_note);
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[test]
    fn list_local_recordings_reflects_vault_audio_deletion() {
        // SAFETY: command tests run with --test-threads=1 (see plan conventions),
        // so the process-wide ARISO_ROOT mutation below has no concurrent writer.
        let tmp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let root = crate::vault::meta_root().unwrap();
        let id = "2026-06-02T10-00-00Z";
        let dir = crate::storage::create_recording_dir(&root, id).unwrap();
        let mut meta = test_meta(id);
        meta.audio_file = Some("clip.mp3".into());
        crate::storage::write_meta(&dir, &meta).unwrap();
        // No attachment written yet: the vault attachment does not exist, so
        // has_audio must reflect that even though meta.audio_file is set.
        let list = list_local_recordings().unwrap();
        assert!(
            !list.iter().find(|s| s.id == id).unwrap().has_audio,
            "attachment missing ⇒ has_audio should be false"
        );

        // Write the attachment: has_audio should flip to true.
        crate::vault::write_audio("clip.mp3", b"bytes").unwrap();
        let list = list_local_recordings().unwrap();
        assert!(
            list.iter().find(|s| s.id == id).unwrap().has_audio,
            "attachment present ⇒ has_audio should be true"
        );

        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[test]
    fn copy_recording_file_copies_transcript_byte_for_byte() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: env mutation requires `--test-threads=1` so no concurrent
        // env access races with these calls (same convention as transcribe).
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let root = crate::vault::meta_root().unwrap();
        let id = "2026-08-30T10-00-00Z";
        let dir = crate::storage::create_recording_dir(&root, id).unwrap();
        // Front-matter included: the export is a copy, not a re-render.
        let body = "---\ntitle: \"Weekly sync\"\n---\n\n**Speaker 1** [0:00]\nHello\n";
        std::fs::write(dir.join("transcript.md"), body).unwrap();

        let dest = tmp.path().join("exported.md");
        copy_recording_file(
            id.into(),
            "transcript".into(),
            dest.to_string_lossy().into_owned(),
        )
        .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), body.as_bytes());
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[test]
    fn copy_recording_file_refuses_to_copy_onto_the_source() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: env mutation requires `--test-threads=1` so no concurrent
        // env access races with these calls (same convention as transcribe).
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let root = crate::vault::meta_root().unwrap();
        let id = "2026-08-30T12-00-00Z";
        let dir = crate::storage::create_recording_dir(&root, id).unwrap();
        let body = "---\ntitle: \"T\"\n---\n\n**Speaker 1** [0:00]\nHello\n";
        let src = dir.join("transcript.md");
        std::fs::write(&src, body).unwrap();

        let err = copy_recording_file(
            id.into(),
            "transcript".into(),
            src.to_string_lossy().into_owned(),
        )
        .unwrap_err();

        assert!(err.contains("outside the vault"), "unexpected error: {err}");
        // The guard must fire BEFORE the copy — otherwise the file is already zeroed.
        assert_eq!(std::fs::read(&src).unwrap(), body.as_bytes());
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }

    #[test]
    fn copy_recording_file_rejects_relative_destination() {
        // The absolute-path guard runs before any vault lookup, so this branch
        // needs no ARISO_ROOT and touches no filesystem.
        let err = copy_recording_file(
            "2026-08-30T10-00-00Z".into(),
            "transcript".into(),
            "relative/export.md".into(),
        )
        .unwrap_err();
        assert!(err.contains("absolute"), "unexpected error: {err}");
    }

    #[test]
    fn copy_recording_file_errors_when_transcript_missing() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: env mutation requires `--test-threads=1` so no concurrent
        // env access races with these calls (same convention as transcribe).
        unsafe { std::env::set_var("ARISO_ROOT", tmp.path()); }
        let root = crate::vault::meta_root().unwrap();
        let id = "2026-08-30T11-00-00Z";
        crate::storage::create_recording_dir(&root, id).unwrap();

        let dest = tmp.path().join("exported.md");
        let err = copy_recording_file(
            id.into(),
            "transcript".into(),
            dest.to_string_lossy().into_owned(),
        )
        .unwrap_err();

        assert!(err.contains("recording file not found"), "unexpected error: {err}");
        assert!(!dest.exists(), "no file should be created when the source is missing");
        unsafe { std::env::remove_var("ARISO_ROOT"); }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod share_tests {
    use super::flip_y;
    #[test]
    fn flips_web_top_left_to_appkit_bottom_left() {
        // view 600 tall, button at css-y=40 height=32 -> appkit-y = 600-(40+32)=528
        assert_eq!(flip_y(600.0, 40.0, 32.0), 528.0);
    }
}
