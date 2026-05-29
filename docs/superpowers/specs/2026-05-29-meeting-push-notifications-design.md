# Meeting Push Notifications (Pusher) — Design

**Date:** 2026-05-29
**App:** `@ariso-ai/desktop` (`ariso-ai/sage`) — Tauri v2 + Vue 3
**Status:** Approved for planning

## Summary

Add native OS push notifications to the desktop app for **meeting prep ready**
events. The desktop subscribes in real time to the user's private **Pusher**
channel (the same mechanism the web app uses), and fires a native OS
notification when a `meeting-prep-complete` event arrives. The notification body
is the actual inbox message text (fetched by `source_id`). Clicking a
notification opens the corresponding prep page in the user's web app
(best-effort).

This **replaces an earlier polling design** (push via Pusher, not polling).
Scope is the **desktop side only** (`ariso-ai/sage`).

**Meeting notes are intentionally out of scope for this iteration** (see
Deferred). Only meeting prep is implemented.

## Background — how the web app pushes

From `apps/web-ui` in `ariso-ai/agents`:

- **`usePusher(channelName)`** (`composables/usePusher.ts`) creates
  `new Pusher(key, { cluster, channelAuthorization })`, subscribes to the
  channel, and returns `{ client, channel, cleanup }`.
- **Channel authorization:** Pusher calls `POST /pusher/auth` with
  `socketId` + `channelName`; the server (`handlers/pusher_auth.ts`) verifies
  `req.orgUserMapping` and only authorizes `private-{orgId}-{orgUserMappingId}`
  (general user channel) and `private-file-processing-{orgUserMappingId}`. We
  use the general channel.
- **Pusher client config** (public client keys — safe to embed):
  | Env | Key | Cluster |
  |---|---|---|
  | dev / local | `39d990870841a6b478cc` | `us2` |
  | prod | `ec77b8bc7dc9ff463c13` | `us2` |
- **The prep event** (verified by reading the publisher
  `worker/handlers/meetingPrep.ts`):
  | Event | Channel | Payload |
  |---|---|---|
  | `meeting-prep-complete` | `private-{orgId}-{orgUserMappingId}` | `{ meetingPrepId, eventId }` |
- **Inbox ordering:** `meetingPrep.ts` calls
  `userInboxService.createMessage(..., 'meeting_prep', meetingPrepId)` **before**
  triggering `meeting-prep-complete`. So when the event arrives, the
  `meeting_prep` inbox message (with `source_id === meetingPrepId`) already
  exists and can be fetched for the notification body.

## This app — relevant facts

- All backend HTTP goes through the Rust `api_request` command (`src/tauri.ts`),
  which attaches the session token. The session resolves to `orgUserMapping`
  server-side, so `/pusher/auth`, `/auth/me`, and `/user-inbox-messages` are all
  reachable.
- **`/auth/me`** returns `{ org_id, id, user_id, full_name, email, ... }` →
  channel name is `private-${org_id}-${id}`.
- `@tauri-apps/plugin-notification` is a dependency but **unwired** (not in
  `Cargo.toml`, `main.rs`, or `capabilities/default.json`).
- The hidden, always-alive **"main" window** (route `/#/`, currently an empty
  inline component) is the home for the persistent Pusher connection — mirroring
  the existing Rust update-scheduler loop in `main.rs`.
- `tauri.conf.json` has `csp: null`, so the webview may open the Pusher
  WebSocket (`wss://ws-us2.pusher.com`) without CSP changes.
- API base URL is a compile-time Rust const gated by cargo features
  (`prod-api` / `dev-api` / default-local). Web app URLs and Pusher config mirror
  this gating. Web URLs (from `infra/ari-config`): prod `https://web.ari.ariso.ai`,
  dev `https://web-dev.ari.ariso.ai`, local `http://localhost:5173`
  *(web-ui Vite default — confirm)*.
- `@tauri-apps/plugin-store` + `auth.checkSession()` + `SettingsView` checkbox
  patterns already exist and will be reused.
- **No JS test runner** (no vitest; workspace excluded from the monorepo turbo
  pipeline).

## Goals

- Fire a native OS notification, in real time, when a `meeting-prep-complete`
  event arrives on the user's private channel.
- Notification body = the real `meeting_prep` inbox message text (fetched by
  `source_id`), with a generic fallback.
- Click opens the prep page in the web app (best-effort).
- User can disable meeting notifications from Settings.
- Connection is maintained while signed in; torn down on sign-out.

