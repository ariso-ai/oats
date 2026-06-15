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
- The persistent Pusher connection lives in the **native Rust runtime** (a
  spawned tokio task, mirroring the existing update-scheduler loop in `main.rs`),
  not in a webview — macOS suspends hidden/occluded webviews, which froze the
  JS listener and dropped events. The Rust process is never suspended.
- Because the WebSocket opens from Rust (`tokio-tungstenite`), webview CSP
  (`csp: null`) doesn't apply to it.
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
- No general event replay: Pusher does not redeliver to disconnected clients.
  A bounded `catch_up` inbox fetch on each (re)subscribe recovers *unread
  meeting preps* specifically; arbitrary missed events are not replayed.

## Architecture

Approach: **Pusher real-time subscription in the native Rust process**
(replaces polling). The connection runs in the always-alive Rust runtime — not
in a webview — because macOS suspends hidden/occluded webviews, which froze the
JS listener and dropped events. The native process is never suspended, so the
WebSocket stays alive in the background. Realtime delivery is best-effort
(Pusher does not redeliver to disconnected clients), so each (re)subscribe also
runs a catch-up inbox fetch and a process-lifetime `seen` set dedupes the
realtime and catch-up paths.

> **Note (build outcome):** an earlier draft of this spec put the subscription
> in the webview via a `pusher-js` + `usePusher.ts` client. That approach was
> abandoned during the build for the macOS-suspension reason above; the
> orchestrator was reimplemented natively in Rust (no `pusher-js` dependency,
> no frontend channel code). The sections below reflect the shipped design.

### Data flow

```
Rust runtime (src-tauri/src/meeting_notifications.rs)
  └─ sync(app)  // triggered on sign-in, sign-out, and the settings toggle
       1. desired = notifications enabled && session token present
       2. if !desired → stop(); else spawn run_loop (if not already running)
       └─ run_loop → run_session (reconnect w/ exponential backoff to 30s):
            me      = GET /auth/me                         // org_id, id (Bearer session token)
            channel = `private-${org_id}-${id}`
            ws      = wss://ws-${PUSHER_CLUSTER}.pusher.com/app/${PUSHER_KEY}
            on pusher:connection_established:
              auth = POST /pusher/auth { socketId, channelName }   // Rust, session token
              send pusher:subscribe { channel, auth }
            on subscription_succeeded → catch_up()          // seed-only on first subscribe
            on 'meeting-prep-complete' { meetingPrepId } → handle_prep(meetingPrepId):
              items = GET /user-inbox-messages?limit=20
              msg   = items.find(source == 'meeting_prep' && source_id == meetingPrepId)
              show('Meeting prep ready',
                   body = msg?.message (markdown-stripped, truncated) ?? 'Your meeting prep is ready.',
                   url  = `${WEB_APP_BASE_URL}/my/meeting-prep-v2/${meetingPrepId}`)
              // macOS bundle: UNUserNotificationCenter; click opens url via delegate.
              // dev / other OS: tauri-plugin-notification (no click handling).
```

The frontend (`src/composables/useMeetingNotifications.ts`) owns only the
settings toggle + OS-permission UI; it persists `meetingNotificationsEnabled`
and broadcasts a `meeting-notifications-sync` event so the native orchestrator
re-evaluates via `sync()`.

### Components / files (sage repo)

The Pusher subscription, channel auth, inbox fetch, and OS-notification dispatch
all live natively in Rust. The frontend keeps only the settings UI; there is no
`pusher-js` dependency and no frontend channel/auth code.

| File | Change |
|---|---|
| `src-tauri/Cargo.toml` | Add `tauri-plugin-notification`, `tokio-tungstenite`, `futures-util` (WS client); macOS: `objc2*` crates for UNUserNotificationCenter click handling. |
| `src-tauri/src/main.rs` | Register the notification plugin + `NotificationManager` state; call `meeting_notifications::init_native` (installs the macOS UNC delegate, main thread); expose `sync_meeting_notifications` / `stop_meeting_notifications` commands. |
| `src-tauri/src/commands.rs` | Add feature-gated consts `PUSHER_KEY`, `PUSHER_CLUSTER`, `WEB_APP_BASE_URL` (baked per `prod-api`/`dev-api`/local, mirroring `API_BASE_URL`); session-token helpers consumed by the orchestrator. |
| `src-tauri/src/meeting_notifications.rs` | **Native orchestrator** (new): `sync`/`stop`, `run_loop` (reconnect + backoff), `run_session` (WS connect → `/pusher/auth` → subscribe → read), realtime `handle_prep` + `catch_up` backstop with a shared `seen` dedup set, inbox fetch + markdown-strip/truncate, and notification `show` (macOS UNC with click→open, else plugin). |
| `src-tauri/capabilities/default.json` | Add `"notification:default"`. |
| `src/composables/notifications.ts` | Pure TS helpers (`findInboxMessage`, `stripMarkdown`, `truncate`, `buildPrepNotification`, `prepChannelName`) retained for unit tests; the Rust orchestrator owns the runtime path. |
| `src/composables/useMeetingNotifications.ts` | Settings/permission shim only: read/write `meetingNotificationsEnabled`, request OS permission, open macOS notification settings, and `emit` `meeting-notifications-sync` so the native `sync()` re-evaluates. No subscription logic. |
| `src/views/SettingsView.vue` | "Notifications" section with a "Meeting notifications" checkbox (persisted in `settings.json`, default ON) that drives the permission flow + sync broadcast. |
| `package.json` | Add `vitest` devDep + `"test"` script for the pure helpers. |

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

