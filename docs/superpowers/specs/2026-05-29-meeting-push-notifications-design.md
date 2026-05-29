# Meeting Push Notifications — Design

**Date:** 2026-05-29
**App:** `@ariso-ai/desktop` (Tauri v2 + Vue 3)
**Status:** Approved for planning

## Summary

Add native OS push notifications to the desktop app for **meeting-related**
inbox events only: **meeting prep** and **meeting notes**. The desktop app polls
the existing backend inbox API, filters for meeting message sources, and fires
native OS notifications for newly-arrived messages. Clicking a notification
opens the corresponding page in the user's web app (best-effort).

This is scoped strictly to meeting prep + meeting notes. Other inbox sources
(`workflow`, `reminder`, `coaching_nudge`, `greeting`, etc.) are ignored.

## Background

The web app (`ariso-ai/agents` → `apps/web-ui`) has a full notifications
center backed by `/user-inbox-messages`. Relevant facts learned from that code:

- **Endpoint:** `GET /user-inbox-messages?page&limit` →
  `{ items, total, pagination: { page, limit, totalPages } }`.
  There is **no server-side filtering by source** — clients filter themselves.
- **Message shape** (`InboxMessage`): `{ id: number, source: string,
  source_id: number | null, message: string | null, created_at, updated_at,
  unread: boolean }`. `message` is markdown, decrypted server-side per user.
- **Meeting-related `source` values:** `meeting_prep`, `daily_meeting_prep`,
  `meeting_notes`.
- **Web routes** (from `UserNotificationRow.resolveLink`):
  - `meeting_notes` + `source_id` → `/meeting-notes/{source_id}`
  - `meeting_prep` + `source_id` → `/my/meeting-prep-v2/{source_id}`
  - `daily_meeting_prep` → `/my/meetings` (fallback, no id-specific route)

This desktop app:

- Routes all backend calls through a Rust `api_request` command
  (`src/tauri.ts`), which attaches the session token. The same session resolves
  to the user's `orgUserMapping` server-side, so `/user-inbox-messages` is
  reachable with no extra auth work. **(Assumption — verify during build that
  the desktop session can read the inbox endpoint.)**
- Already lists `@tauri-apps/plugin-notification` in `package.json`, but it is
  **completely unwired**: not in `Cargo.toml`, not registered in `main.rs`, not
  in `capabilities/default.json`.
- Has a hidden, always-alive **"main" window** (route `/#/`, currently an empty
  inline component) — the natural home for a background poller, mirroring the
  existing Rust update-scheduler loop in `main.rs`.
- Uses `@tauri-apps/plugin-store` (`load('settings.json', { autoSave: true })`)
  for persistence and `auth.checkSession()` to gate on login.
- Has **no JS test runner** (no vitest; the workspace is excluded from the
  monorepo turbo `build`/`lint`/`test` pipeline).

## Goals

- Fire a native OS notification when a new **meeting prep** or **meeting notes**
  inbox message arrives.
- Never re-notify for the same message (across polls and across app restarts).
- Never flood the user with historical messages on first run / fresh install.
- Let the user disable meeting notifications from Settings.
- Clicking a notification opens the relevant meeting page in the web app
  (best-effort).

## Non-Goals

- No in-app notifications center / list UI (OS toasts only).
- No marking messages read or deleting them — the desktop is **read-only** to
  the inbox; the web app owns read/delete state.
- No support for non-meeting sources.
- No real server push (SSE/WebSocket/web-push) — polling only.

## Architecture

