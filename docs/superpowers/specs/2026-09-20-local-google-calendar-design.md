# Local Google Calendar connect (issue #430)

## Problem

Calendar access today is entirely a side effect of signing into Ariso.
`SettingsView.vue`'s `backend === 'local'` branch hides the Account card
outright — there is no sign-in UI at all in Local mode — and the only
calendar-connect path, `auth.ensureCalendarAccess()` /
`auth.connectGoogleCalendar()` (`src/tauri.ts`), is the *second hop* of
Ariso's OAuth: it calls the Ariso API (`/desktop/google-calendar-status`,
`connect_google_calendar` in `commands.rs`), which holds the Google grant and
serves calendar-derived meetings back through `GET /meetings`
(`tray_meeting.rs`, `useMeetingApi.ts`). None of that exists without an Ariso
session. `useAutoTrigger.ts` says this explicitly in a comment: "Only the
Ariso backend has a calendar ... always for Local we fall back to a user
confirmation."

So a privacy-focused user who runs the Local (fully offline) backend has no
way to see calendar context at all — no upcoming-meeting list, no "what's
next" — a feature Ariso users get for free. The business context on the issue
adds a second constraint: there's already an internal UX concern about
presenting a Google-account connection next to a backend whose whole pitch is
"nothing leaves the machine," so the two auth concepts (Ariso identity vs.
calendar-provider grant) must never look like the same thing in the UI.

## Goal

With the Local backend selected, from Settings:

- A user can connect Google Calendar directly — an OAuth handshake between
  the desktop app and Google, with **no Ariso account, session, or API
  call involved anywhere in the flow**.
- They see connection status (not connected / connecting / connected /
  error) and can disconnect, both clearly inside a Local-only "Calendar"
  card that is visually and functionally distinct from Ariso's Account card
  (which stays hidden in Local mode, as it is today).
- Once connected, the user's upcoming Google Calendar events for today (and
  the next day, when today is empty) appear in the Library's existing "Up
  Next" surface (`UpNextCard.vue`) the same way Ariso's calendar-derived
  meetings do — this is the concrete "calendar data available to the local
  meeting workflow" deliverable.
- The existing Ariso calendar integration (`google_sign_in`,
  `connect_google_calendar`, `/meetings`) is untouched: different commands,
  different frontend namespace, different token storage, never invoked from
  the Local card.

Reviewable as: switch to Local backend, open Settings, see a Calendar card
separate from any sign-in UI, connect a real Google account, see today's
events show up in the Library's Up Next card, disconnect, and confirm they
disappear and the grant is revoked on Google's end.

## Non-goals

- **Outlook / Microsoft Calendar.** The issue's acceptance criteria accept
  "Google Calendar and/or Outlook Calendar," so Google alone satisfies it.
  Microsoft Graph needs its own app registration, consent flow, and token
  shape — separable work, following the same precedent as
  `2026-09-10-microsoft-sign-in-design.md`, which shipped Microsoft identity
  and explicitly deferred Microsoft Calendar. A follow-up spec should mirror
  this one's design for Microsoft Graph once this ships.
- **Wiring calendar data into auto-trigger matching, the meeting-end
  stop-prompt, or the native tray-pill "next meeting" text.**
  `useAutoTrigger.ts`'s `resolveAssociation` hardcodes "always confirm" for
  any non-Ariso backend and stays that way here; `meetingEndWatch.ts`'s
  calendar-end-time awareness and the Swift/Win32 recorder-pill "next
  meeting" surface are each a separate subsystem this spec does not touch.
  The issue's bar — "available to the local meeting workflow" — is met by
  surfacing events in the Up Next card; deeper behavioral integration is
  real, valuable, separate work.
- **Meeting prep, auto-join-scheduled indicators.** Server-side-bot concepts
  that don't exist for Local; the mapped event shape (below) has no
  equivalent fields.
- **Multiple Google accounts, non-primary calendar selection, write access,
  or event creation.** Read-only, single (primary) calendar.
- **Push/webhook live sync.** Google Calendar push notifications require a
  publicly reachable HTTPS endpoint, which a pure desktop client doesn't
  have without a backend to broker it. Polling on a timer/window-focus is
  the only option and is sufficient for "what's on today."
