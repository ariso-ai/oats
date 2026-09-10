# Backend mode indicator and sign-in entry points (issue #370)

## Problem

Today there is exactly one way to tell whether oats is running in Local mode
or signed in to Ariso, and exactly one way to sign in: open Settings and read
the Backend row / Account section (`src/views/SettingsView.vue`). The Meetings
window (`src/views/LibraryView.vue`, the app's main surface) shows nothing
about backend or auth state, and the tray menu (`src-tauri/src/tray.rs`,
`build_idle_menu`) has no sign-in entry at all — a signed-out Ariso user only
discovers they're signed out when they try to record and get bounced to
Settings with a banner (`ensure_recording_allowed` in `commands.rs:1430`,
`tray://show-sign-in-prompt`).

Sign-in itself is also narrower than the product wants: `google_sign_in`
(`commands.rs:606`) is the only provider wired up, opening the user's default
browser via the loopback-redirect flow from
`2026-07-15-browser-based-oauth-design.md`. There is no Microsoft path
anywhere in the desktop code or, as far as this repo shows, server-side
(no `integration: "microsoft-signin"`-shaped call to `/oauth2/prepare-state`
exists).

## Goal

- The Meetings window shows, at a glance, whether oats is in Local mode or
  signed in to Ariso.
- When in Ariso mode and signed out, that same indicator is clickable and
  starts sign-in without leaving the Meetings window.
- The tray menu shows a "Sign in" item only when the user is on the Ariso
  backend and signed out.
- Sign-in supports both Google and Microsoft.
- Local-mode users see the indicator but are never prompted to authenticate.
- Signing in or out anywhere updates the indicator and the tray menu
  immediately, in whichever windows are open.

## Non-goals

- No redesign of `SettingsView.vue`'s existing Account section beyond adding
  the Microsoft button next to the existing Google one — it remains the place
  for sign-out, avatar/name, and Calendar connect.
- No Microsoft Calendar integration. `google_sign_in` requests
  `calendar-readonly` as part of its scopes; Microsoft sign-in requests no
  scopes beyond identity. Calendar auto-join for Microsoft-authenticated users
  is out of scope.
- No change to `ensure_recording_allowed`'s existing signed-out gate
  (Settings + `tray://show-sign-in-prompt`) — that path is unaffected by this
  feature; it's a separate, already-working "you tried to record while signed
  out" flow.
- No native tray submenu for provider choice. The tray's "Sign in" item opens
  the Meetings window and lets the (new) indicator popover present the
  Google/Microsoft choice, rather than duplicating that choice as a muda
  submenu.
- No offline/Local sign-in of any kind — requirement 5 in the issue is met by
  construction, not by new gating logic (see Cloud vs offline).

## Design

### 1. Server-side dependency (must land before this ships)

Exactly like the browser-OAuth design's "Deployment ordering" section:
`google_sign_in` posts `{"integration": "google-signin", ...}` to
`{api_base_url}/oauth2/prepare-state`. Microsoft sign-in needs an equivalent
`integration: "microsoft-signin"` handled server-side in the `agents` repo
(web-api) — an OAuth app registration with Microsoft/Azure AD, a
`redirectWithMagicLink` path that accepts the same
`/desktop-auth?callback_port=<port>&nonce=<nonce>` redirect shape, and a
`GET /auth/session`-compatible session on the other end. This spec designs the
desktop side to be provider-agnostic so it lights up as soon as that lands,
but the web-api change is a hard prerequisite, not parallelizable work. See
Open questions.

### 2. Generalize the browser OAuth flow (`src-tauri/src/commands.rs`)

`google_sign_in` (`commands.rs:606`) already has no Google-specific logic in
its loopback/listener machinery — `BrowserFlow::SignIn`, `SignInAttemptGuard`,
`accept_loopback_callback`, and `exchange_token_for_session` are all
provider-agnostic already (confirmed by the existing tests in `mod tests`,
none of which assume a specific provider). Only two things are Google-specific:
the `integration` string and the `scopes` array in the `prepare-state` body.

Extract the body of `google_sign_in` into a private
`async fn browser_oauth_sign_in(window: tauri::WebviewWindow, integration: &str, scopes: &[&str], new_user_signup_intent: &str) -> Result<SignInResult, String>`,
and add:

```rust
#[tauri::command]
pub async fn google_sign_in(window: tauri::WebviewWindow) -> Result<SignInResult, String> {
    browser_oauth_sign_in(window, "google-signin", &["calendar-readonly"], "personal_unless_domain_autojoin").await
}

#[tauri::command]
pub async fn microsoft_sign_in(window: tauri::WebviewWindow) -> Result<SignInResult, String> {
    browser_oauth_sign_in(window, "microsoft-signin", &[], "personal_unless_domain_autojoin").await
}
```

Rename `cancel_google_sign_in` → `cancel_sign_in` (it already cancels
whichever `BrowserFlow::SignIn` attempt is pending, regardless of provider —
the rename just stops the name lying). Register both commands plus the rename
in `main.rs`'s `invoke_handler` (`main.rs:192-193`).

`src/tauri.ts`: add `auth.microsoftSignIn()` mirroring `auth.googleSignIn()`
(same `oauth-result` listener contract — only one sign-in attempt is ever
live, so both providers can share the event name), and rename
`auth.cancelSignIn()`'s underlying invoke to `cancel_sign_in`.

### 3. Shared auth-state composable (`src/composables/useAccountState.ts`, new)

`SettingsView.vue` currently owns `isSignedIn`/`displayName`/`email`/
`avatarUrl` plus `refreshSignedInAccount()`, `fetchUserProfile()`, and the
WKWebView avatar-preload workaround (`SettingsView.vue:1013-1035`) as private
state. The Meetings window needs the same signed-in/profile facts, and Tauri
windows are separate webview processes with independent JS runtimes — there's
no shared module singleton across windows, only events (mirrors why
`BACKEND_CHANGED_EVENT`/`AUTH_SIGNED_IN_EVENT` exist at all). So each window
that needs this state gets its own instance of a shared composable, kept in
sync via a broadcast event, not a cross-window store.

Extract `refreshSignedInAccount`, `fetchUserProfile`, and `preloadAvatar` out
of `SettingsView.vue` into `useAccountState.ts`, returning
`{ isSignedIn, displayName, email, avatarUrl, isSigningIn, errorMessage, refresh, signInWithGoogle, signInWithMicrosoft, cancelSignIn, signOut }`.
`refresh()` calls `auth.checkSession()` (and profile fetch if signed in) —
callers gate calling it on backend, see Cloud vs offline. `signInWithGoogle`/
`signInWithMicrosoft`/`signOut` wrap the existing `auth.*` calls, update local
state, and — new — call `emit(AUTH_STATE_CHANGED_EVENT)` on success (widen
the existing `AUTH_SIGNED_IN_EVENT` from `tauri.ts:8` to fire on sign-out too,
not just sign-in; today only `OnboardingView.vue:108` emits it, and only after
sign-in — `SettingsView.handleSignOut` never broadcasts, because no other
window has ever needed to know). Keep calling `emitNotificationsSync()`
alongside it, same as today, so `tray_meeting`/notifications re-sync.

`SettingsView.vue` and the new Meetings-window indicator (below) both use
this composable instead of hand-rolling the same session-check/profile-fetch/
avatar-preload logic twice.

### 4. Meetings window indicator (`src/views/LibraryView.vue`)

Add a small pill to the `.titlebar` row (next to `panel-toggle`/`add-btn`,
`LibraryView.vue:16-75`), driven by `activeBackend.value?.id` (already
tracked, `LibraryView.vue:341`) and a `useAccountState()` instance:

- **Local**: static "Local" pill, not a button — no click target, matching
  requirement 5 (no auth prompt in Local mode).
- **Ariso, signed in**: pill showing avatar/initials + short label (reuse the
  `.avatar`/`initials` pattern from `SettingsView.vue:901-904` and
  `:1330-1343`). Clicking it opens Settings (`invoke('create_settings_window')`,
  same call `LibraryView.vue`'s existing but currently-unused `openSettings()`
  at line 703 already makes) — full account management (sign out, Calendar
  connect) stays in Settings per Non-goals.
- **Ariso, signed out**: actionable "Sign in" pill. Clicking toggles a small
  popover anchored under it with two buttons, "Continue with Google" /
  "Continue with Microsoft" (same visual treatment as `SettingsView.vue`'s
  `.google-btn`, plus a Microsoft equivalent — Microsoft's brand guidelines
  use a 4-square glyph in Microsoft's four brand colors, analogous to the
  existing Google "G" SVG at `SettingsView.vue:220-225`). Each button calls
  `signInWithGoogle()`/`signInWithMicrosoft()` on the shared composable, shows
  the same "Continue in your browser…" busy state, and a Cancel button that
  calls `cancelSignIn()` — mirrors `SettingsView.vue:214-237` almost exactly,
  because it's the same flow from a different window.

Listen for `AUTH_STATE_CHANGED_EVENT` (from any window, including Settings)
and `BACKEND_CHANGED_EVENT` (already listened to at `LibraryView.vue:1247`
for meeting-list reload — extend that handler to also flip the indicator)
so the pill updates without the user touching it.

### 5. Tray "Sign in" entry (`src-tauri/src/tray.rs`)

`build_idle_menu` (`tray.rs:297`) gains an auth check next to its existing
`featured` param: `let signed_in = active_backend(app) != "ariso" || get_session_token(app).is_some();`
— true for Local (nothing to show) and for a signed-in Ariso user; false only
for a signed-out Ariso user, mirroring the exact condition
`tray_meeting::sync` already uses at `tray_meeting.rs:87`. When `!signed_in`,
insert a `MenuItemBuilder::with_id("sign_in", "Sign in").build(app)?` — placed
right after `start` (mirrors where `ensure_recording_allowed` already sends a
signed-out user first: straight at the primary action).

Add a `"sign_in"` arm to `on_menu_event` in `create_tray` (`tray.rs:186`)
that calls `open_library(app)` (existing helper, `tray.rs:288`) and then
`app.emit("tray://open-sign-in", ())` so the newly-opened (or focused)
Meetings window auto-opens its sign-in popover — same pattern as
`tray://show-sign-in-prompt` opening Settings' banner today
(`commands.rs:1445`), just routed at the new indicator instead.

**No new tray-refresh plumbing is needed.** `build_idle_menu` is already
rebuilt after every sign-in/out today, as a side effect of a path that
already exists for an unrelated reason: `signInWithGoogle`/`signOut` (via the
composable) call `emitNotificationsSync()` →
`SYNC_EVENT` → `BootstrapView.vue:21` → `invoke('sync_tray_meeting')` →
`tray_meeting::sync(app)` (`tray_meeting.rs:85`) → `refresh_tray(app, true)`
(`tray_meeting.rs:120`) → `tray::set_menu(app, false, false)`
(`tray.rs:122`) → `build_idle_menu`. Adding the `signed_in` check to
`build_idle_menu` is therefore the only change the tray needs; the refresh
trigger already fires on every sign-in and sign-out.

### 6. Capabilities

None. `library`, `settings`, and `onboarding` are all already in
`src-tauri/capabilities/default.json`'s `windows` list, and `google_sign_in`/
`check_session`/`sign_out` are already called from more than one of them
today (Settings and Onboarding both call `auth.googleSignIn()`). Custom
`#[tauri::command]`s aren't scoped by the permissions block — only plugin
commands (store, opener, window, webview, notification, updater) are — so
`microsoft_sign_in`/`cancel_sign_in` need no capability entry, same as their
Google counterparts today.

## Cloud vs offline

This is Ariso-backend-only functionality layered on Local mode, same framing
as the triage comment:

- **Local**: the indicator renders from `activeBackend.value?.id === 'local'`
  alone — it never calls `checkSession()` or touches `useAccountState`'s
  network path at all. This is a deliberate tightening versus
  `SettingsView.vue`'s current `onMounted`, which calls
  `refreshSignedInAccount()` (and therefore `check_session` →
  `GET /auth/session`) unconditionally on every app launch, regardless of the
  active backend, because Settings is pre-created hidden at startup
  (`oats-architecture`). That's pre-existing behavior this spec doesn't
  change or fix — it's out of scope — but the new Meetings-window indicator
  should not repeat it: gate `useAccountState().refresh()` behind
  `activeBackend.value?.id === 'ariso'` in `LibraryView.vue`, both on mount
  and on `BACKEND_CHANGED_EVENT`.
- **Ariso**: everything else in this spec — the signed-in/signed-out
  indicator states, both sign-in providers, the tray item — is Ariso-only by
  definition, since it's built entirely on `check_session`/`google_sign_in`/
  `microsoft_sign_in`, all of which are network calls.
- The tray's `signed_in` check treats "Local backend" and "Ariso, signed in"
  identically (both hide "Sign in") specifically so switching to Local never
  surfaces an auth prompt, satisfying requirement 5 at the tray layer too.

## Error handling

- **`microsoft_sign_in` before the web-api ships `integration:
  "microsoft-signin"`**: `prepare-state` returns non-200, and the existing
  code path already turns that into `SignInResult.error = "API returned
  {status}"` (`commands.rs:651-658`) — the popover shows that raw message,
  same as any other prepare-state failure today. No special-casing; this is
  acceptable because the feature is designed not to ship ahead of the
  server-side change (Open questions).
- **Cancel while the popover is open**: `cancelSignIn()` (→ `cancel_sign_in`)
  aborts whichever provider's attempt is pending, exactly like today's
  `handleCancelSignIn`; the popover returns to the two-button choice, not a
  bare error.
- **Starting a second provider while one is pending**: already handled —
  `SignInAttemptGuard::begin` supersedes any prior `BrowserFlow::SignIn`
  attempt (`commands.rs:615`), so clicking "Continue with Microsoft" while a
  Google attempt is still waiting on the browser silently cancels the Google
  one and starts fresh, matching the existing "starting a new sign-in aborts
  any pending one" behavior.
- **Sign-in succeeds in a window that isn't visible** (e.g. tray-triggered,
  Meetings window closed again before the browser completes): `oauth-result`
  is scoped to the webview that started the attempt
  (`browser-based-oauth-design.md`'s security note), so if that window closed
  mid-flow the event has nowhere to land — same pre-existing limitation as
  today's Settings flow, not introduced by this change.
- **Tray "Sign in" click racing a Local→Ariso backend switch**: `open_library`
  + `tray://open-sign-in` always resolves against whatever backend is active
  when the Meetings window reads it; if the user switched to Local in the
  half-second between the click and the window opening, the indicator simply
  renders its Local (static) state and the emitted `tray://open-sign-in` is
  ignored — no popover, no error.

## Testing

- **Rust (`src-tauri/src/tray.rs`)**: extend the existing `build_idle_menu`
  tests (`tray.rs:365` `mod tests`) with cases for `signed_in = true`/`false`
  — item present/absent, and that Local (`active_backend != "ariso"`) never
  shows it regardless of session token.
- **Rust (`src-tauri/src/commands.rs`)**: `browser_oauth_sign_in` is
  integration-string-parametrized only — add a unit test asserting
  `google_sign_in`'s prepare-state body still carries `"google-signin"` +
  `["calendar-readonly"]` and `microsoft_sign_in`'s carries
  `"microsoft-signin"` + `[]`, following the existing pattern of testing pure
  helper functions (`desktop_auth_redirect`, `validate_browser_auth_url`)
  rather than the full network round-trip.
- **Vitest (`useAccountState.test.ts`, new)**: mirrors
  `SettingsView.test.ts`'s existing auth mocks — `refresh()` reflects
  `checkSession()`; `signInWithGoogle`/`signInWithMicrosoft`/`signOut` each
  emit `AUTH_STATE_CHANGED_EVENT` and call `emitNotificationsSync()`.
- **Vitest (`LibraryView.test.ts`)**: indicator renders "Local" (non-button)
  for the local backend; "Sign in" (button) for Ariso+signed-out; avatar/name
  for Ariso+signed-in; clicking "Sign in" opens the popover with both
  provider buttons; `AUTH_STATE_CHANGED_EVENT`/`BACKEND_CHANGED_EVENT` each
  refresh it without a remount; `refresh()` is never called while the active
  backend is local.
- **Vitest (`SettingsView.test.ts`)**: extend the existing Google sign-in
  tests with a parallel Microsoft button case, and a case asserting sign-in
  and sign-out now emit `AUTH_STATE_CHANGED_EVENT` (currently neither does).
- **Manual (oats-desktop MCP, Ariso backend, an account with Google auth
  available)**: sign out in Settings, confirm the Meetings window indicator
  and tray both flip to "signed out" without reopening either window; click
  the Meetings-window indicator, sign in with Google, confirm both the
  indicator and the tray's "Sign in" item disappear immediately; switch to
  Local, confirm the indicator goes static and no sign-in prompt appears
  anywhere; trigger the tray's "Sign in" item, confirm it opens/focuses the
  Meetings window with the popover already open. Microsoft sign-in can only
  be manually verified once the web-api dependency (Design §1) ships.

## Open questions

1. **Sequencing with the web-api Microsoft OAuth work.** This spec assumes
   Microsoft sign-in support lands server-side (agents repo) either before or
   in lockstep with this change, the same ordering constraint called out in
   `2026-07-15-browser-based-oauth-design.md`. Is that work already planned,
   or should this ship as "Google now, Microsoft follows once the server
   side exists" — i.e. build the generalized `browser_oauth_sign_in` plumbing
   and the two-button UI now, but leave `microsoft_sign_in` returning a clear
   "not available yet" error (or hide the Microsoft button) until the API
   supports it?
2. **Signed-in indicator click target.** This spec sends it to Settings
   (the existing Account section). An alternative is a small inline
   dropdown (name/email + "Sign out") directly in the Meetings window,
   avoiding a window switch for the single most common action (signing out).
   Worth the extra UI for that one action, or is "opens Settings" enough?
3. **Tray sign-in routing.** This spec has the tray's "Sign in" item open the
   Meetings window and its popover, reusing that UI rather than building a
   native two-item submenu. Confirm that's preferred over a `MenuBuilder`
   submenu with "Sign in with Google" / "Sign in with Microsoft" as direct
   tray entries (which would let a user sign in without any window opening
   at all, but duplicates the provider-choice UI natively).