## Non-Goals

- **Meeting notes notifications** (deferred — see below).
- No in-app notifications center / list UI (OS toasts only).
- No marking read / deleting inbox messages (desktop stays read-only; web owns
  read state).
- No non-meeting sources.
- No replay of events missed while the app was offline (Pusher does not replay;
  accepted — see Edge cases).

## Architecture

Approach: **Pusher real-time subscription in the always-alive main window**
(replaces polling). Matches the web app's mechanism, delivers events instantly,
and needs no client-side dedup/watermark.

### Data flow

```
BootstrapView (main window, /#/)
  └─ useMeetingNotifications.start()
       1. if signed in & enabled & OS permission granted:
       2. config  = getDesktopConfig()      // pusher key/cluster, web base URL (Rust, feature-baked)
       3. me      = GET /auth/me             // org_id, id
       4. channel = usePusher(`private-${org_id}-${id}`)   // auth via POST /pusher/auth (Rust)
       5. channel.bind('meeting-prep-complete', e => onPrepComplete(e.meetingPrepId))
       └─ onPrepComplete(meetingPrepId):
            items = GET /user-inbox-messages?limit=20
            msg   = findInboxMessage(items, 'meeting_prep', meetingPrepId)
            notify('Meeting prep ready',
                   body = msg?.message ?? 'Your meeting prep is ready.',
                   url  = `${webBase}/my/meeting-prep-v2/${meetingPrepId}`)   // OS toast
            on click → openUrl(url)                                           // opener plugin, best-effort
```

### Components / files (sage repo)

| File | Change |
|---|---|
| `package.json` | Add `pusher-js` dependency. |
| `src-tauri/Cargo.toml` | Add `tauri-plugin-notification = "2"`. |
| `src-tauri/src/main.rs` | Register `.plugin(tauri_plugin_notification::init())`; add `get_desktop_config` to the invoke handler. |
| `src-tauri/src/commands.rs` | Add feature-gated consts `PUSHER_KEY`, `PUSHER_CLUSTER`, `WEB_APP_BASE_URL` + `get_desktop_config` command returning `{ pusherKey, pusherCluster, webAppBaseUrl }`. |
| `src-tauri/capabilities/default.json` | Add `"notification:default"`. |
| `src/tauri.ts` | Add `getDesktopConfig()` wrapper; `pusherAuth(socketId, channelName)` wrapper over `api.request('POST','/pusher/auth', …)`. |
| `src/composables/usePusher.ts` | Create authenticated Pusher client/channel (channelAuthorization customHandler → `pusherAuth`); return `{ client, channel, cleanup }`. Mirrors the web composable. |
| `src/composables/useInbox.ts` | `InboxMessage` type; `listInboxMessages(limit)`; pure `findInboxMessage(items, source, sourceId)`; markdown-strip/truncate helper. |
| `src/composables/useMeetingNotifications.ts` | Singleton orchestrator: fetch config + `/auth/me`, build channel, subscribe, bind `meeting-prep-complete`, fetch inbox text, fire OS toast, wire click→openUrl. Gated on auth + setting + permission. `start()` / `stop()`. |
| `src/views/BootstrapView.vue` | New view at `/#/`; `start()` on mount, `stop()` on unmount. Replaces the empty inline component. |
| `src/main.ts` | Wire `BootstrapView` into the `/` route. |
| `src/views/SettingsView.vue` | "Notifications" section with a "Meeting notifications" checkbox (persisted in `settings.json`, default ON) controlling the subscription. |
| vitest setup | Add `vitest` devDep + `"test"` script + minimal config; spec for the pure helpers. |

### Notification mapping

- **Event:** `meeting-prep-complete`, payload `{ meetingPrepId, eventId }`.
- **Title:** "Meeting prep ready".
- **Body:** the fetched `meeting_prep` inbox message text (matched by
  `source_id === meetingPrepId`), markdown-stripped to plain text and truncated
  (~120 chars). If not found (race / not yet created), fall back to "Your
  meeting prep is ready."
- **Deep link:** `${webAppBaseUrl}/my/meeting-prep-v2/${meetingPrepId}`.
- `findInboxMessage(items, source, sourceId)` is **pure**: returns the item with
  matching `source` and numeric `source_id`, else `null`.

### Pusher client config delivery

`get_desktop_config` (Rust) returns the feature-baked `pusherKey`,
`pusherCluster`, and `webAppBaseUrl`, keeping client config consistent with the
build target (same pattern as the existing `API_BASE_URL` const):