- **A generic multi-provider calendar abstraction.** This spec is
  Google-specific end to end; if/when Outlook ships, that's the point to
  decide whether a shared abstraction is warranted.

## Design

### Why this can't reuse the Ariso calendar path

Today's calendar connect is additive to an existing Ariso identity: the
backend already holds a Google OAuth grant from `google_sign_in`
(`/oauth2/prepare-state`), and `connect_google_calendar` just asks it to
widen that grant with calendar scope. The desktop app never talks to Google
directly — it talks to the Ariso API, which talks to Google. Local mode has
no Ariso session and no backend to broker any of this, so it needs its own,
direct, client-only OAuth relationship with Google, plus its own on-device
event fetch to replace what `/meetings` provides for Ariso.

### OAuth: direct-to-Google, PKCE, loopback

The codebase already has a proven RFC 8252 loopback-redirect implementation
in `commands.rs` (`browser_oauth_sign_in`, `connect_google_calendar`,
`accept_loopback_callback`, `SignInAttemptGuard`, `validate_browser_auth_url`
— see `2026-07-15-browser-based-oauth-design.md`). This reuses that *pattern*
(bind the loopback listener before navigating, open the system browser, admit
exactly one nonce-matched callback, tear down on completion/timeout/cancel)
without touching those functions, because the delivery shape differs — an
authorization code instead of a magic-link token.

- A new Google Cloud OAuth 2.0 client of type **Desktop app**, distinct from
  Ariso's own web-application client used for `google-signin`. Desktop-app
  client identifiers are not confidential per Google's own installed-app
  guidance, so this design relies on **PKCE**, not a client secret, exactly
  as RFC 8252 recommends. The client id is a new, non-secret constant
  compiled into the app, same posture as `DEFAULT_API_BASE_URL`.
- Scope: `https://www.googleapis.com/auth/calendar.readonly` only — nothing
  broader, so the consent screen and Google's review surface stay minimal,
  and no `email`/`profile`/`openid` scope is requested (see Open questions).
- New `src-tauri/src/local_calendar.rs`:
  - `connect_local_google_calendar(window) -> Result<LocalCalendarStatus, String>`
    (`#[tauri::command]`): generates a PKCE `code_verifier`/`code_challenge`
    and a random nonce, binds a loopback `TcpListener` on `127.0.0.1:0`,
    builds `accounts.google.com/o/oauth2/v2/auth` with
    `redirect_uri=http://127.0.0.1:<port>/callback`, `response_type=code`,
    `access_type=offline`, `prompt=consent` (guarantees a `refresh_token` even
    on a reconnect), opens it via the system opener, and awaits one
    nonce-matched `GET /callback?code=…&state=…` — a new delivery variant
    alongside the existing token/status ones in the loopback-callback parser.
  - Exchanges the code at `https://oauth2.googleapis.com/token` (`reqwest`,
    `grant_type=authorization_code`, `code_verifier`), getting back
    `access_token` + `refresh_token` + `expires_in`.
  - Persists only the `refresh_token` (see Token storage). The `access_token`
    is cached in memory only, keyed off an `AppHandle` state, and refreshed
    on demand (`grant_type=refresh_token`) when expired.
  - `cancel_local_calendar_connect(window)`: mirrors `cancel_sign_in`'s
    contract, but against its own attempt-guard — a separate flow from
    Ariso's sign-in/calendar-connect guard, since the two are otherwise
    unrelated and could in principle both be mid-flight (a user toggling
    backends while one flow is pending).
  - `local_calendar_status() -> LocalCalendarStatus { connected: bool }`: a
    cheap keychain-presence check, no network call.
  - `disconnect_local_google_calendar()`: best-effort
    `POST https://oauth2.googleapis.com/revoke?token=<refresh_token>`, then
    deletes the keychain entry unconditionally — a failed revoke call must
    never leave the user stuck unable to disconnect locally.

### Token storage: OS keychain, never plugin-store

