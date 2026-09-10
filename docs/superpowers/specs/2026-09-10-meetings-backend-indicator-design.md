# Meetings backend indicator (issue #370, part 3 of 3)

Stacked on part 1 (`2026-09-10-microsoft-sign-in-design.md`: `microsoftSignIn`,
`cancel_sign_in`) and part 2 (`2026-09-10-tray-sign-in-item-design.md`: the
`on_session_changed` hook).

## Problem

The Meetings window (`src/views/LibraryView.vue`) is the app's main surface,
but it shows nothing about backend or auth state. Only Settings shows whether
oats is in Local mode or signed in to Ariso. There's also no way to sign in
from the Meetings window.

## Goal

- The Meetings titlebar shows at a glance whether oats is **Local** or on
  **Ariso**.
- On Ariso while signed out, the indicator is clickable and signs in with
  Google or Microsoft without leaving the window.
- Local users see a static indicator and are never prompted to authenticate.
- The indicator updates as soon as auth state changes anywhere: Settings,
  Onboarding, the popover, sign-out, or a background 401 clearing the token.

## Non-goals

- Account management in the Meetings window. Sign-out, avatar/name, and
  Calendar connect stay in Settings, and the signed-in indicator links there.
- Rerouting the tray's "Sign In..." item (part 2) to this popover. Native
  entry points (the tray and the record gate) keep one route, to Settings.
  The popover serves users who are already in the Meetings window.
- Fixing Settings' unconditional `check_session` on mount
  (`SettingsView.vue:1074`, which runs even in Local mode). That behavior
  predates this change and isn't touched here. The new indicator doesn't
  repeat it.

## Design

### 1. Broadcast auth changes from Rust (`commands.rs`)

Part 2's `on_session_changed(app)` runs after every session-token write. Here
it also calls `app.emit("auth://changed", ())` to every window, after the tray
redraw.

This replaces the earlier draft's plan to have each frontend call site emit an
`AUTH_STATE_CHANGED_EVENT`. That plan missed every token change the frontend
doesn't make:

- `is_session_valid` clearing on a 401
- the notifications and tray orchestrators clearing on a 401
- sign-in completing while the initiating window is hidden

The event carries **no payload**. Session tokens stay scoped to the webview
that started the flow (the `oauth-result` rule from
`2026-07-15-browser-based-oauth-design.md`). Listeners re-read state through
`check_session`.

