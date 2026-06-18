//! Meeting-prep notification orchestrator (native).
//!
//! The Pusher connection lives here in the Rust process — NOT in a webview —
//! because macOS suspends hidden/occluded webviews, which froze the old
//! JS-based listener and dropped events. The native process is never
//! suspended, so the connection stays alive in the background.
//!
//! Reliability: realtime delivery is best-effort (Pusher does not redeliver to
//! disconnected clients), so on every (re)subscribe we also run a catch-up
//! fetch of the inbox and surface any meeting-prep we haven't notified yet.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_store::StoreExt;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

use crate::commands::{
    api_base_url, clear_session_token, get_session_token, http_client, web_app_base_url,
    PUSHER_CLUSTER, PUSHER_KEY,
};

const SETTINGS_PATH: &str = "settings.json";
const ENABLED_KEY: &str = "meetingNotificationsEnabled";
const MEETING_PREP_SOURCE: &str = "meeting_prep";

/// Hard cap on a single websocket handshake. Without this a hung TCP/TLS
/// connect can stall the orchestrator past the point where run_loop's
/// reconnect/backoff could otherwise recover.
const WS_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Per-HTTP-request cap covering both `.send()` and body decode. Same reason as
/// the WS timeout; reqwest's builder isn't configured with a request timeout
/// (see `commands::http_client`).
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

/// Errors raised by the session-bound HTTP/WS calls. `Auth` means the server
/// rejected the stored session (401/403) — distinct because it requires
/// tearing down the orchestrator instead of retrying.
enum SessionError {
    Auth,
    Other(String),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auth => f.write_str("session invalid (auth rejected)"),
            Self::Other(e) => f.write_str(e),
        }
    }
}

