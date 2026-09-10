# Microsoft sign-in (issue #370, part 1 of 3)

Issue #370 is delivered as three stacked changes, each shippable on its own:

1. **Microsoft sign-in** (this spec) — a second browser-OAuth provider next to Google.
2. **Tray "Sign In..." item** — `2026-09-10-tray-sign-in-item-design.md`.
3. **Meetings backend indicator** — `2026-09-10-meetings-backend-indicator-design.md`.

## Problem

`google_sign_in` (`src-tauri/src/commands.rs:606`) is the only sign-in
provider. Settings (`SettingsView.vue:214-237`) and Onboarding
(`OnboardingView.vue:8-24`) each offer a single "Sign in with Google" button.
Users whose identity lives in Microsoft 365 / Entra ID can't use Ariso from
oats.

## Server side: already shipped

Unlike what an earlier draft of this spec assumed, this needs no `agents`
change. As of `agents` `origin/main` (checked 2026-09-10):

- `apps/web-api/src/handlers/oauth_prepare_state.ts` handles
  `integration: "microsoft-signin"` (added in 0a9f365af, 2026-07-16). It
  stores `redirect` and `newUserSignupIntent` in exactly the same state shape
  as `google-signin`. It is **identity-only**: it ignores `scopes`, and
  Microsoft 365 data access is a separate connect flow.
- `apps/web-api/src/handlers/microsoft_signin.ts`
  (`/oauth2/microsoft-signin-callback`) finishes through the shared
  `redirectWithMagicLink` (`signin_shared.ts`). That function recognizes the
  desktop `/desktop-auth?callback_port=N&nonce=…` redirect and delivers the
  magic-link token to the loopback listener, the same way it does for Google.
- `GET /users/microsoft-avatar` returns `{ avatar, connected }`, the
  counterpart of `/users/google-avatar`.

The web UI already signs in this way (`apps/web-ui/src/Components/SignInForm.vue`).

## Goal

- "Sign in with Microsoft" works from Settings and Onboarding with the same
  browser + loopback flow, busy state, Cancel, and error handling as Google.
- A Microsoft-authenticated user never gets a Google consent screen.
- The signed-in account card shows the Microsoft avatar when one exists.

## Non-goals

- Microsoft Calendar. A Microsoft user gets no calendar-backed features (tray
  next meeting, meeting notifications, auto-join). Those features read meetings
  from the API, which gets them through Google Calendar. For these users they
  simply stay empty.
- Account linking between Google and Microsoft identities with the same email.
  The server owns this.
- Persisting which provider the user signed in with. Nothing below needs it.

## Design

### 1. Provider-parametrized browser sign-in (`src-tauri/src/commands.rs`)

Only the `prepare-state` body in `google_sign_in` is provider-specific. The
loopback listener, `SignInAttemptGuard`, `run_browser_sign_in`, and
`exchange_token_for_session` don't depend on the provider.

Add:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
enum SignInProvider { Google, Microsoft }

impl SignInProvider {
    /// The `/oauth2/prepare-state` body for this provider. Pure so it is unit-testable.
    fn prepare_state_body(self, redirect: &str) -> serde_json::Value { … }
}
```

- `Google` →
  `{ integration: "google-signin", scopes: ["calendar-readonly"], newUserSignupIntent: "personal_unless_domain_autojoin", redirect }`
  (unchanged from today).
- `Microsoft` →
  `{ integration: "microsoft-signin", newUserSignupIntent: "personal_unless_domain_autojoin", redirect }`.
  The body has no `scopes` key: the server ignores it, and sending
  `calendar-readonly` would suggest a grant that never happens.

Move the body of `google_sign_in` into a private
`async fn browser_oauth_sign_in(window, provider: SignInProvider) -> Result<SignInResult, String>`.
Then add two thin commands:

```rust
#[tauri::command]
pub async fn google_sign_in(window: tauri::WebviewWindow) -> Result<SignInResult, String> {
    browser_oauth_sign_in(window, SignInProvider::Google).await
}

#[tauri::command]
pub async fn microsoft_sign_in(window: tauri::WebviewWindow) -> Result<SignInResult, String> {
    browser_oauth_sign_in(window, SignInProvider::Microsoft).await
}
```

The frontend picks a command, not a provider string, so it has no way to
send an arbitrary `integration` value.

Rename `cancel_google_sign_in` to `cancel_sign_in`. It already aborts whatever
browser flow is pending, whether that's sign-in or the Calendar connect hop
(`commands.rs:706-727`), so after the rename the name matches what it does.
Register `microsoft_sign_in` and the renamed command in `main.rs`'s
`invoke_handler` (`main.rs:192-193`).

`validate_browser_auth_url` checks only the scheme (https, or local http in
non-prod builds), so `login.microsoftonline.com` passes without changes.

### 2. Frontend wrapper (`src/tauri.ts`)

Move the body of `auth.googleSignIn()` (`tauri.ts:56-88`) into a private
`browserSignIn(command: 'google_sign_in' | 'microsoft_sign_in')`. Export
`auth.googleSignIn()` and a new `auth.microsoftSignIn()` built on it. Both use
the same `oauth-result` listener contract. At most one sign-in attempt is live
at a time, so the two providers can share the event name. `auth.cancelSignIn()`
now invokes `cancel_sign_in`.

### 3. Settings (`src/views/SettingsView.vue`)

- Put a "Sign in with Microsoft" button under the Google button in
  `.sign-in-container`. Style it like `.google-btn`. For the glyph, use
  Microsoft's four-square logo (`#F25022`, `#7FBA00`, `#00A4EF`, `#FFB900`)
  as an inline SVG, next to the Google "G".
