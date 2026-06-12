//! Next-meeting tray orchestrator (native).
//!
//! Fetches today's scheduled meetings from the Ariso API, features the next
//! strictly-upcoming one in the menu-bar tray (countdown title + menu rows),
//! and promotes the following meeting once it starts. Lives in the Rust
//! process for the same reason as `meeting_notifications`: macOS suspends
//! hidden webviews, so a webview timer would freeze in the background.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Local, SecondsFormat, Utc};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::commands::{api_base_url, clear_session_token, get_session_token, http_client};

/// A meeting from `GET /meetings` (list payload carries no end time).
#[derive(Clone, Debug, PartialEq)]
pub struct ScheduledMeeting {
    pub id: i64,
    pub title: Option<String>,
    pub start_at: DateTime<Utc>,
}

/// The meeting currently surfaced in the tray. `end_at` is resolved
/// separately via `GET /meeting-notes/:id` and may be absent.
#[derive(Clone, Debug, PartialEq)]
pub struct FeaturedMeeting {
    pub id: i64,
    pub title: Option<String>,
    pub start_at: DateTime<Utc>,
    pub end_at: Option<DateTime<Utc>>,
}

/// Holds the handle to the running orchestrator task (if any).
#[derive(Default)]
pub struct TrayMeetingManager {
    handle: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

impl TrayMeetingManager {
    pub fn new() -> Self {
        Self::default()
    }
}

/// The currently featured meeting, readable by the tray menu builder and the
/// `record_featured` menu-event arm (both need it from `'static` contexts).
#[derive(Default)]
pub struct FeaturedMeetingState(pub Mutex<Option<FeaturedMeeting>>);

impl FeaturedMeetingState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Per-HTTP-request cap covering `.send()` and body decode (the shared
/// `http_client` has no request timeout — same convention as
/// `meeting_notifications`).
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

/// Countdown redraw cadence.
const TICK: Duration = Duration::from_secs(60);

/// Full server re-fetch happens every N ticks (5 minutes).
const FETCH_EVERY_TICKS: u32 = 5;

/// `Auth` means the server rejected the stored session (401/403) — the
/// orchestrator tears down instead of retrying (mirrors meeting_notifications).
enum FetchError {
    Auth,
    Other(String),
}

/// Start the orchestrator iff the active backend is Ariso and a session token
/// is present; otherwise stop it. Safe to call repeatedly (sign-in/out,
/// backend change). The Local backend has no scheduled meetings, so it gets
/// no tray title and the unchanged idle menu.
pub fn sync(app: &AppHandle) {
    let desired =
        crate::commands::active_backend(app) == "ariso" && get_session_token(app).is_some();
    if !desired {
        stop(app);
        return;
    }
    let mgr = app.state::<TrayMeetingManager>();
    let mut guard = mgr.handle.lock().unwrap();
    if guard.is_some() {
        return;
    }
    let app = app.clone();
    *guard = Some(tauri::async_runtime::spawn(async move {
        run_loop(app).await;
    }));
}

/// Stop the orchestrator, clear the featured meeting, and redraw the tray
/// (title cleared, idle menu without meeting rows).
pub fn stop(app: &AppHandle) {
    let mgr = app.state::<TrayMeetingManager>();
    let mut guard = mgr.handle.lock().unwrap();
    if let Some(handle) = guard.take() {
        handle.abort();
    }
    drop(guard);
    *app.state::<FeaturedMeetingState>().0.lock().unwrap() = None;
    refresh_tray(app, true);
}

/// Redraw the tray on the main thread (muda menus are main-thread on macOS).
/// While recording, the recording menu owns the tray — only the title is
/// refreshed (which clears it). `rebuild_menu` skips a full idle-menu rebuild
/// on countdown-only ticks so an open tray menu isn't yanked shut.
fn refresh_tray(app: &AppHandle, rebuild_menu: bool) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        let recording = app
            .state::<crate::recording_state::RecordingState>()
            .is_active();
        if rebuild_menu && !recording {
            crate::tray::set_menu(&app, false, false);
        } else {
            crate::tray::refresh_tray_title(&app);
        }
    });
}