/// Holds the handle to the running orchestrator task (if any).
#[derive(Default)]
pub struct NotificationManager {
    handle: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

impl NotificationManager {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Whether the user has notifications enabled (defaults to true).
fn notifications_enabled(app: &AppHandle) -> bool {
    match app.store(SETTINGS_PATH) {
        Ok(store) => !matches!(store.get(ENABLED_KEY), Some(Value::Bool(false))),
        Err(_) => true,
    }
}

/// Start the orchestrator if the user is signed in and notifications are
/// enabled; otherwise stop it. Safe to call repeatedly (sign-in/out, toggle).
pub async fn sync(app: &AppHandle) {
    let desired = notifications_enabled(app) && get_session_token(app).is_some();
    if !desired {
        stop(app);
        return;
    }
    let mgr = app.state::<NotificationManager>();
    let mut guard = mgr.handle.lock().unwrap();
    // The task loops forever and is only removed via `stop()` (abort), so a
    // present handle means it's already running.
    if guard.is_some() {
        return;
    }
    let app = app.clone();
    *guard = Some(tauri::async_runtime::spawn(async move {
        run_loop(app).await;
    }));
}

/// Stop and tear down the orchestrator task.
pub fn stop(app: &AppHandle) {
    let mgr = app.state::<NotificationManager>();
    let mut guard = mgr.handle.lock().unwrap();
    if let Some(handle) = guard.take() {
        handle.abort();
    }
}

/// Supervisor loop: (re)connect with exponential backoff until the session is
/// invalidated. A `seen` set (shared across reconnects) dedupes notifications
/// between the realtime path and the catch-up fetch.
///
/// On `SessionError::Auth` the loop exits: the stored token is cleared, the
/// orchestrator handle is reset, and we wait for the next sign-in (a
/// `SYNC_EVENT` broadcast) to call `sync()` and re-spawn us. Without this
/// the loop would reauth every 1–30s forever against a server that has
/// already invalidated the session.
async fn run_loop(app: AppHandle) {
    let mut backoff = 1u64;
    let mut seen: HashSet<i64> = HashSet::new();
    // First successful subscribe seeds `seen` with the current inbox WITHOUT
    // notifying, so launch doesn't replay every old prep as a banner.
    let mut first = true;
    loop {
        match run_session(&app, &mut seen, &mut first).await {
            Ok(()) => backoff = 1,
            Err(SessionError::Auth) => {
                eprintln!(
                    "meeting-notifications: session invalid; clearing token and stopping orchestrator"
                );
                let _ = clear_session_token(&app);
                break;
            }
            Err(SessionError::Other(e)) => {
                eprintln!("meeting-notifications: session error: {e}");
            }
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
    // Reset the manager handle so a future `sync()` (e.g. after sign-in) can
    // spawn us again. Without this the `guard.is_some()` early-return in
    // `sync()` would keep the orchestrator off until app restart.
    let mgr = app.state::<NotificationManager>();
    let mut guard = mgr.handle.lock().unwrap();
    *guard = None;
}

/// One connection lifecycle: connect → auth → subscribe → read until close.
async fn run_session(
    app: &AppHandle,
    seen: &mut HashSet<i64>,
    first: &mut bool,
) -> Result<(), SessionError> {
    let (org_id, user_id) = fetch_me(app).await?;
    let channel = format!("private-{org_id}-{user_id}");

    let url = format!(
        "wss://ws-{PUSHER_CLUSTER}.pusher.com/app/{PUSHER_KEY}?protocol=7&client=ariso-desktop&version=0.2.1"
    );
    let (ws, _) = tokio::time::timeout(WS_CONNECT_TIMEOUT, tokio_tungstenite::connect_async(&url))
        .await
        .map_err(|_| SessionError::Other("connect: timed out".into()))?
        .map_err(|e| SessionError::Other(format!("connect: {e}")))?;
    let (mut write, mut read) = ws.split();

    // Application-level keepalive so a dead connection is detected promptly.
    let mut ping = tokio::time::interval(Duration::from_secs(30));

    loop {
        tokio::select! {
            maybe_msg = read.next() => {
                let msg = match maybe_msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => return Err(SessionError::Other(format!("read: {e}"))),
                    None => return Ok(()),
                };
                let text = match msg {
                    Message::Text(t) => t.to_string(),
                    Message::Ping(p) => { let _ = write.send(Message::Pong(p)).await; continue; }
                    Message::Close(_) => return Ok(()),
                    _ => continue,
                };
                handle_message(app, &mut write, &channel, &text, seen, first).await?;
            }
            _ = ping.tick() => {
                let _ = write
                    .send(Message::Text(r#"{"event":"pusher:ping","data":"{}"}"#.to_string()))
                    .await;
            }
        }
    }
}

type WsWrite = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    Message,
>;

async fn handle_message(
    app: &AppHandle,
    write: &mut WsWrite,
    channel: &str,
    text: &str,
    seen: &mut HashSet<i64>,
    first: &mut bool,
) -> Result<(), SessionError> {
    let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let event = v.get("event").and_then(Value::as_str).unwrap_or("");

    match event {
        "pusher:connection_established" => {
            let socket_id = inner_json(&v)
                .get("socket_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let auth = pusher_auth(app, &socket_id, channel).await?;
            let sub = json!({
                "event": "pusher:subscribe",
                "data": { "channel": channel, "auth": auth }
            });
            write
                .send(Message::Text(sub.to_string()))
                .await
                .map_err(|e| SessionError::Other(format!("subscribe send: {e}")))?;
        }
        "pusher:ping" => {
            let _ = write
                .send(Message::Text(r#"{"event":"pusher:pong","data":"{}"}"#.to_string()))
                .await;
        }
        "pusher:error" => eprintln!("meeting-notifications: pusher error: {text}"),
        "pusher_internal:subscription_succeeded" => {
            catch_up(app, seen, *first).await?;
            *first = false;
        }
        "meeting-prep-complete" => {
            if let Some(prep_id) = inner_json(&v).get("meetingPrepId").and_then(Value::as_i64) {
                handle_prep(app, prep_id, seen).await?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Pusher wraps payloads as a JSON-encoded string in `data`; decode it. Falls
/// back to treating `data` as an object if it's already one.
fn inner_json(v: &Value) -> Value {
    match v.get("data") {
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
        Some(other) => other.clone(),
        None => Value::Null,
    }
}

/// GET /auth/me → (org_id, user_id) for the channel name. Both fields may come
/// back as either a JSON string or number, so coerce to string either way.
async fn fetch_me(app: &AppHandle) -> Result<(String, String), SessionError> {
    let token = get_session_token(app).ok_or(SessionError::Auth)?;
    let v: Value = tokio::time::timeout(HTTP_TIMEOUT, async {
        let resp = http_client()
            .get(format!("{}/auth/me", api_base_url()))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await
            .map_err(|e| SessionError::Other(e.to_string()))?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN
        {
            return Err(SessionError::Auth);
        }
        if !status.is_success() {
            return Err(SessionError::Other(format!(
                "/auth/me returned {}",
                status.as_u16()
            )));
        }
        resp.json::<Value>()
            .await
            .map_err(|e| SessionError::Other(e.to_string()))
    })
    .await
    .map_err(|_| SessionError::Other("/auth/me timed out".into()))??;
    let org_id =
        value_to_string(v.get("org_id")).ok_or_else(|| SessionError::Other("missing org_id".into()))?;
    let user_id =
        value_to_string(v.get("id")).ok_or_else(|| SessionError::Other("missing id".into()))?;
    Ok((org_id, user_id))
}

/// Coerce a JSON string or number to a String (for channel-name building).
fn value_to_string(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

/// POST /pusher/auth { socketId, channelName } → auth signature.
async fn pusher_auth(
    app: &AppHandle,
    socket_id: &str,
    channel: &str,
) -> Result<String, SessionError> {
    let token = get_session_token(app).ok_or(SessionError::Auth)?;
    let body = json!({ "socketId": socket_id, "channelName": channel });
    let v: Value = tokio::time::timeout(HTTP_TIMEOUT, async {
        let resp = http_client()
            .post(format!("{}/pusher/auth", api_base_url()))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| SessionError::Other(e.to_string()))?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN
        {
            return Err(SessionError::Auth);
        }
        if !status.is_success() {
            return Err(SessionError::Other(format!(
                "/pusher/auth returned {}",
                status.as_u16()
            )));
        }
        resp.json::<Value>()
            .await
            .map_err(|e| SessionError::Other(e.to_string()))
    })
    .await
    .map_err(|_| SessionError::Other("/pusher/auth timed out".into()))??;
    v.get("auth")
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| SessionError::Other("missing auth in /pusher/auth response".into()))
}

struct InboxItem {
    source: String,
    source_id: Option<i64>,
    message: Option<String>,
    unread: bool,
}

/// GET /user-inbox-messages?limit=20 → parsed items (newest first).
async fn fetch_inbox(app: &AppHandle) -> Result<Vec<InboxItem>, SessionError> {
    let token = get_session_token(app).ok_or(SessionError::Auth)?;
    let v: Value = tokio::time::timeout(HTTP_TIMEOUT, async {
        let resp = http_client()
            .get(format!("{}/user-inbox-messages?limit=20", api_base_url()))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await
            .map_err(|e| SessionError::Other(e.to_string()))?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN
        {
            return Err(SessionError::Auth);
        }
        if !status.is_success() {
            return Err(SessionError::Other(format!(
                "/user-inbox-messages returned {}",
                status.as_u16()
            )));
        }
        resp.json::<Value>()
            .await
            .map_err(|e| SessionError::Other(e.to_string()))
    })
    .await
    .map_err(|_| SessionError::Other("/user-inbox-messages timed out".into()))??;
    let items = v
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(items
        .iter()
        .map(|it| InboxItem {
            source: it.get("source").and_then(Value::as_str).unwrap_or("").to_string(),
            source_id: parse_id(it.get("source_id")),
            message: it.get("message").and_then(Value::as_str).map(String::from),
            unread: it.get("unread").and_then(Value::as_bool).unwrap_or(false),
        })
        .collect())
}

/// source_id may arrive as a number or a numeric string.
fn parse_id(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    }
}

/// Realtime handler: a live event is an explicit "this just happened" signal,
/// so always notify. Recording it in `seen` keeps the catch-up backstop from
/// re-notifying the same prep on the next (re)subscribe.
async fn handle_prep(
    app: &AppHandle,
    prep_id: i64,
    seen: &mut HashSet<i64>,
) -> Result<(), SessionError> {
    seen.insert(prep_id);
    let message = match fetch_inbox(app).await {
        Ok(items) => items
            .into_iter()
            .find(|m| m.source == MEETING_PREP_SOURCE && m.source_id == Some(prep_id))
            .and_then(|m| m.message),
        Err(SessionError::Auth) => return Err(SessionError::Auth),
        Err(SessionError::Other(e)) => {
            eprintln!("meeting-notifications: inbox fetch failed (using fallback body): {e}");
            None
        }
    };
    let (title, body) = build_notification(message.as_deref());
    show(app, &title, &body, &prep_url(prep_id));
    Ok(())
}

/// Catch-up backstop run on each (re)subscribe. On the first subscribe it only
/// seeds `seen` (no notifications); afterwards it surfaces unread meeting-preps
/// that arrived while we were disconnected/suspended.
async fn catch_up(
    app: &AppHandle,
    seen: &mut HashSet<i64>,
    seed_only: bool,
) -> Result<(), SessionError> {
    let items = match fetch_inbox(app).await {
        Ok(items) => items,
        Err(SessionError::Auth) => return Err(SessionError::Auth),
        Err(SessionError::Other(e)) => {
            eprintln!("meeting-notifications: catch-up inbox fetch failed: {e}");
            return Ok(());
        }
    };
    for item in items {
        if item.source != MEETING_PREP_SOURCE {
            continue;
        }
        let Some(id) = item.source_id else { continue };
        if seed_only {
            seen.insert(id);
        } else if item.unread && seen.insert(id) {
            let (title, body) = build_notification(item.message.as_deref());
            show(app, &title, &body, &prep_url(id));
        }
    }
    Ok(())
}

fn build_notification(message: Option<&str>) -> (String, String) {
    let title = "Meeting prep ready".to_string();
    let body = match message {
        Some(m) => truncate(&strip_markdown(m), 120),
        None => "Your meeting prep is ready.".to_string(),
    };
    (title, body)
}

/// The web deep link a meeting-prep notification opens when clicked.
fn prep_url(prep_id: i64) -> String {
    format!("{}/my/meeting-prep-v2/{prep_id}", web_app_base_url())
}

// ---------------------------------------------------------------------------
// Auto-record confirm/deny prompt
//
// When the mic monitor detects a meeting it asks here for a decision via a
// notification carrying Record / Dismiss buttons. The user has 10 seconds to
// choose; if they don't, the caller's mode default applies (record when
// auto-record is on, skip when it's off). Lives in this module because there
// is a single UNUserNotificationCenter delegate and it must route both the
// meeting-prep clicks and these action buttons.
// ---------------------------------------------------------------------------

/// Identifiers for the auto-record prompt. The category id tells macOS which
/// action buttons to attach to the notification; the action ids come back in
/// the delegate's `didReceive` so we know which button was tapped.
const AUTO_RECORD_CATEGORY: &str = "ai.ariso.auto-record";
const AUTO_RECORD_ACTION_RECORD: &str = "ai.ariso.auto-record.record";
const AUTO_RECORD_ACTION_DISMISS: &str = "ai.ariso.auto-record.dismiss";

/// How long the prompt stays actionable before the caller's mode default wins.
const AUTO_RECORD_PROMPT_TIMEOUT: Duration = Duration::from_secs(10);

/// Identifiers for the silence stop prompt. Mirrors the auto-record category:
/// the category id attaches the Keep / Stop buttons; the action ids come back
/// in the delegate's `didReceive`. The identifier is fixed so we can dismiss it.
const SILENCE_CATEGORY: &str = "ai.ariso.silence-stop";
const SILENCE_ACTION_KEEP: &str = "ai.ariso.silence-stop.keep";
const SILENCE_ACTION_STOP: &str = "ai.ariso.silence-stop.stop";
const SILENCE_IDENTIFIER: &str = "ai.ariso.silence-stop";

/// One-shot sender for the in-flight auto-record prompt. The notification
/// delegate (ObjC callback, main thread) fills it when a button is tapped; the
/// awaiting `prompt_auto_record` task receives the choice. Only one prompt is
/// ever live at a time — the mic monitor stays in its Recording phase until the
/// meeting ends — so a new prompt simply replaces any stale sender.
fn prompt_slot() -> &'static Mutex<Option<oneshot::Sender<bool>>> {
    static SLOT: OnceLock<Mutex<Option<oneshot::Sender<bool>>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// Deliver the user's choice from the notification delegate to the awaiting
/// `prompt_auto_record`. No-op if nothing is waiting (already timed out).
fn deliver_auto_record_decision(record: bool) {
    if let Some(tx) = prompt_slot().lock().unwrap().take() {
        let _ = tx.send(record);
    }
}

/// The app handle, stored at delegate init so the notification delegate (an
/// ObjC callback with no captured state) can emit silence-prompt events back to
/// the frontend, which owns the silence state machine.
fn app_handle_slot() -> &'static std::sync::Mutex<Option<AppHandle>> {
    static SLOT: std::sync::OnceLock<std::sync::Mutex<Option<AppHandle>>> =
        std::sync::OnceLock::new();
    SLOT.get_or_init(|| std::sync::Mutex::new(None))
}

/// Emit the user's silence-prompt choice to the frontend. No-op if no handle
/// is stored (delegate not initialized — i.e. dev/unsigned, where the frontend
/// grace timer auto-stops anyway).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn emit_silence_decision(keep: bool) {
    use tauri::Emitter;
    let event = if keep {
        "silence-prompt://keep"
    } else {
        "silence-prompt://stop"
    };
    if let Some(app) = app_handle_slot().lock().unwrap().as_ref() {
        let _ = app.emit(event, ());
    }
}

/// Whether the meeting happening right now is already flagged for server-side
/// auto-join (an Ariso notetaker will record it), in which case the desktop must
/// not also record. Only the Ariso backend has a calendar; everything else — and
/// any network/parse error — returns false so detection proceeds normally.
pub async fn current_meeting_auto_join_scheduled(app: &AppHandle) -> bool {
    use crate::commands::{active_backend, api_base_url, get_session_token, http_client};
    if active_backend(app) != "ariso" {
        return false;
    }
    let Some(token) = get_session_token(app) else {
        return false;
    };
    let now = chrono::Utc::now();
    let start = (now - chrono::Duration::hours(2)).to_rfc3339();
    let end = (now + chrono::Duration::hours(2)).to_rfc3339();
    let result = tokio::time::timeout(HTTP_TIMEOUT, async {
        http_client()
            .get(format!("{}/meetings", api_base_url()))
            .query(&[("startDate", start.as_str()), ("endDate", end.as_str())])
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await
            .ok()?
            .json::<Value>()
            .await
            .ok()
    })
    .await;
    match result {
        Ok(Some(v)) => current_meeting_auto_join_from(&v, now.timestamp_millis()),
        _ => false,
    }
}

/// Pure selector (testable): given the `/meetings` response and the current time
/// in epoch-ms, return whether the *current* meeting is auto-join-scheduled.
/// "Current" mirrors the frontend's `pickDefaultMeeting`: a meeting is current
/// when `start - 5min <= now <= start + 60min`; if several overlap, the latest
/// start wins (the one you most recently joined).
fn current_meeting_auto_join_from(resp: &Value, now_ms: i64) -> bool {
    const FIVE_MIN_MS: i64 = 5 * 60_000;
    const SIXTY_MIN_MS: i64 = 60 * 60_000;
    let Some(meetings) = resp.get("meetings").and_then(Value::as_array) else {
        return false;
    };
    let mut current_start = i64::MIN;
    let mut current_auto_join = false;
    for m in meetings {
        let Some(start_str) = m.get("start_at").and_then(Value::as_str) else {
            continue;
        };
        let Ok(start_ms) = chrono::DateTime::parse_from_rfc3339(start_str)
            .map(|dt| dt.timestamp_millis())
        else {
            continue;
        };
        let is_current = start_ms - FIVE_MIN_MS <= now_ms && now_ms <= start_ms + SIXTY_MIN_MS;
        if is_current && start_ms >= current_start {
            current_start = start_ms;
            current_auto_join = truthy(m.get("auto_join_scheduled"));
        }
    }
    current_auto_join
}

/// Lenient truthiness for an API flag that may arrive as a bool, 0/1, or string.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_i64().map(|i| i != 0).unwrap_or(false),
        Some(Value::String(s)) => s == "true" || s == "1",
        _ => false,
    }
}

/// Show the auto-record confirm/deny notification and await the user's choice.
/// Resolves `true` to start recording. Returns `default_record` if the user
/// doesn't respond within the 10s window — or on a build where the action
/// buttons aren't available (the plugin fallback can't report a tap).
pub async fn prompt_auto_record(app: &AppHandle, default_record: bool) -> bool {
    let (tx, rx) = oneshot::channel();
    *prompt_slot().lock().unwrap() = Some(tx);
    show_auto_record_prompt(app);
    match tokio::time::timeout(AUTO_RECORD_PROMPT_TIMEOUT, rx).await {
        Ok(Ok(record)) => record,
        // Timed out, or the sender was dropped/replaced — apply the mode
        // default and drop any stale sender we still own.
        _ => {
            let _ = prompt_slot().lock().unwrap().take();
            default_record
        }
    }
}

/// Show the prompt notification with Record/Dismiss buttons. On a signed macOS
/// bundle this uses UNUserNotificationCenter (the only path that renders action
/// buttons). In dev / on other platforms it falls back to a plain banner with
/// no buttons — the prompt then resolves on the timeout default.
fn show_auto_record_prompt(app: &AppHandle) {
    let title = "Meeting detected";
    let body = "Start recording this meeting?";
    #[cfg(target_os = "macos")]
    if !tauri::is_dev() {
        macos_un::show_auto_record(app, title, body);
        return;
    }
    // Dev / non-macOS: plugin banner (no buttons). NOTE: from `tauri dev` on
    // macOS the process isn't a real .app bundle, so this may post nothing
    // visible — the prompt with buttons only renders from the built bundle.
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("auto-record: failed to show prompt notification: {e}");
    }
}

/// Initialize native notification support. On a macOS bundle this installs the
/// UNUserNotificationCenter delegate (which handles clicks) — MUST be called on
/// the main thread. In dev / on other platforms it's a no-op (the plugin path
/// is used instead).
#[allow(unused_variables)]
pub fn init_native(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    if !tauri::is_dev() {
        *app_handle_slot().lock().unwrap() = Some(app.clone());
        macos_un::init();
    }
}

/// Show the notification. On a macOS bundle we use UNUserNotificationCenter
/// directly (the plugin exposes no click handling on desktop) so the delegate
/// can open the deep link on click; the URL rides along as the request id. In
/// dev and on other platforms we fall back to the plugin (no click).
fn show(app: &AppHandle, title: &str, body: &str, url: &str) {
    #[cfg(target_os = "macos")]
    if !tauri::is_dev() {
        macos_un::show(app, title, body, url);
        return;
    }
    let _ = url; // click-to-open is only wired up for the macOS bundle
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("meeting-notifications: failed to show notification: {e}");
    }
}

/// UNUserNotificationCenter integration: a delegate receives the click on the
/// main thread (non-blocking) and opens the deep link carried as the request
/// identifier. UNC requires a properly signed (Developer ID) app; in unsigned
/// builds `addNotificationRequest` errors and we fall back to the plugin.
#[cfg(target_os = "macos")]
mod macos_un {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{define_class, msg_send, AnyThread};
    use objc2_foundation::{NSArray, NSError, NSSet, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationAction,
        UNNotificationActionOptions, UNNotificationCategory, UNNotificationCategoryOptions,
        UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
        UNUserNotificationCenter, UNUserNotificationCenterDelegate,
    };
    use tauri::AppHandle;
    use tauri_plugin_notification::NotificationExt;

    define_class!(
        // SAFETY: superclass NSObject has no subclassing requirements and we
        // don't implement Drop.
        #[unsafe(super(NSObject))]
        #[name = "ArisoNotificationDelegate"]
        struct Delegate;

        unsafe impl NSObjectProtocol for Delegate {}

        unsafe impl UNUserNotificationCenterDelegate for Delegate {
            // Show notifications even when the app is frontmost.
            #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
            fn will_present(
                &self,
                _center: &UNUserNotificationCenter,
                _notification: &UNNotification,
                completion: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
            ) {
                let opts = UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List
                    | UNNotificationPresentationOptions::Sound;
                completion.call((opts,));
            }

            // Auto-record action buttons resolve the pending prompt; otherwise
            // (a meeting-prep body click) open the deep link carried as the
            // request identifier.
            #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
            fn did_receive(
                &self,
                _center: &UNUserNotificationCenter,
                response: &UNNotificationResponse,
                completion: &block2::DynBlock<dyn Fn()>,
            ) {
                let action = response.actionIdentifier().to_string();
                if action == super::SILENCE_ACTION_KEEP {
                    super::emit_silence_decision(true);
                    completion.call(());
                    return;
                }
                if action == super::SILENCE_ACTION_STOP {
                    super::emit_silence_decision(false);
                    completion.call(());
                    return;
                }
                if action == super::AUTO_RECORD_ACTION_RECORD {
                    super::deliver_auto_record_decision(true);
                    completion.call(());
                    return;
                }
                if action == super::AUTO_RECORD_ACTION_DISMISS {
                    super::deliver_auto_record_decision(false);
                    completion.call(());
                    return;
                }
                let url = response.notification().request().identifier().to_string();
                if url.starts_with("http") {
                    let _ = std::process::Command::new("open").arg(&url).spawn();
                }
                completion.call(());
            }
        }
    );

    impl Delegate {
        fn new() -> Retained<Self> {
            unsafe { msg_send![Self::alloc(), init] }
        }
    }

    fn err_desc(err: *mut NSError) -> Option<String> {
        if err.is_null() {
            None
        } else {
            Some(unsafe { &*err }.localizedDescription().to_string())
        }
    }

    /// Install the delegate (retained for the process lifetime — it's a weak
    /// property) and request authorization. Must run on the main thread.
    pub fn init() {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let delegate = Delegate::new();
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        // setDelegate stores a weak reference; leak a strong ref so the
        // delegate lives for the whole process.
        std::mem::forget(delegate);
        register_categories(&center);
        let handler = RcBlock::new(|_granted: Bool, _err: *mut NSError| {});
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
            &handler,
        );
    }

    /// Register every notification category in one call. `setNotificationCategories`
    /// is a wholesale REPLACE (not additive), so both the auto-record and silence
    /// categories must go into a single `NSSet` here or one would clobber the
    /// other.
    fn register_categories(center: &UNUserNotificationCenter) {
        let categories =
            NSSet::from_retained_slice(&[auto_record_category(), silence_category()]);
        center.setNotificationCategories(&categories);
    }

    /// Build the auto-record category so the prompt notification renders its
    /// Record / Dismiss buttons. Actions run in the background (no Foreground
    /// option) — the handler only signals a channel, it doesn't need the app
    /// frontmost. Dismiss is styled destructive.
    fn auto_record_category() -> Retained<UNNotificationCategory> {
        let record = UNNotificationAction::actionWithIdentifier_title_options(
            &NSString::from_str(super::AUTO_RECORD_ACTION_RECORD),
            &NSString::from_str("Record"),
            UNNotificationActionOptions::empty(),
        );
        let dismiss = UNNotificationAction::actionWithIdentifier_title_options(
            &NSString::from_str(super::AUTO_RECORD_ACTION_DISMISS),
            &NSString::from_str("Dismiss"),
            UNNotificationActionOptions::Destructive,
        );
        let actions = NSArray::from_retained_slice(&[record, dismiss]);
        let intents: Retained<NSArray<NSString>> = NSArray::from_slice(&[]);
        UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
            &NSString::from_str(super::AUTO_RECORD_CATEGORY),
            &actions,
            &intents,
            UNNotificationCategoryOptions::empty(),
        )
    }

    /// Build the silence category so the prompt renders Keep / Stop buttons. Like
    /// auto-record, actions run in the background (no Foreground option); Stop is
    /// styled destructive.
    fn silence_category() -> Retained<UNNotificationCategory> {
        let keep = UNNotificationAction::actionWithIdentifier_title_options(
            &NSString::from_str(super::SILENCE_ACTION_KEEP),
            &NSString::from_str("Keep recording"),
            UNNotificationActionOptions::empty(),
        );
        let stop = UNNotificationAction::actionWithIdentifier_title_options(
            &NSString::from_str(super::SILENCE_ACTION_STOP),
            &NSString::from_str("Stop now"),
            UNNotificationActionOptions::Destructive,
        );
        let actions = NSArray::from_retained_slice(&[keep, stop]);
        let intents: Retained<NSArray<NSString>> = NSArray::from_slice(&[]);
        UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
            &NSString::from_str(super::SILENCE_CATEGORY),
            &actions,
            &intents,
            UNNotificationCategoryOptions::empty(),
        )
    }