In `src/tauri.ts`, `AUTH_SIGNED_IN_EVENT = 'auth://signed-in'` becomes
`AUTH_CHANGED_EVENT = 'auth://changed'`. Onboarding stops emitting it by
hand (`OnboardingView.vue:108` and part 1's Microsoft handler), because the
Rust hook now fires it. Settings' listener (`SettingsView.vue:1107`) switches
to the new name. Settings then also reflects sign-outs and background clears,
which it misses today.

### 2. Shared account-state composable (`src/composables/useAccountState.ts`, new)

Each Tauri window is a separate webview with its own JS runtime, so each
window gets its own instance, kept in sync by `auth://changed`. Move these out
of `SettingsView.vue`:

- the `isSignedIn`/`displayName`/`email`/`avatarUrl` state
- `refreshSignedInAccount` (`:1045`)
- `fetchUserProfile` (`:988`, including part 1's Microsoft avatar fallback)
- `preloadAvatar` (`:1015`)

The composable returns:

```ts
{
  isSignedIn, displayName, email, avatarUrl, initials,
  signingInWith,            // 'google' | 'microsoft' | null
  errorMessage,
  refresh,                  // check_session + profile; never throws; concurrent calls coalesce
  signIn(provider),         // 'google' | 'microsoft' → auth.googleSignIn / auth.microsoftSignIn
  cancelSignIn,
  signOut,
}
```

- `signIn` and `signOut` call `emitNotificationsSync()` after a success, as
  both views do today, so the native orchestrators restart or stop.
- They don't emit auth events. The Rust hook does that.
- `signIn` returns the result, so views keep their own follow-ups. Settings
  runs `refreshCalendarAccess()` after a Google sign-in. Onboarding runs
  `finishOnboarding()`.
- `refresh()` shares one in-flight promise across concurrent calls. A
  window's own sign-in triggers both its local refresh and the
  `auth://changed` listener, and should fetch the profile only once.

`SettingsView.vue` switches to the composable with no visible change.
Onboarding keeps calling `auth.*` directly: it's a one-shot window with no
account display, so moving it gains nothing.

### 3. The indicator (`src/views/LibraryView.vue`)

A pill in `.titlebar`, after the Start-recording button and before the Windows
`window-controls` (`LibraryView.vue:58-76`). It's an interactive element
outside `data-tauri-drag-region`. It is driven by `activeBackend.value?.id`
(`LibraryView.vue:341`) and a `useAccountState()` instance.

| State | Renders | Click |
| --- | --- | --- |
| backend not loaded yet | nothing | — |
| `local` | static pill: lock glyph + "Local", `title="Local mode — recordings stay on this Mac"` | none (a `<span>`, not a button) |
| `ariso`, signed in | avatar or initials + "Ariso", `title` = email | opens Settings via the existing `openSettings()` (`LibraryView.vue:703`) |
| `ariso`, signed out | "Sign in" button | toggles the provider popover |
| `ariso`, checking | "Ariso" with no avatar, not clickable | — |

The **provider popover** is anchored under the pill:

- It holds "Sign in with Google" and "Sign in with Microsoft", with the same
  glyphs and treatment as the Settings buttons. Factor both into a small
  shared `SignInButtons.vue` used by Settings and the popover.
- It shows the same busy label ("Continue in your browser…"), a Cancel button,
  and an inline error.
- It closes on Escape, on an outside click (unless a flow is pending), and on
  successful sign-in. Once `isSignedIn` turns true, the pill switches state.

The pill is compact: at most about 120px, with the label ellipsized. The
Windows titlebar already carries brand, divider, and window controls, so the
pill mustn't push Start recording off narrow windows. At widths where
`.add-btn-label` collapses, the pill collapses to its icon too, keeping the
`title` tooltip.

### 4. Keeping it current

- **On mount**: load `activeBackend` (already done), then call `refresh()`
  only if it's `ariso`.
- **`auth://changed`**: call `refresh()` if the backend is `ariso`. Ignore it
  in Local mode.
- **`BACKEND_CHANGED_EVENT`**: the existing handler (`LibraryView.vue:1247`)
  already reloads meetings and re-reads the backend. When the new backend is
  `ariso`, it also calls `refresh()`. When it's `local`, it resets
  `isSignedIn` so a later switch back doesn't briefly show stale state.

### 5. Window-aware sign-in attempts (`commands.rs`)

With this part, three windows can start a browser flow: Onboarding, Settings,
and Meetings. Today `begin_sign_in_attempt` (`commands.rs:288`) supersedes a
pending attempt by aborting its task **without telling its window**. Take
Settings mid-flow, then the popover starting a new attempt:

- Settings stays on "Continue in your browser…" forever.
- Its `resultPromise` never resolves.
- Its Cancel button calls `cancel_sign_in`, which aborts *the popover's*
  attempt.

To fix this:

- `PendingSignIn` records the starting window's label.
- `SignInAttemptGuard::begin` returns the superseded attempt's `(flow, label)`.
  The command then emits that flow's silent-cancel result
  (`SIGN_IN_CANCELED`) to the old window with `emit_to(label, …)`. The old
  window resets, just as if the user had pressed Cancel.
- `cancel_sign_in` aborts the pending attempt only when its label matches the
  calling window's. Otherwise it does nothing.

`begin_sign_in_attempt` stays a pure function (no `AppHandle`), so the
existing attempt-slot unit tests still work.

### 6. Capabilities

None. `library` is already in `capabilities/default.json`'s `windows`, and
`listen` on an app-wide event plus custom commands need no new permission.

## Cloud vs offline

- **Local**: the pill renders from `activeBackend.value?.id === 'local'` alone.
  The Meetings window makes no `check_session` or profile request in Local
  mode, including on `auth://changed` (§4). There's no clickable state, so
  there's no auth prompt, which is requirement 5 of issue #370.
- **Ariso**: all other states. They're built on `check_session`, `/auth/me`,
  the avatar endpoints, and part 1's sign-in commands.

## Error handling

- **`check_session` or network failure**: `refresh()` never throws and treats
  failure as signed out (the current Settings behavior). While offline, the
  pill says "Sign in", and a click shows the prepare-state error inline in
  the popover.
- **Sign-in completes after the popover's window closed**: Meetings windows are
  destroyed on close, but the loopback task keeps running. If the browser
  finishes, the token is still stored. The `oauth-result` emit to the destroyed
  webview does nothing, and `on_session_changed` updates the tray and every
  open window through `auth://changed`. Today's Settings-only flow can't do
  this.
- **Popover open, backend switched to Local**: the Local state has no popover.
  Hide it and cancel any flow this window has pending.

## Testing

- **Rust (`commands.rs` `mod tests`)**:
  - Superseding an attempt returns the previous attempt's flow and window
    label.
  - Cancelling from a window that doesn't own the pending attempt leaves it
    active.
  - Cancelling from the owning window aborts it.
- **Vitest (`useAccountState.test.ts`, new)**:
  - `refresh()` reflects `checkSession()` and never throws.
  - Concurrent `refresh()` calls fetch `/auth/me` once.
  - `signIn('google' | 'microsoft')` calls the matching `auth.*` and
    `emitNotificationsSync()` on success, and stays silent on
    `SIGN_IN_CANCELED_ERROR`.
  - `signOut()` clears state.
- **Vitest (`LibraryView.test.ts`)**:
  - Local renders a non-button "Local" and never calls `checkSession`, even
    after `auth://changed`.
  - Ariso + signed out renders "Sign in", and clicking it opens the popover
    with both providers.
  - Ariso + signed in renders avatar or initials, and clicking it calls
    `create_settings_window`.
  - `auth://changed` and `BACKEND_CHANGED_EVENT` each update the pill without
    a remount.
- **Vitest (`SettingsView.test.ts`)**: existing account tests pass against the
  composable, and `auth://changed` triggers a refresh (sign-out elsewhere
  clears the card).
- **Manual (oats-desktop MCP, Ariso backend)**:
  - Sign out in Settings, with Meetings open. The pill flips to "Sign in"
    without a reload.
  - Sign in with Google from the popover. The pill shows the avatar, and
    "Sign In..." disappears from the tray.
  - Repeat with Microsoft.
  - Start sign-in in Settings, then start one from the popover. Settings
    resets to idle (§5).
  - Switch to Local. The pill turns into a static "Local", and no network
    requests appear in the Meetings window's devtools.