/// Fetch → pick → resolve end_at → render, then tick every 60s (countdown
/// redraw + promote-on-start) and re-fetch every 5min. Exits only on an auth
/// rejection; transient errors keep the last known state and retry.
async fn run_loop(app: AppHandle) {
    let mut meetings: Vec<ScheduledMeeting> = Vec::new();
    let mut end_cache: HashMap<i64, Option<DateTime<Utc>>> = HashMap::new();
    let mut ticks_until_fetch = 0u32;
    let mut interval = tokio::time::interval(TICK);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        interval.tick().await;

        if ticks_until_fetch == 0 {
            ticks_until_fetch = FETCH_EVERY_TICKS;
            match fetch_today_meetings(&app).await {
                Ok(list) => meetings = list,
                Err(FetchError::Auth) => break,
                Err(FetchError::Other(e)) => {
                    eprintln!("tray-meeting: list fetch failed: {e}");
                }
            }
        }
        ticks_until_fetch -= 1;

        let now = Utc::now();
        let featured = match pick_next_upcoming(&meetings, now) {
            Some(m) => {
                let end_at = match end_cache.get(&m.id) {
                    Some(cached) => *cached,
                    None => {
                        let end = match fetch_end_at(&app, m.id).await {
                            Ok(end) => end,
                            Err(FetchError::Auth) => break,
                            Err(FetchError::Other(e)) => {
                                eprintln!(
                                    "tray-meeting: end_at fetch failed (start-only display): {e}"
                                );
                                None
                            }
                        };
                        end_cache.insert(m.id, end);
                        end
                    }
                };
                Some(FeaturedMeeting {
                    id: m.id,
                    title: m.title.clone(),
                    start_at: m.start_at,
                    end_at,
                })
            }
            None => None,
        };

        let menu_changed = {
            let state = app.state::<FeaturedMeetingState>();
            let mut guard = state.0.lock().unwrap();
            let changed = *guard != featured;
            *guard = featured;
            changed
        };
        refresh_tray(&app, menu_changed);
    }

    eprintln!("tray-meeting: session invalid; clearing token and stopping orchestrator");
    let _ = clear_session_token(&app);
    {
        let mgr = app.state::<TrayMeetingManager>();
        *mgr.handle.lock().unwrap() = None;
    }
    *app.state::<FeaturedMeetingState>().0.lock().unwrap() = None;
    refresh_tray(&app, true);
}

/// GET /meetings?startDate&endDate (local-day bounds) → parsed list.
async fn fetch_today_meetings(app: &AppHandle) -> Result<Vec<ScheduledMeeting>, FetchError> {
    let token = get_session_token(app).ok_or(FetchError::Auth)?;
    let (start, end) = day_bounds(Local::now());
    let v: Value = tokio::time::timeout(HTTP_TIMEOUT, async {
        let resp = http_client()
            .get(format!("{}/meetings", api_base_url()))
            .query(&[
                ("startDate", start.to_rfc3339_opts(SecondsFormat::Millis, true)),
                ("endDate", end.to_rfc3339_opts(SecondsFormat::Millis, true)),
            ])
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await
            .map_err(|e| FetchError::Other(e.to_string()))?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN
        {
            return Err(FetchError::Auth);
        }
        if !status.is_success() {
            return Err(FetchError::Other(format!(
                "/meetings returned {}",
                status.as_u16()
            )));
        }
        resp.json::<Value>()
            .await
            .map_err(|e| FetchError::Other(e.to_string()))
    })
    .await
    .map_err(|_| FetchError::Other("/meetings timed out".into()))??;
    Ok(parse_meetings(&v))
}

/// GET /meeting-notes/:id → optional end_at.
async fn fetch_end_at(
    app: &AppHandle,
    meeting_id: i64,
) -> Result<Option<DateTime<Utc>>, FetchError> {
    let token = get_session_token(app).ok_or(FetchError::Auth)?;
    let v: Value = tokio::time::timeout(HTTP_TIMEOUT, async {
        let resp = http_client()
            .get(format!("{}/meeting-notes/{meeting_id}", api_base_url()))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await
            .map_err(|e| FetchError::Other(e.to_string()))?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN
        {
            return Err(FetchError::Auth);
        }
        if !status.is_success() {
            return Err(FetchError::Other(format!(
                "/meeting-notes/{meeting_id} returned {}",
                status.as_u16()
            )));
        }
        resp.json::<Value>()
            .await
            .map_err(|e| FetchError::Other(e.to_string()))
    })
    .await
    .map_err(|_| FetchError::Other(format!("/meeting-notes/{meeting_id} timed out")))??;
    Ok(parse_end_at(&v))
}