| Feature | Pusher key | Cluster | Web app base |
|---|---|---|---|
| `prod-api` | `ec77b8bc7dc9ff463c13` | `us2` | `https://web.ari.ariso.ai` |
| `dev-api` | `39d990870841a6b478cc` | `us2` | `https://web-dev.ari.ariso.ai` |
| *(default/local)* | `39d990870841a6b478cc` | `us2` | `http://localhost:5173` *(confirm)* |

### Channel authorization

`usePusher` uses a `channelAuthorization.customHandler` that calls
`pusherAuth(socketId, channelName)` → `api.request('POST', '/pusher/auth',
{ socketId, channelName })` (Rust attaches the session). The handler reads
`req.body.{socketId,channelName}`, so a JSON body works (Express has both
json + urlencoded parsers). Returns `{ auth: … }` to the Pusher callback. The
WebSocket itself connects directly from the webview to Pusher.

### Click handling

On the notification, open the deep-link URL via the already-registered `opener`
plugin (`openUrl`). **Best-effort:** Tauri v2 desktop notification
click-callbacks are unreliable across platforms; if the hook doesn't fire, the
toast stays informational. Noted in code comments.

## Settings toggle

A new "Notifications" section in `SettingsView.vue` with a "Meeting
notifications" checkbox (matching the existing `auto-check-row` pattern),
persisted as `meetingNotificationsEnabled` in `settings.json` (default `true`).
Off → `stop()` (disconnect Pusher). On → `start()` (request OS permission if
needed, connect).

## Lifecycle

- Started by `BootstrapView` in the always-alive main window; the composable is
  a module-level singleton so navigation never creates duplicate connections.
- On sign-in → `start()`; on sign-out → `stop()` + `cleanup()` (unbind,
  unsubscribe, disconnect).
- `pusher-js` handles reconnection/backoff automatically.

## Error handling & edge cases

- **`/auth/me` or `/pusher/auth` fails:** log, do not crash; retry on next
  `start()` / reconnect. No UI error (background task).
- **Not signed in / permission denied / setting off:** no connection; toast is
  a no-op.
- **Inbox message not found for `meetingPrepId`:** use the generic body; still
  notify (the event itself signals prep is ready).
- **`csp: null`:** WS allowed; no config change. If CSP is ever tightened, add
  `connect-src wss://ws-us2.pusher.com https://sockjs-us2.pusher.com` + the API.
- **Events missed while offline:** Pusher does not replay; those notifications
  are lost (no historical flood, no catch-up). Accepted.
- **Duplicate delivery on reconnect:** not expected from Pusher; if observed,
  add a small in-memory set of recently-notified `meetingPrepId`s.

## Testing

- **Automated (vitest):** unit-test the pure logic —
  - `findInboxMessage(items, source, sourceId)`: matches on source + numeric
    `source_id`; returns `null` when absent; ignores other sources.
  - body builder: markdown stripping + truncation; generic-body fallback when
    the message is null.
  - channel-name builder: `private-${org_id}-${id}`.
  Add `vitest` devDep + `"test": "vitest run"` + minimal config.
- **Manual (`npm run tauri:dev`):** sign in; trigger a real
  `meeting-prep-complete`; confirm one OS toast with the inbox text as body;
  toggle off suppresses; click opens the web prep URL (where the platform fires
  the callback); sign-out disconnects.

## Deferred — meeting notes

Meeting notes notifications are **out of scope** for this iteration. There is no
Pusher event for notes today (`generateMeetingNotes.ts` is silent; only a daily
`meeting_notes` inbox digest exists). Adding them later would require a backend
change in `ariso-ai/agents` to, on notes completion, create the per-meeting
`meeting_notes` inbox message and trigger a `meeting-notes-complete` event on
`private-{orgId}-{orgUserMappingId}` — after which the desktop client extends
trivially (bind a second event, reuse `findInboxMessage` with source
`meeting_notes`, title "Meeting notes ready", deep link
`${webAppBaseUrl}/meeting-notes/{meetingId}`). The client is being structured so
this is an additive change.

## Open items to confirm during build

1. Local web app base URL (`http://localhost:5173` assumed).
2. Notification click-callback reliability on the target OS (macOS).
3. `POST /pusher/auth` accepts a JSON body via the Rust `api_request` path
   (verify the Express route parses JSON, not only urlencoded).
