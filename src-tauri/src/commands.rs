use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tauri::webview::WebviewWindowBuilder;
use tauri::{Emitter, Manager};
use tauri_plugin_store::StoreExt;
use tokio::sync::oneshot;
use url::Url;

const APP_USER_AGENT: &str = "ArisoDesktop/0.2.1";

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

/// Initiates Google OAuth sign-in using a native webview window.
/// Opens an OAuth window, intercepts the magic-link redirect, exchanges
/// the token for a session, and returns the result.
#[tauri::command]
pub async fn google_sign_in(app: tauri::AppHandle) -> Result<SignInResult, String> {
    let client = http_client();

    // Step 1: Get the OAuth redirect URL from the API. The backend expands
    // these service names into Google scopes and owns credential persistence.
    let response = client
        .post(format!("{}/oauth2/prepare-state", api_base_url()))
        .header(CONTENT_TYPE, "application/json")
        .body(
            r#"{"integration":"google-signin","scopes":["calendar-readonly"],"newUserSignupIntent":"personal_unless_domain_autojoin"}"#,
        )
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        return Ok(SignInResult {
            success: None,
            session_token: None,
            error: Some(format!("API returned {status}")),
        });
    }

    let body: PrepareStateResponse = response.json().await.map_err(|e| e.to_string())?;

    // Step 2: Open a native webview window for OAuth
    let (tx, rx) = oneshot::channel::<Result<String, String>>();
    let tx = std::sync::Mutex::new(Some(tx));

    let auth_window = WebviewWindowBuilder::new(&app, "oauth", tauri::WebviewUrl::External(
        Url::parse(&body.redirect_url).map_err(|e| e.to_string())?,
    ))
    .title("Sign in with Google")
    .inner_size(500.0, 700.0)
    .on_navigation(move |url| {
        // Intercept magic-link redirect
        if url.path().contains("magic-link") {
            let token = url
                .query_pairs()
                .find(|(key, _)| key == "token")
                .map(|(_, value)| value.to_string());

            if let Some(sender) = tx.lock().unwrap().take() {
                match token {
                    Some(t) => { let _ = sender.send(Ok(t)); }
                    None => { let _ = sender.send(Err("No token in callback URL".into())); }
                }
            }

            // Block navigation — we've captured the token
            return false;
        }
        true
    })
    .build()
    .map_err(|e| e.to_string())?;

    // Listen for the window being closed by the user
    let auth_window_clone = auth_window.clone();
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = rx.await;

        // Close the OAuth window if it's still open
        let _ = auth_window_clone.close();

        match result {
            Ok(Ok(token)) => {
                // Step 3: Exchange the magic-link token for a session
                let exchange_result = exchange_token_for_session(&app_clone, &token).await;
                let _ = app_clone.emit("oauth-result", exchange_result);
            }
            Ok(Err(err)) => {
                let _ = app_clone.emit("oauth-result", SignInResult {
                    success: None,
                    session_token: None,
                    error: Some(err),
                });
            }
            Err(_) => {
                // Channel dropped — window was closed
                let _ = app_clone.emit("oauth-result", SignInResult {
                    success: None,
                    session_token: None,
                    error: Some("Auth window closed".into()),
                });
            }
        }
    });

    // Return immediately — the frontend listens for the "oauth-result" event
    Ok(SignInResult {
        success: None,
        session_token: None,
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
    crate::tray::set_menu(&app, is_recording, is_paused);
    let state = app.state::<crate::recording_state::RecordingState>();
    if is_recording {
        // The recorder window reports this right after capture starts; the
        // pill visibility watcher waits for it before hiding the window.
        state.mark_capture_active();
    } else {
        state.clear();
        let _ = app.emit("recording://state", false);
    }
    Ok(())
}

#[tauri::command]
pub async fn create_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::{WebviewWindowBuilder, WebviewUrl};

    // Focus if already exists
    if let Some(win) = app.get_webview_window("settings") {
        win.show().map_err(|e: tauri::Error| e.to_string())?;
        win.set_focus().map_err(|e: tauri::Error| e.to_string())?;
        return Ok(());
    }

    WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App("/#/settings".into()))
        .title("Ariso Settings")
        .inner_size(450.0, 800.0)
        .resizable(false)
        .center()
        .build()
        .map_err(|e| e.to_string())?;

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
        .title("Welcome to Ariso")
        .inner_size(450.0, 600.0)
        .resizable(false)
        .center()
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Shared helper to open the waveform recording window. Used by the
/// `start_recording_window` command, the tray (Local backend path), and the
/// auto mic monitor. `auto` adds `auto=1` to the URL and tags the shared
/// `RecordingState` as an auto recording.
pub(crate) fn open_waveform_window(
    app: &tauri::AppHandle,
    meeting_id: Option<i64>,
    auto: bool,
) -> Result<(), String> {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

    if let Some(picker) = app.get_webview_window("meeting-picker") {
        let _ = picker.close();
    }
    if let Some(existing) = app.get_webview_window("waveform") {
        let _ = existing.set_focus();
        return Ok(());
    }
    let mut url = match meeting_id {
        Some(id) => format!("/#/waveform?meetingId={id}"),
        None => "/#/waveform".to_string(),
    };
    if auto {
        url.push_str(if url.contains('?') { "&auto=1" } else { "?auto=1" });
    }
    let win = WebviewWindowBuilder::new(app, "waveform", WebviewUrl::App(url.into()))
        .title("")
        // Fixed size: room for the expanded pill plus its CSS shadow. The pill
        // itself is anchored to the bottom and grows upward within this window.
        .inner_size(92.0, 284.0)
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
        .map_err(|e| e.to_string())?;

    let source = if auto {
        crate::recording_state::RecordingSource::Auto
    } else {
        crate::recording_state::RecordingSource::Manual
    };
    app.state::<crate::recording_state::RecordingState>()
        .set(source, meeting_id);
    let _ = app.emit("recording://state", true);

    // If the window is destroyed without a clean stop (crash / force-close),
    // clear the shared flag so the monitor can recover and re-arm.
    let app_for_event = app.clone();
    win.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            app_for_event
                .state::<crate::recording_state::RecordingState>()
                .clear();
            let _ = app_for_event.emit("recording://state", false);
        }
    });

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