    /// Show the silence prompt with Keep / Stop buttons under a fixed identifier
    /// so `dismiss_silence` can remove it. Falls back to a button-less banner on
    /// UNC failure (unsigned build).
    pub fn show_silence(app: &AppHandle, title: &str, body: &str) {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        content.setCategoryIdentifier(&NSString::from_str(super::SILENCE_CATEGORY));
        let identifier = NSString::from_str(super::SILENCE_IDENTIFIER);
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &identifier, &content, None,
        );
        let app = app.clone();
        let title = title.to_string();
        let body = body.to_string();
        let handler = RcBlock::new(move |err: *mut NSError| {
            if let Some(desc) = err_desc(err) {
                eprintln!("silence-prompt: UNC unavailable ({desc}); using plugin fallback (no buttons)");
                let _ = app.notification().builder().title(&title).body(&body).show();
            }
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&handler));
    }

    /// Remove the silence prompt (delivered and/or pending) by its fixed identifier.
    pub fn dismiss_silence() {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let ids: Retained<NSArray<NSString>> =
            NSArray::from_retained_slice(&[NSString::from_str(super::SILENCE_IDENTIFIER)]);
        center.removeDeliveredNotificationsWithIdentifiers(&ids);
        center.removePendingNotificationRequestsWithIdentifiers(&ids);
    }