/// Re-evaluate the orchestrator's desired state. Invoked by the bootstrap
/// window on launch and on every SYNC_EVENT broadcast (sign-in/out, backend
/// change) — same pattern as `sync_meeting_notifications`.
#[tauri::command]
pub async fn sync_tray_meeting(app: AppHandle) -> Result<(), String> {
    sync(&app);
    Ok(())
}

/// Max characters of the meeting title shown in the menu-bar (tray) title.
const TITLE_MAX_CHARS: usize = 10;

/// Truncate a meeting title for the tray title bar. `None`/blank titles
/// become "Untitled meeting"; titles longer than 10 Unicode scalar values
/// are cut to 10 + `…`.
pub(crate) fn truncate_title(title: Option<&str>) -> String {
    let Some(s) = title.filter(|s| !s.trim().is_empty()) else {
        return "Untitled meeting".to_string();
    };
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= TITLE_MAX_CHARS {
        return s.to_string();
    }
    let mut out: String = chars[..TITLE_MAX_CHARS].iter().collect();
    out.push('…');
    out
}

/// Relative countdown to a future start time: `in 12min`, or `in <1min`
/// under a minute. Callers only pass strictly-upcoming meetings; a stale
/// (already-started) one reads `in <1min` until the next tick drops it.
pub(crate) fn format_countdown(start: DateTime<Utc>, now: DateTime<Utc>) -> String {
    // num_minutes truncates toward zero == floor for positive durations.
    let mins = (start - now).num_minutes();
    if mins >= 1 {
        format!("in {mins}min")
    } else {
        "in <1min".to_string()
    }
}

/// The full menu-bar string next to the tray icon, e.g. `Weekly Eng… in 12min`.
pub(crate) fn format_title_bar(
    title: Option<&str>,
    start: DateTime<Utc>,
    now: DateTime<Utc>,
) -> String {
    format!("{} {}", truncate_title(title), format_countdown(start, now))
}

/// Gray time row under the menu title row: `10:00 – 10:30 AM` (or `10:00 AM`
/// when the end time is unknown). Takes already-localized datetimes; the
/// caller converts with `.with_timezone(&chrono::Local)`. Generic over the
/// timezone so tests can pin a `FixedOffset`.
pub(crate) fn format_time_range<Tz: chrono::TimeZone>(
    start: DateTime<Tz>,
    end: Option<DateTime<Tz>>,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    match end {
        Some(end) if start.format("%p").to_string() == end.format("%p").to_string() => {
            format!("{} – {}", start.format("%-I:%M"), end.format("%-I:%M %p"))
        }
        Some(end) => format!(
            "{} – {}",
            start.format("%-I:%M %p"),
            end.format("%-I:%M %p")
        ),
        None => start.format("%-I:%M %p").to_string(),
    }
}

/// The soonest meeting with `start_at` strictly after `now`. Computes the min
/// explicitly so it is order-independent (mirrors the TS `pickDefaultMeeting`
/// `next` arm; the "current meeting" arm is intentionally absent — only
/// strictly-upcoming meetings are featured in the tray).
pub(crate) fn pick_next_upcoming(
    meetings: &[ScheduledMeeting],
    now: DateTime<Utc>,
) -> Option<&ScheduledMeeting> {
    meetings
        .iter()
        .filter(|m| m.start_at > now)
        .min_by_key(|m| m.start_at)
}

/// Parse the `GET /meetings` payload `{ "meetings": [{ id, title, start_at }] }`.
/// Entries with a missing/unparseable id or start_at are skipped.
pub(crate) fn parse_meetings(v: &Value) -> Vec<ScheduledMeeting> {
    let items = v
        .get("meetings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    items
        .iter()
        .filter_map(|it| {
            Some(ScheduledMeeting {
                id: parse_id(it.get("id"))?,
                title: it.get("title").and_then(Value::as_str).map(String::from),
                start_at: parse_datetime(it.get("start_at"))?,
            })
        })
        .collect()
}

/// Read the optional `end_at` from a `GET /meeting-notes/:id` payload.
pub(crate) fn parse_end_at(v: &Value) -> Option<DateTime<Utc>> {
    parse_datetime(v.get("end_at"))
}

/// id may arrive as a JSON number or a numeric string.
fn parse_id(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    }
}