/// Open the waveform recording window, optionally attaching to an existing
/// meeting id. Closes the meeting-picker window if present and flips the
/// tray menu to the recording state.
#[tauri::command]
pub async fn start_recording_window(
    app: tauri::AppHandle,
    meeting_id: Option<i64>,
) -> Result<(), String> {
    open_waveform_window(&app, meeting_id, false)
}

/// Show/focus the meeting-picker window, building it if absent. Shared by the
/// tray (Ariso path) and the `open_meeting_picker` command so both open the
/// picker identically.
pub(crate) fn open_meeting_picker_window(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    if let Some(picker) = app.get_webview_window("meeting-picker") {
        let _ = picker.show();
        let _ = picker.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(app, "meeting-picker", WebviewUrl::App("/#/meeting-picker".into()))
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
pub async fn open_meeting_picker(app: tauri::AppHandle) -> Result<(), String> {
    open_meeting_picker_window(&app)
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
}

/// Returns build-baked client config (Pusher key/cluster, web app base URL).
#[tauri::command]
pub fn get_desktop_config() -> DesktopConfig {
    DesktopConfig {
        pusher_key: PUSHER_KEY.to_string(),
        pusher_cluster: PUSHER_CLUSTER.to_string(),
        web_app_base_url: web_app_base_url(),
    }
}

#[tauri::command]
pub fn list_local_recordings() -> Result<Vec<crate::storage::RecordingSummary>, String> {
    let root = crate::storage::ariso_root()?;
    crate::storage::list_recordings(&root)
}

/// Resolve a recording's directory under `<ariso_root>/recordings/<id>`,
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
    let root = crate::storage::ariso_root()?;
    Ok(crate::storage::recordings_dir(&root).join(id))
}

/// Map an openable file `kind` to its on-disk filename. Only `note` and
/// `transcript` are valid; anything else is an error.
fn note_or_transcript_filename(kind: &str) -> Result<&'static str, String> {
    match kind {
        "note" => Ok("meeting-note.md"),
        "transcript" => Ok("transcript.md"),
        other => Err(format!("invalid recording file kind: {other}")),
    }
}

/// Upper bound on the audio we'll load into memory for playback. The whole file
/// is read into RAM and copied across IPC into a JS Blob, so this guards against
/// OOM on a corrupt or pathologically large file. ~1 GB is far above any real
/// meeting recording (mp3 at this app's bitrate is well under 1 MB/min).
const MAX_AUDIO_BYTES: u64 = 1024 * 1024 * 1024;

