use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tauri::webview::WebviewWindowBuilder;
use tauri::{Emitter, Manager};
use tauri_plugin_store::StoreExt;
use tokio::sync::oneshot;
use url::Url;

const APP_USER_AGENT: &str = "ArisoDesktop/0.2.0";

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .build()
        .expect("failed to build HTTP client")
}

#[cfg(all(feature = "prod-api", feature = "dev-api"))]
compile_error!("Features `prod-api` and `dev-api` are mutually exclusive");

#[cfg(feature = "prod-api")]
const API_BASE_URL: &str = "https://api.ari.ariso.ai";
#[cfg(feature = "dev-api")]
const API_BASE_URL: &str = "https://api-dev.ari.ariso.ai";
#[cfg(not(any(feature = "prod-api", feature = "dev-api")))]
const API_BASE_URL: &str = "http://localhost:4000";
const STORE_PATH: &str = "session.json";
const SESSION_KEY: &str = "session_token";

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

fn get_session_token(app: &tauri::AppHandle) -> Option<String> {
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
        .get(format!("{API_BASE_URL}/auth/session"))
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

fn clear_session_token(app: &tauri::AppHandle) -> Result<(), String> {
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

    // Step 1: Get the OAuth redirect URL from the API
    let response = client
        .post(format!("{API_BASE_URL}/oauth2/prepare-state"))
        .header(CONTENT_TYPE, "application/json")
        .body(r#"{"integration":"google-signin"}"#)
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
        .get(format!("{API_BASE_URL}/auth/check"))
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
        .get(format!("{API_BASE_URL}/auth/session"))
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
    let url = format!("{API_BASE_URL}{path}");

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
    let url = format!("{API_BASE_URL}{path}");

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
    Ok(())
}

#[tauri::command]
pub async fn create_waveform_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::{WebviewWindowBuilder, WebviewUrl};

    // Don't create if already exists
    if app.get_webview_window("waveform").is_some() {
        return Ok(());
    }

    WebviewWindowBuilder::new(&app, "waveform", WebviewUrl::App("/#/waveform".into()))
        .title("")
        .inner_size(280.0, 56.0)
        .decorations(false)
        .always_on_top(true)
        .resizable(false)
        .shadow(true)
        .skip_taskbar(true)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn destroy_waveform_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("waveform") {
        win.close().map_err(|e: tauri::Error| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn create_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::{WebviewWindowBuilder, WebviewUrl};

    // Focus if already exists
    if let Some(win) = app.get_webview_window("settings") {
        win.set_focus().map_err(|e: tauri::Error| e.to_string())?;
        return Ok(());
    }

    WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App("/#/settings".into()))
        .title("Ariso Settings")
        .inner_size(450.0, 520.0)
        .resizable(false)
        .center()
        .build()
        .map_err(|e| e.to_string())?;

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
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    // Close the meeting picker window if it exists.
    if let Some(picker) = app.get_webview_window("meeting-picker") {
        let _ = picker.close();
    }

    // If a waveform window already exists, focus it instead of recreating.
    if let Some(existing) = app.get_webview_window("waveform") {
        let _ = existing.set_focus();
        return Ok(());
    }

    let url = match meeting_id {
        Some(id) => format!("/#/waveform?meetingId={id}"),
        None => "/#/waveform".to_string(),
    };

    WebviewWindowBuilder::new(&app, "waveform", WebviewUrl::App(url.into()))
        .title("")
        .inner_size(320.0, 56.0)
        .decorations(false)
        .always_on_top(true)
        .resizable(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .build()
        .map_err(|e| e.to_string())?;

    crate::tray::set_menu(&app, true, false);
    Ok(())
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
