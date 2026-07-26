# Browser-based OAuth sign-in (issue #226)

## Problem

`google_sign_in` opens the Google OAuth flow in a native Tauri webview window and
intercepts the `…/magic-link?token=…` redirect via `on_navigation`. Native webviews are
increasingly hostile to identity flows: passkeys don't work, Google can block embedded
user agents, and cookies/password managers from the user's real browser are unavailable.
Support reports (passkey sign-in failures) motivated moving the handshake to the user's
default browser (the RFC 8252 "native apps" recommendation).

## Constraint discovered during design

The OAuth redirect chain is entirely server-controlled:

```
Google → api /oauth2/google-signin-callback → redirectWithMagicLink()
       → {WEB_APP}/magic-link?token=<magic-link-token>&target=<web path>
```

`prepare-state` accepts a `redirect` body field, but `redirectWithMagicLink` only honors
web-app paths (`startsWith('/')`). Once the flow runs in a real browser we can no longer
intercept navigation, so the token must be *delivered* to the app. That requires a small
companion change in the `agents` repo (web-api). There is precedent: the web app's
`/cli-auth` page already forwards CLI tokens to `http://127.0.0.1:<port>/callback`.

## Design: loopback redirect (RFC 8252 §7.3)

Chosen over a custom URL scheme (deep link) because (a) any macOS app can claim a URL
scheme, making scheme-based token delivery interceptable; (b) loopback works identically
in dev (unbundled binary) and prod; (c) the codebase already has the loopback pattern.

### Desktop (oats)

`google_sign_in` (src-tauri/src/commands.rs):

1. Bind `tokio::net::TcpListener` on `127.0.0.1:0` (ephemeral port, loopback only).
   Binding *before* prepare-state means no other process can take the port.
2. Generate a random `nonce` (32 hex chars, OS RNG).
3. `POST /oauth2/prepare-state` with
   `redirect: "/desktop-auth?callback_port=<port>&nonce=<nonce>"` (plus the existing
   `integration`/`scopes`/`newUserSignupIntent` fields).
4. Validate the returned `redirectUrl` is `https:` (dev `http://localhost` exempt) and
   open it with the system opener (default browser). No webview window is created.
5. Accept loopback connections until a request matches
   `GET /callback?token=…&nonce=…` **with the expected nonce**; other requests get 404.
   Reply with a static "You're signed in — return to oats" HTML page that never echoes
   the token, then exchange the magic-link token via the existing
   `exchange_token_for_session` (`GET /auth/check`) and emit `oauth-result`.
6. Failure states: 5-minute timeout → `oauth-result` error; a new `cancel_google_sign_in`
   command aborts a pending flow silently (frontend treats it like the old
   "Auth window closed" cancel); starting a new sign-in aborts any pending one.

Frontend: `auth.googleSignIn()` contract unchanged (invoke + `oauth-result` event).
Onboarding/Settings show "finish signing in in your browser" copy with a Cancel button
while waiting.

### Server (agents repo, web-api)

`redirectWithMagicLink` (handlers/signin_shared.ts): when the stored `redirect` matches
`/desktop-auth?callback_port=<1-65535>&nonce=<hex>` and there is no
`overrideTarget`, respond with a redirect to
`http://127.0.0.1:<port>/callback?token=<magic-link-token>&nonce=<nonce>` instead of the
web magic-link page.

Riding on the existing `redirect` field (rather than a new prepare-state field) means the
desktop callback survives every flow that already threads `redirect` through its state
data (existing users, and the desktop's `personal_unless_domain_autojoin` new-user path).

### Known limitation

Sign-ins that detour into web onboarding wizards (existing user with no membership,
company-domain auto-join, placeholder users) complete in the browser via a client-side
`/magic-link` navigation that bypasses `redirectWithMagicLink`, so the desktop app times
out. Recovery: the user finishes onboarding in the browser and clicks "Sign in" in oats
again — now an existing user, they take the fast path. The old webview flow had the
equivalent dead-end (the wizard ran inside the popup).

## Security notes

- **CSRF / login-fixation on the loopback**: a drive-by web page or local process could
  hit `http://127.0.0.1:<port>/callback` with an attacker token. The single-use random
  nonce (never exposed to other origins; round-trips through server state) makes forged
  callbacks unmatchable; non-matching requests do not consume the listener.
- Listener binds loopback only, accepts exactly one matching callback, and is torn down
  on completion/timeout/cancel.
- The magic-link token is never logged and never appears in the HTML response.
- Server-side, the loopback redirect host is hardcoded `127.0.0.1`; port must parse as
  1–65535; nonce must be hex ≤ 64 chars. No open redirect is introduced.
- Opened URL is validated (https) before handing to the system opener.
- Capabilities: no new permissions; the `oauth` webview window goes away.
- Post-review hardening (security audit, 2026-07-17): nonce comparison goes through
  SHA-256 digests so comparison timing can't leak the nonce to a local process; the
  per-connection budget is 2s so stalled local connections can't starve the real
  callback within the 5-minute window; the pending-flow handle is stored under the same
  lock that aborts its predecessor so overlapping invokes can't leak a listener.
- The delivered magic-link token expires server-side after 10 minutes (the
  `newMagicLink` default), bounding its exposure in browser history. A follow-up worth
  considering: redirect a one-time code instead and let the app redeem `code + nonce`
  over TLS — a PKCE-equivalent upgrade that keeps the bearer token off the URL bar.

## Deployment ordering

The web-api change must deploy before an oats release ships this flow; until then the
browser lands on the web app signed in and oats times out with a clear retry path.