Approach: **JS poller in the always-alive main window** (chosen over a Rust
poller, which would duplicate session/markdown logic for no gain, and over real
server push, which needs backend infra we don't own here).

### Data flow

```
BootstrapView (main window, /#/)
   └─ starts useMeetingNotifications() singleton
        └─ every 60s, if signed in & enabled:
             GET /user-inbox-messages?limit=20   (via api.request)
               → filter source ∈ {meeting_prep, daily_meeting_prep, meeting_notes}
               → selectMessagesToNotify(meetingMsgs, watermark)   [pure fn]
               → for each selected: fire OS notification
               → persist watermark = max(id) to notifications.json store
```

### Components / files

| File | Change |
|---|---|
| `src-tauri/Cargo.toml` | Add `tauri-plugin-notification = "2"`. |
| `src-tauri/src/main.rs` | Register `.plugin(tauri_plugin_notification::init())`. Add `WEB_APP_BASE_URL` const (feature-gated, mirrors `API_BASE_URL`) + a `get_web_app_base_url` command. |
| `src-tauri/src/commands.rs` | Add `WEB_APP_BASE_URL` const + `get_web_app_base_url` command (returns the baked web base URL to JS). |
| `src-tauri/capabilities/default.json` | Add `"notification:default"` permission. |
| `src/composables/useInbox.ts` | `InboxMessage` type; `MEETING_SOURCES` const; `listInboxMessages(limit)`; pure `selectMessagesToNotify(messages, watermark)`; `notificationFor(msg)` (title + body + deep-link URL). |
| `src/composables/useMeetingNotifications.ts` | Singleton poller: start/stop, permission request, store watermark, fire toasts, gate on auth + enabled setting. |
| `src/views/BootstrapView.vue` | New view replacing the empty inline `/#/` component; starts the poller `onMounted`, stops `onUnmounted`. |
| `src/main.ts` | Wire `BootstrapView` into the `/` (Bootstrap) route. |
| `src/views/SettingsView.vue` | New "Notifications" section with a "Meeting notifications" checkbox (persisted in `settings.json`, default ON), toggling the poller. |
| `src/tauri.ts` | Add `getWebAppBaseUrl()` wrapper for the new command. |
| `vitest` setup | Add `vitest` devDependency, a `test` script, minimal config; one spec for `selectMessagesToNotify` + `notificationFor`. |

### Notification selection (the only non-trivial logic)

`selectMessagesToNotify(messages, watermark)` is a **pure function**:

- Input: the meeting-filtered messages from the current poll, and the persisted
  `watermark` (highest `id` already handled, or `null` if never run).
- **First run (`watermark === null`):** return `[]` and signal the caller to
  set the watermark to the current max meeting id — i.e. baseline silently, do
  not notify for pre-existing messages.
- **Subsequent runs:** return messages with `id > watermark`, sorted ascending
  by `id`.
- Caller updates `watermark = max(existing watermark, max id of meeting msgs
  seen this poll)` after firing, so deletions/read-state changes never cause
  re-notification.

Using `id` (monotonic auto-increment) as the watermark is robust against
interleaving of other sources and against `created_at` ties.

### Notification content (`notificationFor`)

| Source | Title | Body | Deep link |
|---|---|---|---|
| `meeting_prep` | "Meeting prep ready" | markdown→plain, truncated ~120 chars | `{web}/my/meeting-prep-v2/{source_id}` or `{web}/my/meetings` |
| `daily_meeting_prep` | "Meeting prep ready" | same | `{web}/my/meetings` |
| `meeting_notes` | "Meeting notes ready" | same | `{web}/meeting-notes/{source_id}` or `{web}/my/meetings` |

Markdown is stripped to plain text with a lightweight inline helper (strip
`#`, `*`, `_`, `` ` ``, link syntax → text; collapse whitespace; truncate).
No new markdown dependency.

### Web app base URL

The desktop bakes its `API_BASE_URL` at compile time via cargo features. The web
base URL mirrors it (from `infra/ari-config` in the agents repo):

| Feature | API | Web app |
|---|---|---|
| `prod-api` | `https://api.ari.ariso.ai` | `https://web.ari.ariso.ai` |
| `dev-api` | `https://api-dev.ari.ariso.ai` | `https://web-dev.ari.ariso.ai` |
| *(default)* | `http://localhost:4000` | `http://localhost:5173` *(web-ui Vite default — confirm)* |

Exposed to JS via a `get_web_app_base_url` Tauri command so the URL stays
consistent with the build target.

### Click handling

Use the notification plugin's action/click hook to open the deep-link URL via
the already-registered `opener` plugin (`openUrl`). **Best-effort:** Tauri v2
desktop notification click-callbacks are unreliable across platforms; if the
hook does not fire, the toast remains purely informational. This limitation is
accepted and will be noted in code comments.

## Settings toggle

A new "Notifications" section in `SettingsView.vue` with a single checkbox,
"Meeting notifications", matching the existing `auto-check-row` checkbox
pattern. Persisted as `meetingNotificationsEnabled` in `settings.json`
(default `true`). Toggling off stops the poller; toggling on starts it (and
requests OS permission if needed).

## Polling details

- **Interval:** 60s (meeting prep/notes are produced server-side; sub-minute
  latency is unnecessary). A 10s initial delay lets startup finish, mirroring
  the update scheduler.
- **Gating:** poll only when `auth.checkSession()` returns a session AND the
  setting is enabled AND OS permission is granted.
- **Lifecycle:** started by `BootstrapView` (always-alive main window); the
  composable is a module-level singleton so navigation never spawns duplicates.

## Error handling & edge cases

- **API failure:** log and skip the cycle; do not advance the watermark; retry
  next interval. Never surface a UI error (background task).
- **Not signed in:** skip the cycle silently; resume once signed in.
- **Permission denied:** poller no-ops; re-checks permission when the setting is
  re-enabled.
- **First run / fresh install:** baseline the watermark to current max; no
  flood.
- **App restart:** watermark persisted in `notifications.json`; no duplicate
  toasts.
- **Message with `source_id === null`:** use the fallback web route.
- **Empty / null `message`:** notify with title only (empty body).
- **Inbox endpoint not reachable by desktop session:** fail closed (logged, no
  notifications). Verify reachability early during build.

## Testing

- **Automated (vitest):** unit-test the pure logic — `selectMessagesToNotify`
  (first-run baseline returns empty; only `id > watermark` selected; ascending
  order; mixed sources already filtered out) and `notificationFor` (correct
  title/body/URL per source, source_id present vs fallback, markdown stripping,
  truncation). Add `vitest` devDep + `"test": "vitest run"` script + minimal
  config.
- **Manual (`npm run tauri:dev`):** sign in, trigger/seed a meeting_prep and a
  meeting_notes inbox message, confirm exactly one OS toast each, no repeats on
  next poll, no flood on first launch, toggle off suppresses, click opens the
  web URL (where the platform fires the callback).

## Open items to confirm during build

1. Desktop session can read `/user-inbox-messages` (auth/orgUserMapping).
2. Local web app base URL (`http://localhost:5173` assumed).
3. Reliability of the notification click callback on the target OS (macOS).