    /// Show the auto-record prompt with action buttons. Mirrors `show` but tags
    /// the content with the auto-record category so macOS attaches the buttons;
    /// on UNC failure it falls back to a (button-less) plugin banner.
    pub fn show_auto_record(app: &AppHandle, title: &str, body: &str) {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        content.setCategoryIdentifier(&NSString::from_str(super::AUTO_RECORD_CATEGORY));
        // A non-http identifier so the body-click handler's URL guard skips it.
        let identifier = NSString::from_str(super::AUTO_RECORD_CATEGORY);
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &identifier, &content, None,
        );
        let app = app.clone();
        let title = title.to_string();
        let body = body.to_string();
        let handler = RcBlock::new(move |err: *mut NSError| {
            if let Some(desc) = err_desc(err) {
                eprintln!("auto-record: UNC unavailable ({desc}); using plugin fallback (no buttons)");
                let _ = app.notification().builder().title(&title).body(&body).show();
            }
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&handler));
    }

    pub fn show(app: &AppHandle, title: &str, body: &str, url: &str) {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        let identifier = NSString::from_str(url);
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &identifier,
            &content,
            None,
        );
        let app = app.clone();
        let title = title.to_string();
        let body = body.to_string();
        let handler = RcBlock::new(move |err: *mut NSError| {
            if let Some(desc) = err_desc(err) {
                // UNUserNotificationCenter rejects unsigned/ad-hoc builds
                // (UNErrorDomain 1). Fall back to the plugin so the notification
                // still displays — click-to-open is unavailable on that path but
                // works in a properly Developer-ID-signed build.
                eprintln!("meeting-notifications: UNC unavailable ({desc}); using plugin fallback");
                let _ = app
                    .notification()
                    .builder()
                    .title(&title)
                    .body(&body)
                    .show();
            }
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&handler));
    }
}