Per `oats-security` item #4, a Google refresh token is a long-lived secret
and must never land in `settings.json` (plaintext `plugin-store`). Use the
[`keyring`](https://crates.io/crates/keyring) crate (Security.framework on
macOS), the same approach proposed for bring-your-own remote-LLM API keys in
`2026-09-19-local-notes-model-picker-design.md`. If that spec's
`credentials.rs` wrapper has already landed by the time this is implemented,
reuse it — one keychain wrapper, one more account — rather than adding a
second `keyring` integration. Service name `ai.ariso.desktop`, account
`local-google-calendar-refresh-token`. No command reachable from the
frontend ever returns the token value; `local_calendar_status()` only ever
returns a boolean.

### Fetching events

```rust
pub struct LocalCalendarEvent {
    pub id: String,          // Google event id
    pub title: Option<String>,
    pub start_at: String,    // RFC3339
    pub end_at: Option<String>,
    pub cancelled: bool,     // event.status == "cancelled"
}
```

`list_local_calendar_events(days: u32) -> Result<Vec<LocalCalendarEvent>, String>`
calls `GET https://www.googleapis.com/calendar/v3/calendars/primary/events`
with `timeMin`/`timeMax` (local-day bounds, the same approach
`tray_meeting.rs`'s `day_bounds` already uses for Ariso), `singleEvents=true`,
`orderBy=startTime`, and `Authorization: Bearer <access_token>` (refreshed
first if stale). This is deliberately a smaller shape than Ariso's
`ScheduledMeeting` — no `auto_join_scheduled`/`prep_id`, which are
server-bot concepts with no Local equivalent.

### Frontend wiring

- `src/tauri.ts`: a new `localCalendar` namespace — `connect()`,
  `cancelConnect()`, `status()`, `disconnect()`, `listEvents(days)` — the
  same wrapper conventions as the existing `auth`/`local` namespaces.
- `SettingsView.vue`: inside the existing `backend === 'local'` branch,
  alongside the on-device-models card, add a **Calendar** card:
  - Not connected → "Connect Google Calendar" button.
  - Connecting → busy state + "Continue in your browser…" + Cancel,
    mirroring the Ariso sign-in busy pattern but with its own ref (e.g.
    `isConnectingLocalCalendar`) so the two flows' busy/error state can never
    cross-contaminate.
  - Connected → "Google Calendar connected" + Disconnect button.
  - Error → distinguishes "couldn't connect, try again" (network/server)
    from "permission needed" (user declined consent), both derived from the
    command's `Result::Err` string.
  - This card never appears alongside the Ariso Account card (Local mode
    already hides that card entirely), so there is no shared surface where
    the two auth concepts could be confused.
- `useBackend.ts`: `LocalBackend.listMeetings()` keeps calling
  `local.listRecordings()` and, when `localCalendar.status().connected`,
  also calls `localCalendar.listEvents(...)`, mapping each
  `LocalCalendarEvent` to a `MeetingListItem`: `id: `cal:${event.id}`` (a
  prefix that can never collide with a recording folder's UTC-timestamp id),
  `timestamp: start_at`, `endTimestamp: end_at`, `title`, with `files` and
  `status` left `undefined` to mark it as calendar-only (not a completed
  recording). The merged, re-sorted list is exactly what `UpNextCard.vue`
  and `groupMeetingsByDate.ts` already render generically off
  `timestamp`/`endTimestamp`/`title` — **no changes needed** to
  `UpNextCard.vue`, `groupMeetingsByDate.ts`, or `LibraryView.vue`.
- Clicking "Start" on a calendar-only row starts a normal new local
  recording; `RecordingMeta.meetingId` stays Ariso-only and is ignored by
  Local, so the new recording is not retroactively associated with the
  calendar event. That association is exactly the auto-trigger work called
  out in Non-goals.

### Capabilities

None. Like `connect_google_calendar` and the notes-model-picker's `keyring`
integration, this is plain Rust (`reqwest` + `keyring`) behind
`#[tauri::command]`s in an existing window (`settings`); no new window, no
new plugin permission, no capability-file entry.

## Cloud vs offline

Local-backend-only. The Ariso path is completely untouched — different
commands, different frontend namespace, different token storage — so signed-in
Ariso users see no change at all.

This is a deliberate, narrow exception to "Local means nothing leaves the
machine" (`oats-security` item #10), the same posture
`2026-09-19-local-notes-model-picker-design.md` already established for
bring-your-own-key remote notes: opt-in (nothing happens until the user
clicks Connect), narrow (OAuth + calendar-event reads only — never
meeting audio, transcript, or notes), and disclosed in the UI before the user
connects. Flag this in the PR description for review: Local mode already
makes network calls for model downloads, but this is the first **user data
integration** (an outbound account connection, not just an inbound model
pull) Local mode has ever had.

## Error handling

- **Google/OAuth client misconfigured**: surfaces as a plain error string
  under Connect; no special-cased UI.
- **User declines consent / closes the browser tab**: the loopback listener
  times out (same `SIGN_IN_TIMEOUT` pattern as sign-in) → UI returns to "Not
  connected" with a short message, no lingering busy state.
- **Cancel mid-flow**: `cancel_local_calendar_connect` tears down the
  listener and resets the UI silently, mirroring `cancel_sign_in`.
- **Refresh fails at fetch time** (token revoked outside oats, network down):
  `list_local_calendar_events` errors; `LocalBackend.listMeetings()` catches
  it and falls back to recordings-only — a calendar outage must never hide
  existing recordings. If Google reports `invalid_grant`, the next status
  check flips the Settings card to a "Reconnect" state.
- **Network loss**: same soft-fail — calendar rows silently drop from Up
  Next until connectivity returns, matching how `tray_meeting.rs`'s
  `fetch_today_meetings` already fails soft for Ariso.
- **Keychain unavailable/locked**: `connect`/`disconnect`/`status` return a
  clear error string; never panics.

## Testing

- **Rust (`local_calendar.rs`)**: PKCE verifier/challenge generation;
  loopback callback parsing for the new `code`+`state` delivery (valid,
  malformed, wrong nonce, missing code — extending the existing
  `parse_loopback_callback` test style); `LocalCalendarEvent` mapping from a
  sample Google Calendar API payload, including a cancelled event (mirrors
  `parse_meetings_drops_cancelled_meetings`'s style in `tray_meeting.rs`);
  token refresh fires only when the cached token is expired; keychain
  round-trip via the `keyring` crate's mock backend (per the notes-model-
  picker precedent) — assert the token never appears in any `Debug`/error
  string.
- **Vitest (`useBackend.test.ts`)**: `LocalBackend.listMeetings()` merges
  calendar events with recordings, prefixes calendar ids with `cal:`, and
  falls back to recordings-only when the calendar fetch rejects.
- **Vitest (`SettingsView.test.ts`)**: the Calendar card renders only when
  `backend === 'local'`; Connect → busy → Connected/error transitions;
  Disconnect calls the disconnect command and resets to "Not connected"; the
  Ariso Account card and this Calendar card never both render.
- **Manual**: with Local selected and a real Google account, Connect, grant
  calendar-readonly, confirm today's real events show up in the Library's Up
  Next card; Disconnect and confirm the grant is revoked
  (myaccount.google.com/permissions) and the events disappear; kill network
  mid-fetch and confirm recordings still list; reconnect a previously
  disconnected account and confirm a fresh consent screen appears rather
  than a silent re-grant.

## Open questions

1. **Google OAuth verification lead time.** `calendar.readonly` is a
   sensitive (not restricted) scope; a public OAuth consent screen requesting
   it needs Google's verification review before "unverified app" warnings
   stop showing to non-test users. This is a scheduling dependency, not an
   engineering one — worth starting in parallel with implementation.
2. **Ship Google alone, or hold for Outlook too?** This spec's position is
   Google first (see Non-goals); confirm that sequencing is acceptable given
   the issue's title names both providers explicitly.
3. **Is `calendar.readonly`-only (no `email`/`profile` scope) the right
   trade-off?** It keeps the Settings status line to a bare "Connected"
   rather than "Connected as you@gmail.com." Confirm that's an acceptable UX
   simplification versus the extra scope and review surface `email`/`profile`
   would add.