- Replace the boolean busy state with `signingInWith: 'google' | 'microsoft' | null`.
  - Both buttons are disabled while either provider's flow is pending.
  - Only the clicked button changes its label to "Continue in your browser…".
  - The existing Cancel button covers both providers.
- `handleMicrosoftSignIn()` mirrors `handleGoogleSignIn()`
  (`SettingsView.vue:1174`) with one difference: it **does not call
  `refreshCalendarAccess()`**. That function runs `auth.ensureCalendarAccess()`,
  which opens the *Google* Workspace connect hop in the browser. A Microsoft
  user would land on a Google consent screen.
  - Because of that, `calendarConnected` stays `null` for Microsoft users, and
    the "Connect Calendar" nudge (`v-if="calendarConnected === false"`) never
    appears.
- In `fetchUserProfile()` (`SettingsView.vue:988`), if
  `/users/google-avatar` returns no avatar, try `/users/microsoft-avatar`.
  - Both endpoints return `{ avatar }`, and both results go through the same
    `preloadAvatar()` WKWebView workaround.
  - This needs no provider bookkeeping.
  - If neither endpoint returns an avatar, the card falls back to initials, as
    it does today.

### 4. Onboarding (`src/views/OnboardingView.vue`)

Onboarding is where most users sign in for the first time, so it gets the same
second button.

- `handleMicrosoftSignIn()` mirrors `handleGoogleSignIn()`
  (`OnboardingView.vue:93`): it calls `emitNotificationsSync()`, emits
  `AUTH_SIGNED_IN_EVENT`, and finishes with `finishOnboarding({ openSettings: true })`.
- It skips the `ensureCalendarAccess()` block, for the same reason as Settings.
- The "Connecting your calendar…" label therefore never shows on the Microsoft
  path.

### 5. Capabilities

None. Custom `#[tauri::command]`s aren't scoped by
`src-tauri/capabilities/default.json`. Only plugin commands are. So
`microsoft_sign_in` and `cancel_sign_in` need no capability entry, just as
`google_sign_in` has none today.

## Cloud vs offline

This is Ariso-only by construction. Every piece is a network call to
`/oauth2/prepare-state` and `/auth/check`, and it's only reachable from the
sign-in UI, which Local users never need. Local-mode behavior doesn't change.

## Error handling

- **Server not configured**: if prod lacks
  `MICROSOFT_OAUTH_CLIENT_ID`/`SECRET`, prepare-state returns 500
  (`server_config_error`). The existing path turns that into
  `"API returned 500"` (`commands.rs:651-658`), shown inline under the
  buttons. No special case is added.
- **Cancel**: `cancel_sign_in` aborts the pending attempt whichever provider
  started it. The UI returns to the two-button state silently
  (`SIGN_IN_CANCELED_ERROR`).
- **First-ever sign-in of a placeholder user**: `redirectWithMagicLink` sends
  these users to the web onboarding wizard (`overrideTarget`) instead of the
  loopback, so the desktop waits until the 5-minute `SIGN_IN_TIMEOUT`. This is
  identical for Google today and isn't made worse here. Noted so a manual tester
  doesn't mistake it for a Microsoft bug.

## Testing

- **Rust (`commands.rs` `mod tests`)**:
  - `SignInProvider::prepare_state_body` for both providers. Google's body is
    byte-for-byte what `google_sign_in` sends today. Microsoft's carries
    `"microsoft-signin"`, `newUserSignupIntent`, `redirect`, and no `scopes`
    key.
  - Existing sign-in attempt tests keep passing after the rename.
- **Vitest (`tauri` wrapper)**: `auth.microsoftSignIn()` invokes
  `microsoft_sign_in` and resolves from `oauth-result`. `auth.cancelSignIn()`
  invokes `cancel_sign_in`.
- **Vitest (`SettingsView.test.ts`)**:
  - The Microsoft button calls `auth.microsoftSignIn()` and then
    `emitNotificationsSync()`, and never calls `auth.ensureCalendarAccess()`.
  - The Google path still calls it.
  - Both buttons are disabled while either flow is pending.
  - The avatar falls back to `/users/microsoft-avatar` when the Google avatar
    is null.
- **Vitest (`OnboardingView.test.ts`)**: same provider split. Microsoft
  sign-in emits `AUTH_SIGNED_IN_EVENT`, skips calendar connect, and finishes
  onboarding.
- **Manual (prod API)**:
  - Sign out. Sign in with a Microsoft work account from Settings, then from
    Onboarding (reset the `onboarded` flag).
  - Confirm the account card shows the name, email, and avatar (or initials),
    and that no Google consent page opens.
  - Cancel mid-flow and confirm the UI resets.

## Open questions

1. Does the prod Entra app registration accept personal Microsoft accounts
   (MSA) as well as work/school tenants? This is server config, and it decides
   whether the button label should say "Microsoft" or "Microsoft 365 / work
   account". This spec assumes plain "Sign in with Microsoft".
