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
use std::sync::Mutex;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_store::StoreExt;
use tokio_tungstenite::tungstenite::Message;

use crate::commands::{
    clear_session_token, get_session_token, http_client, API_BASE_URL, PUSHER_CLUSTER, PUSHER_KEY,
    WEB_APP_BASE_URL,
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
            .get(format!("{API_BASE_URL}/auth/me"))
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
            .post(format!("{API_BASE_URL}/pusher/auth"))
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
            .get(format!("{API_BASE_URL}/user-inbox-messages?limit=20"))
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
    format!("{WEB_APP_BASE_URL}/my/meeting-prep-v2/{prep_id}")
}

/// Initialize native notification support. On a macOS bundle this installs the
/// UNUserNotificationCenter delegate (which handles clicks) — MUST be called on
/// the main thread. In dev / on other platforms it's a no-op (the plugin path
/// is used instead).
#[allow(unused_variables)]
pub fn init_native(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    if !tauri::is_dev() {
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
    use objc2_foundation::{NSError, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
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

            // On click, open the deep link carried as the request identifier.
            #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
            fn did_receive(
                &self,
                _center: &UNUserNotificationCenter,
                response: &UNNotificationResponse,
                completion: &block2::DynBlock<dyn Fn()>,
            ) {
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
        let handler = RcBlock::new(|_granted: Bool, _err: *mut NSError| {});
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
            &handler,
        );
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