/// Strip common markdown to plain text and collapse whitespace (mirrors the
/// former JS `stripMarkdown`).
fn strip_markdown(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut prev_space = false;
    for ch in md.chars() {
        let c = if matches!(ch, '#' | '>' | '*' | '_' | '`' | '~' | '[' | ']') {
            continue;
        } else if ch.is_whitespace() {
            ' '
        } else {
            ch
        };
        if c == ' ' {
            if prev_space {
                continue;
            }
            prev_space = true;
        } else {
            prev_space = false;
        }
        out.push(c);
    }
    out.trim().to_string()
}

fn truncate(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_string();
    }
    let mut s: String = chars[..max.saturating_sub(1)].iter().collect();
    s = s.trim_end().to_string();
    s.push('…');
    s
}

#[tauri::command]
pub async fn sync_meeting_notifications(app: AppHandle) -> Result<(), String> {
    sync(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn stop_meeting_notifications(app: AppHandle) -> Result<(), String> {
    stop(&app);
    Ok(())
}

/// Show the silence stop prompt (Keep / Stop). Native buttons on a signed macOS
/// bundle; button-less banner otherwise (the frontend grace timer still stops).
#[tauri::command]
pub fn show_silence_prompt(app: AppHandle) {
    let title = "Still recording";
    let body = "No audio for 10 minutes. Keep recording or stop?";
    #[cfg(target_os = "macos")]
    if !tauri::is_dev() {
        macos_un::show_silence(&app, title, body);
        return;
    }
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("silence-prompt: failed to show notification: {e}");
    }
}

/// Clear a shown silence prompt (audio resumed, paused, or recording ended).
#[tauri::command]
pub fn dismiss_silence_prompt(app: AppHandle) {
    let _ = &app;
    #[cfg(target_os = "macos")]
    if !tauri::is_dev() {
        macos_un::dismiss_silence();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // 2026-06-16T12:00:00Z in epoch-ms, used as "now" across the cases.
    const NOW_MS: i64 = 1_781_956_800_000;

    fn at(offset_min: i64) -> String {
        let dt = chrono::DateTime::from_timestamp_millis(NOW_MS + offset_min * 60_000).unwrap();
        dt.to_rfc3339()
    }

    #[test]
    fn skips_when_current_meeting_is_auto_join_scheduled() {
        // Started 10 min ago (current), flagged for server-side auto-join.
        let resp = json!({ "meetings": [
            { "id": 1, "start_at": at(-10), "auto_join_scheduled": true }
        ]});
        assert!(current_meeting_auto_join_from(&resp, NOW_MS));
    }

    #[test]
    fn does_not_skip_when_current_meeting_is_not_flagged() {
        let resp = json!({ "meetings": [
            { "id": 1, "start_at": at(-10), "auto_join_scheduled": false }
        ]});
        assert!(!current_meeting_auto_join_from(&resp, NOW_MS));
    }

    #[test]
    fn flag_on_a_non_current_meeting_is_ignored() {
        // Started 2h ago (well past the +60min current window) — not current.
        let resp = json!({ "meetings": [
            { "id": 1, "start_at": at(-120), "auto_join_scheduled": true }
        ]});
        assert!(!current_meeting_auto_join_from(&resp, NOW_MS));
    }

    #[test]
    fn latest_starting_current_meeting_wins_on_overlap() {
        // Two overlapping current meetings; the later-starting one (not flagged)
        // is the active one, mirroring pickDefaultMeeting's tie-break.
        let resp = json!({ "meetings": [
            { "id": 1, "start_at": at(-50), "auto_join_scheduled": true },
            { "id": 2, "start_at": at(-2),  "auto_join_scheduled": false }
        ]});
        assert!(!current_meeting_auto_join_from(&resp, NOW_MS));
    }

    #[test]
    fn within_five_minute_lead_in_counts_as_current() {
        // Starts in 3 min — inside the 5-min lead-in, so it's already current.
        let resp = json!({ "meetings": [
            { "id": 1, "start_at": at(3), "auto_join_scheduled": true }
        ]});
        assert!(current_meeting_auto_join_from(&resp, NOW_MS));
    }

    #[test]
    fn truthy_accepts_bool_number_and_string_forms() {
        assert!(truthy(Some(&json!(true))));
        assert!(truthy(Some(&json!(1))));
        assert!(truthy(Some(&json!("true"))));
        assert!(!truthy(Some(&json!(false))));
        assert!(!truthy(Some(&json!(0))));
        assert!(!truthy(None));
    }

    #[test]
    fn missing_or_empty_meetings_does_not_skip() {
        assert!(!current_meeting_auto_join_from(&json!({}), NOW_MS));
        assert!(!current_meeting_auto_join_from(&json!({ "meetings": [] }), NOW_MS));
    }
}