The Pusher key/cluster and web base URL are feature-baked Rust consts
(`PUSHER_KEY`, `PUSHER_CLUSTER`, `WEB_APP_BASE_URL` in `commands.rs`) consumed
directly by the native orchestrator — no invoke command crosses to the
frontend. They follow the same per-feature gating as `API_BASE_URL`:

| Feature | Pusher key | Cluster | Web app base |
|---|---|---|---|
| `prod-api` | `ec77b8bc7dc9ff463c13` | `us2` | `https://web.ari.ariso.ai` |
| `dev-api` | `39d990870841a6b478cc` | `us2` | `https://web-dev.ari.ariso.ai` |
| *(default/local)* | `39d990870841a6b478cc` | `us2` | `http://localhost:5173` *(confirm)* |

### Channel authorization

The orchestrator opens the Pusher WebSocket directly from the Rust process. On
`pusher:connection_established` it calls `POST /pusher/auth` with a JSON body
`{ socketId, channelName }` and a `Bearer` session token (the server reads
`req.body.{socketId,channelName}`), then sends `pusher:subscribe` with the
returned `{ auth }` signature. A 401/403 on this (or `/auth/me`) is treated as
an invalid session: the token is cleared and the loop exits until the next
sign-in broadcast.

### Click handling

The deep-link URL rides along as the notification's request identifier. On the
**macOS bundle**, a `UNUserNotificationCenter` delegate receives the click on
the main thread and runs `open <url>`. On unsigned/ad-hoc builds UNC errors, and
in dev / on other platforms the orchestrator falls back to
`tauri-plugin-notification`, which has **no click handling** (the toast stays
informational). Noted in code comments.

## Settings toggle

A new "Notifications" section in `SettingsView.vue` with a "Meeting
notifications" checkbox (matching the existing `auto-check-row` pattern),
persisted as `meetingNotificationsEnabled` in `settings.json` (default `true`).
Toggling it requests OS permission (when turning on) and emits
`meeting-notifications-sync`; the native `sync()` then starts or stops the
orchestrator accordingly.

## Lifecycle

- The native orchestrator runs in the always-alive Rust runtime (never spawned
  per-webview), so navigation can't create duplicate connections.
- `sync()` is (re)invoked on sign-in, sign-out, and the settings toggle. It
  starts the task when `enabled && signed in`, and `stop()`s (aborts the task)
  otherwise.
- `run_loop` handles reconnection itself: exponential backoff (1s → 30s cap) on
  transient errors; on an auth rejection it clears the token, resets the handle,
  and waits for the next sign-in sync to re-spawn.

## Error handling & edge cases

- **Transient `/auth/me`, `/pusher/auth`, `/user-inbox-messages`, or WS failure:**
  log, do not crash; `run_loop` reconnects with backoff. No UI error
  (background task). All session-bound calls have a 15s timeout so a hung
  connect can't stall the loop.
- **Auth rejected (401/403):** clear the stored token, exit the loop, and wait
  for the next sign-in sync — avoids hammering an already-invalidated session.
- **Not signed in / setting off:** `sync()` keeps the orchestrator stopped; no
  connection. (OS permission is a frontend concern; a denied permission still
  lets the orchestrator run — the OS just suppresses the banner.)
- **Inbox message not found for `meetingPrepId`:** use the generic body; still
  notify (the event itself signals prep is ready).
- **WebSocket transport:** the connection opens from the Rust process, not the
  webview, so webview CSP (`csp: null`) is irrelevant to it.
- **Events missed while offline:** Pusher does not replay, but the `catch_up`
  inbox fetch on each (re)subscribe surfaces any unread `meeting_prep` not yet
  notified. The first subscribe is seed-only (no replay of pre-existing preps
  on launch).
- **Duplicate delivery (reconnect / realtime+catch-up overlap):** a
  process-lifetime `seen` set of notified `meetingPrepId`s dedupes both paths.

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