fn parse_datetime(v: Option<&Value>) -> Option<DateTime<Utc>> {
    v.and_then(Value::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

/// The fetch window for "today": [00:00:00.000, 23:59:59.999] in `now`'s
/// timezone, returned in UTC for the API query. Generic over the timezone so
/// tests can pin a `FixedOffset` (production passes `chrono::Local::now()`).
fn day_bounds<Tz: chrono::TimeZone>(now: DateTime<Tz>) -> (DateTime<Utc>, DateTime<Utc>) {
    let tz = now.timezone();
    let date = now.date_naive();
    let start_naive = date.and_hms_opt(0, 0, 0).expect("midnight is valid");
    let end_naive = date
        .and_hms_milli_opt(23, 59, 59, 999)
        .expect("end of day is valid");
    // earliest/latest resolve DST folds; fall back to `now` on the
    // pathological edge where the local wall-clock instant doesn't exist.
    let start = tz
        .from_local_datetime(&start_naive)
        .earliest()
        .unwrap_or_else(|| now.clone());
    let end = tz
        .from_local_datetime(&end_naive)
        .latest()
        .unwrap_or_else(|| now.clone());
    (start.with_timezone(&Utc), end.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_title_none_is_untitled() {
        assert_eq!(truncate_title(None), "Untitled meeting");
    }

    #[test]
    fn truncate_title_blank_is_untitled() {
        assert_eq!(truncate_title(Some("")), "Untitled meeting");
        assert_eq!(truncate_title(Some("   ")), "Untitled meeting");
    }

    #[test]
    fn truncate_title_short_unchanged() {
        assert_eq!(truncate_title(Some("Standup")), "Standup");
        assert_eq!(truncate_title(Some("Exactly10!")), "Exactly10!");
    }

    #[test]
    fn truncate_title_long_truncated_to_10_plus_ellipsis() {
        assert_eq!(truncate_title(Some("Weekly Engineering Sync")), "Weekly Eng…");
    }

    #[test]
    fn truncate_title_counts_unicode_scalars_not_bytes() {
        assert_eq!(truncate_title(Some("héllo wörld plus")), "héllo wörl…");
    }

    /// Parse an RFC 3339 timestamp into UTC for test fixtures.
    fn t(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn countdown_whole_minutes() {
        assert_eq!(
            format_countdown(t("2026-06-11T10:12:00Z"), t("2026-06-11T10:00:00Z")),
            "in 12min"
        );
    }

    #[test]
    fn countdown_floors_partial_minutes() {
        assert_eq!(
            format_countdown(t("2026-06-11T10:12:30Z"), t("2026-06-11T10:00:00Z")),
            "in 12min"
        );
    }

    #[test]
    fn countdown_exactly_one_minute() {
        assert_eq!(
            format_countdown(t("2026-06-11T10:01:00Z"), t("2026-06-11T10:00:00Z")),
            "in 1min"
        );
    }

    #[test]
    fn countdown_under_a_minute() {
        assert_eq!(
            format_countdown(t("2026-06-11T10:00:59Z"), t("2026-06-11T10:00:00Z")),
            "in <1min"
        );
    }

    #[test]
    fn title_bar_composes_truncated_title_and_countdown() {
        assert_eq!(
            format_title_bar(
                Some("Weekly Engineering Sync"),
                t("2026-06-11T10:12:00Z"),
                t("2026-06-11T10:00:00Z")
            ),
            "Weekly Eng… in 12min"
        );
    }

    #[test]
    fn title_bar_untitled_meeting() {
        assert_eq!(
            format_title_bar(None, t("2026-06-11T10:00:30Z"), t("2026-06-11T10:00:00Z")),
            "Untitled meeting in <1min"
        );
    }

    use chrono::{FixedOffset, TimeZone};

    /// Fixed UTC-4 zone so these tests don't depend on the machine timezone.
    fn tz() -> FixedOffset {
        FixedOffset::west_opt(4 * 3600).unwrap()
    }

    fn at(h: u32, m: u32) -> DateTime<FixedOffset> {
        tz().with_ymd_and_hms(2026, 6, 11, h, m, 0).unwrap()
    }

    #[test]
    fn time_range_same_meridiem_shares_suffix() {
        assert_eq!(format_time_range(at(10, 0), Some(at(10, 30))), "10:00 – 10:30 AM");
    }

    #[test]
    fn time_range_cross_meridiem_shows_both() {
        assert_eq!(format_time_range(at(11, 30), Some(at(12, 15))), "11:30 AM – 12:15 PM");
    }

    #[test]
    fn time_range_start_only() {
        assert_eq!(format_time_range(at(10, 0), None), "10:00 AM");
    }

    fn meeting(id: i64, start: &str) -> ScheduledMeeting {
        ScheduledMeeting { id, title: None, start_at: t(start) }
    }

    #[test]
    fn pick_empty_list_is_none() {
        assert!(pick_next_upcoming(&[], t("2026-06-11T10:00:00Z")).is_none());
    }

    #[test]
    fn pick_all_past_is_none() {
        let ms = [meeting(1, "2026-06-11T08:00:00Z"), meeting(2, "2026-06-11T09:00:00Z")];
        assert!(pick_next_upcoming(&ms, t("2026-06-11T10:00:00Z")).is_none());
    }

    #[test]
    fn pick_soonest_strictly_future_order_independent() {
        let ms = [
            meeting(1, "2026-06-11T15:00:00Z"),
            meeting(2, "2026-06-11T11:00:00Z"),
            meeting(3, "2026-06-11T08:00:00Z"),
        ];
        assert_eq!(pick_next_upcoming(&ms, t("2026-06-11T10:00:00Z")).unwrap().id, 2);
    }

    #[test]
    fn pick_excludes_meeting_starting_exactly_now() {
        let ms = [meeting(1, "2026-06-11T10:00:00Z"), meeting(2, "2026-06-11T11:00:00Z")];
        assert_eq!(pick_next_upcoming(&ms, t("2026-06-11T10:00:00Z")).unwrap().id, 2);
    }

    #[test]
    fn parse_meetings_reads_payload_and_skips_bad_entries() {
        let v = serde_json::json!({ "meetings": [
            { "id": 7, "title": "Standup", "start_at": "2026-06-11T14:00:00.000Z" },
            { "id": "8", "title": null, "start_at": "2026-06-11T15:00:00Z" },
            { "id": 9, "title": "Bad date", "start_at": "not-a-date" },
            { "title": "No id", "start_at": "2026-06-11T16:00:00Z" }
        ]});
        let ms = parse_meetings(&v);
        assert_eq!(ms.len(), 2);
        assert_eq!(
            ms[0],
            ScheduledMeeting {
                id: 7,
                title: Some("Standup".to_string()),
                start_at: t("2026-06-11T14:00:00Z"),
            }
        );
        assert_eq!(ms[1].id, 8);
        assert_eq!(ms[1].title, None);
    }

    #[test]
    fn parse_meetings_tolerates_missing_array() {
        assert!(parse_meetings(&serde_json::json!({})).is_empty());
        assert!(parse_meetings(&serde_json::Value::Null).is_empty());
    }

    #[test]
    fn parse_end_at_reads_optional_field() {
        let v = serde_json::json!({ "id": 7, "end_at": "2026-06-11T14:30:00Z" });
        assert_eq!(parse_end_at(&v), Some(t("2026-06-11T14:30:00Z")));
        assert_eq!(parse_end_at(&serde_json::json!({ "id": 7 })), None);
        assert_eq!(parse_end_at(&serde_json::json!({ "end_at": "garbage" })), None);
    }

    #[test]
    fn day_bounds_cover_the_local_day_in_utc() {
        // 13:45 local in UTC-4 → local day is [04:00:00Z, next-day 03:59:59.999Z].
        let now = tz().with_ymd_and_hms(2026, 6, 11, 13, 45, 0).unwrap();
        let (start, end) = day_bounds(now);
        assert_eq!(start, t("2026-06-11T04:00:00Z"));
        assert_eq!(end, t("2026-06-12T03:59:59.999Z"));
    }
}
