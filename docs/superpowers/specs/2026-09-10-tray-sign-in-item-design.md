# Tray "Sign In..." item (issue #370, part 2 of 3)

Stacked on part 1 (`2026-09-10-microsoft-sign-in-design.md`). The Settings
Account card it opens already offers both Google and Microsoft.
Part 3 (`2026-09-10-meetings-backend-indicator-design.md`) builds on the auth
refresh hook added here.

## Problem

The tray idle menu (`src-tauri/src/tray.rs:297` `build_idle_menu`) has no
sign-in entry. A signed-out Ariso user only finds out when they try to record:
`ensure_recording_allowed` (`commands.rs:1430`) bounces them to Settings with
a "Please sign in to start recording." banner.

## Goal

- Show a "Sign In..." tray item only when the active backend is Ariso and there
  is no session. Never show it in Local mode.
- The item disappears the moment the user signs in and reappears the moment
  the session is gone (sign-out, or a background 401), even if nothing else
  in the menu changed.

## Non-goals

- A native provider submenu ("Sign in with Google" / "Sign in with
  Microsoft"). Sign-in reports its result to the webview that started it
  (`oauth-result`, scoped because it carries the session token), so a
  window-less tray sign-in would need new plumbing. The Settings card already
  offers both providers.
- The recording menu (`build_recording_menu`). Recording on Ariso already
  requires a session, and Local needs none.

## Design

### 1. When to show the item (`src-tauri/src/tray.rs`)

Add a pure predicate so it can be unit-tested. `build_idle_menu` takes an
`AppHandle` and has no tests today. The existing `mod tests` covers only
icons.

```rust
/// Whether the idle menu offers "Sign In...": Ariso backend with no stored session.
/// Local never needs auth, so it never shows the item.
fn needs_sign_in(backend: &str, has_session: bool) -> bool {
    backend == "ariso" && !has_session
}
```

`build_idle_menu` evaluates it with `crate::commands::active_backend(app)` and
`get_session_token(app).is_some_and(|t| !t.is_empty())`. That's the same
presence check `tray_meeting::sync` (`tray_meeting.rs:87`) and
`is_session_valid` use. When the predicate is true, insert
`MenuItemBuilder::with_id("sign_in", "Sign In...")` directly above
`start_recording`. Signing in is the step before recording, and a signed-out
Ariso user has no featured-meeting rows above it anyway, because
`tray_meeting` is stopped. The "..." matches the existing window-opening items
("Settings...", "Meetings...").

The check is token *presence*, not validity. An expired token nobody has used
yet leaves the item hidden until the next API call gets a 401 and clears it
(see §3). That's the same trade-off the tray's next-meeting row makes today.

### 2. Click routing (`tray.rs` `on_menu_event`, `commands.rs`)

Pull the signed-out branch of `ensure_recording_allowed`
(`commands.rs:1444-1445`) into
`pub(crate) fn surface_sign_in(app, reason: SignInReason)`. It opens the
pre-created Settings window and emits `tray://show-sign-in-prompt` with a
`{ reason: "record" | "menu" }` payload. `ensure_recording_allowed` calls it
with `Record`, and a new `"sign_in"` arm in `create_tray`'s `on_menu_event`
calls it with `Menu`. The native sign-in entry points share one route, so they
can't drift apart.

In Settings (`SettingsView.vue:1104`), the listener keeps the reason. The
banner (`SettingsView.vue:19-21`) then shows:

- `record` (or no payload, for backward compatibility): "Please sign in to
  start recording." (unchanged)
- `menu`: "Sign in to your Ariso account." A banner about recording would be
  wrong when the user just clicked "Sign In...".

The listener also scrolls the Account card into view. Settings is tall, and the
card sits below Backend and Local models.

### 3. Rebuilding the menu when auth changes

An earlier draft of this spec claimed no new plumbing was needed because
sign-in/out already reaches `build_idle_menu` through `emitNotificationsSync()`
→ `sync_tray_meeting` → `tray_meeting::sync`. **That holds for sign-out but
not for sign-in:**

- Sign-out: `sync` hits `stop()`, which calls `refresh_tray(app, true)`, so the
  menu is rebuilt.
- Sign-in: `sync` spawns `run_loop`, which calls
  `refresh_tray(&app, menu_changed)`. `menu_changed` is false whenever the
  featured meeting stays `None` (no meetings left today, a Microsoft user with
  no calendar, or a failed fetch), so only the title is redrawn and a stale
  "Sign In..." stays in the menu.

Fix it at the point where the token actually changes. Every write already goes
through two functions in `commands.rs`: `set_session_token` (`:216`) and
`clear_session_token` (`:222`). Their callers are:

- `exchange_token_for_session`
- `sign_out`
- `is_session_valid`
- `meeting_notifications.rs:146`
- `tray_meeting.rs:182`

After a successful store save, both functions call a new
`fn on_session_changed(app)`. In this part it does one thing: it redraws the
tray through `tray_meeting`'s existing `refresh_tray(app, true)`, made
`pub(crate)`. That function already hops to the main thread (muda menus are
main-thread on macOS) and already skips rebuilding the idle menu while a
recording owns the tray. Part 3 adds a broadcast event to the same hook.

This also covers the background paths the frontend never hears about: the
notifications or tray orchestrator getting a 401 and clearing the token now
brings "Sign In..." back right away.

### 4. Backend switches

Switching backend already resyncs the tray: `selectBackend`
(`SettingsView.vue:663`) emits `emitNotificationsSync()`, which leads to
`tray_meeting::sync`.

- Switching to Local runs `stop()`, which rebuilds the menu without the item.
- Switching to Ariso while signed out also runs `stop()`, which rebuilds the
  menu with the item.

One path misses the resync. `cancelDownloadModels` (`SettingsView.vue:706`)
reverts Local → Ariso when the user declines the model download, but it only
emits `BACKEND_CHANGED_EVENT`. The tray, and the next-meeting orchestrator,
keep their Local state, so a signed-out user wouldn't see "Sign In...". Add
the same `emitNotificationsSync()` call there. This bug predates the feature,
but the feature makes it visible.

### 5. Capabilities

None. The new Settings listener handles a Rust-emitted event, and no new
command is added.

## Cloud vs offline

`needs_sign_in` returns false for any backend other than `"ariso"`, so Local
mode never shows the item. Issue #370's requirement 5 holds at the tray layer
by construction. `on_session_changed` only redraws the menu, which makes no
network call, so it's safe to run in either mode.

## Error handling

- **Menu build fails**: `set_menu` keeps the previous menu, as it does today
  (`tray.rs:140`). The item may be stale until the next rebuild. No new
  failure mode.
- **Click races a switch to Local**: Settings opens with a banner, but the
  banner is conditioned on `!isSignedIn`. The Account card is still correct,
  and the banner is harmless. It clears on the next sign-in or window reload,
  as today.
- **Store save fails in `set_session_token`/`clear_session_token`**: no redraw.
  The token state didn't change either, so the menu is still accurate.

## Testing

- **Rust (`tray.rs` `mod tests`)**: truth table for `needs_sign_in`.
  `("ariso", false)` → true. `("ariso", true)`, `("local", false)`, and
  `("local", true)` → false.
- **Rust (`commands.rs`)**: `SignInReason` serializes to
  `{ "reason": "record" }` / `{ "reason": "menu" }`.
- **Vitest (`SettingsView.test.ts`)**:
  - `tray://show-sign-in-prompt` with `reason: "menu"` shows the account
    banner.
  - No payload, or `reason: "record"`, keeps the recording copy.
  - `cancelDownloadModels` now calls `emitNotificationsSync()`.
- **Manual (oats-desktop MCP, Ariso backend)**:
  - Sign out: "Sign In..." appears.
  - Click it: Settings opens scrolled to the Account card, with the account
    banner.
  - Sign in on a day with no remaining meetings: the item disappears
    immediately. This is the case the earlier draft missed.
  - Switch to Local: the item disappears.
  - Switch to Local, decline the model download (reverts to Ariso): the item
    reappears.