/// Read the raw bytes of a recording's `recording.mp3`, returned as a raw
/// binary IPC response so the frontend can build a Blob URL for an `<audio>`
/// element (avoids JSON-array bloat from a `Vec<u8>` return).
#[tauri::command]
pub fn read_recording_audio(id: String) -> Result<tauri::ipc::Response, String> {
    let path = recording_dir(&id)?.join("recording.mp3");
    let size = std::fs::metadata(&path)
        .map_err(|e| format!("read recording audio: {e}"))?
        .len();
    if size > MAX_AUDIO_BYTES {
        return Err(format!("recording audio too large to play: {size} bytes"));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("read recording audio: {e}"))?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Upper bound on a note/transcript markdown file we'll read into memory for
/// in-app rendering. These are plain text; 16 MB is far above any real note or
/// transcript and just guards against a pathological/corrupt file.
const MAX_TEXT_BYTES: u64 = 16 * 1024 * 1024;

/// Read a recording's `meeting-note.md` or `transcript.md` as UTF-8 so the
/// frontend can render it inline. Returns `Ok(None)` when the file doesn't
/// exist yet (a normal "not generated" state), distinct from a read error.
/// `kind` must be `"note"` or `"transcript"`.
#[tauri::command]
pub fn read_recording_file(id: String, kind: String) -> Result<Option<String>, String> {
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

/// Open a recording's `meeting-note.md` or `transcript.md` in the OS default app.
/// `kind` must be `"note"` or `"transcript"`.
#[tauri::command]
pub fn open_recording_file(app: tauri::AppHandle, id: String, kind: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    let filename = note_or_transcript_filename(&kind)?;
    let path = recording_dir(&id)?.join(filename);
    if !path.exists() {
        return Err(format!("recording file not found: {}", path.display()));
    }
    app.opener()
        .open_path(path.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Read the user-authored local note artifact used by the Library editor.
/// Missing notes return an empty string so a fresh recording can autosave into
/// `user-note.md` without affecting generated Overview content.
#[tauri::command]
pub fn read_recording_note(id: String) -> Result<String, String> {
    let path = recording_dir(&id)?.join("user-note.md");
    match std::fs::read_to_string(&path) {
        Ok(contents) => Ok(contents),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("read recording note: {e}")),
    }
}

/// Persist user-authored in-meeting notes to `user-note.md` beside the
/// recording. Generated meeting notes use `meeting-note.md`, keeping Overview
/// visibility independent from My note autosaves.
#[tauri::command]
pub fn write_recording_note(id: String, markdown: String) -> Result<(), String> {
    let dir = recording_dir(&id)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("create recording note dir: {e}"))?;
    crate::storage::write_atomic(&dir.join("user-note.md"), markdown.as_bytes())
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
    use tauri::{TitleBarStyle, WebviewUrl, WebviewWindowBuilder};
    // The library window has no hide-on-close handler, so it is destroyed on
    // close and recreated (with fresh data) on the next open. This branch only
    // fires if it is opened again while still visible — just focus it.
    if let Some(win) = app.get_webview_window("library") {
        // Restore the window if it was minimized/hidden before focusing it.
        let _ = win.unminimize();
        let _ = win.show();
        win.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    // Overlay title bar (with the native title hidden) lets the web content
    // extend under the traffic lights, so the in-app panel toggle can sit on
    // the same row, just to the right of them.
    WebviewWindowBuilder::new(&app, "library", WebviewUrl::App("/#/library".into()))
        .title("Meetings")
        .title_bar_style(TitleBarStyle::Overlay)
        .hidden_title(true)
        .inner_size(900.0, 600.0)
        .resizable(true)
        .center()
        .skip_taskbar(true)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_or_transcript_filename_maps_known_kinds() {
        assert_eq!(note_or_transcript_filename("note").unwrap(), "meeting-note.md");
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
        assert_eq!(dir, crate::storage::recordings_dir(tmp.path()).join(id));
        unsafe { std::env::remove_var("ARISO_ROOT"); }
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
        std::fs::create_dir_all(crate::storage::recordings_dir(tmp.path()).join(id)).unwrap();
        assert_eq!(read_recording_note(id.into()).unwrap(), "");
        write_recording_note(id.into(), "# Note\n- point".into()).unwrap();
        let saved = read_recording_note(id.into()).unwrap();
        assert_eq!(saved, "# Note\n- point");
        assert!(crate::storage::recordings_dir(tmp.path()).join(id).join("user-note.md").is_file());

        unsafe {
            std::env::remove_var("ARISO_ROOT");
        }
    }
}
