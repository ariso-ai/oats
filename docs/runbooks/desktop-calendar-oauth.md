# Desktop Calendar OAuth Runbook

Use this runbook to verify that Sage desktop sign-in asks for Google Calendar
permissions and that the Agents API stores usable Google Workspace credentials.

## Prerequisites

- Sage worktree: `/Users/michaelgeiger/Developer/repos/worktrees/sage/sturdy-dune/sage`
- Isolated Agents worktree: `/Users/michaelgeiger/Developer/repos/worktrees/sage/sturdy-dune/agents`
- Desktop launcher: `/Users/michaelgeiger/desktop-dev.sh`
- Google OAuth env configured in the Agents stack.

## Start From a Fresh Desktop State

```bash
cd /Users/michaelgeiger/Developer/repos/worktrees/sage/sturdy-dune/sage
~/desktop-dev.sh --fresh-install --with-agents-platform
```

The launcher should start one tmux session named `ariso-desktop-dev` with:

- `desktop` window: Sage/Tauri desktop app.
- `agents-platform` window: isolated Agents platform stack, started from
  `/Users/michaelgeiger/Developer/repos/worktrees/sage/sturdy-dune/agents` with
  `WORKSPACE_ID=agents-desktop ~/dev.sh --reset --ngrok .`.

`desktop-dev.sh` waits for the Agents stack to publish its resolved
`WEB_APP_BASE_URL` and then exports:

- `ARISO_DESKTOP_WEB_APP_BASE_URL=<resolved Agents web origin>`
- `ARISO_DESKTOP_API_BASE_URL=<resolved Agents web origin>/api`

When ngrok is available, the resolved origin is the ngrok URL. Otherwise it
falls back to the Tailscale or OrbStack URL chosen by `~/dev.sh`.

Attach any time:

```bash
tmux attach -t ariso-desktop-dev
```

## Consent Screen Check

In the fresh desktop onboarding window:

1. Click **Sign in with Google**.
2. Choose the test Google account.
3. On the Google consent screen, confirm the permissions include Calendar access.
4. Click **Allow**.

Expected:

- The consent page includes identity permissions and Google Calendar permissions.
- The OAuth window closes after the final magic-link redirect.
- The desktop onboarding/settings UI should no longer show the user as signed out.

## Log Checks

In tmux, inspect the `desktop` and `agents-platform` windows.

Expected Agents-side signals:

- No `invalid_scopes` response from `/oauth2/prepare-state`.
- No `Error handling Google sign-in callback`.
- If Calendar webhook creation succeeds, Calendar sync should be queued.

Webhook registration can fail in some local stacks if the callback URL is not
externally reachable. That should log an error but should not break sign-in or
credential storage.

## API Checks

After sign-in, use the session token from desktop state or an authenticated web
session and check Google Workspace credentials:

```bash
curl "$ARISO_DESKTOP_API_BASE_URL/integration/credentials/check?integration_name=googleWorkspace&source_id=<google_workspace_server_id>" \
  -H "Authorization: Bearer <session_token>"
```

Expected:

- `hasCredentials` is `true`.
- `scopeStatus.calendar` is `partial` or `full`.
- `owner_id` is the Google account email used during consent.

## Failure Guide

- If the consent screen only shows profile/email permissions, Sage is not
  sending the new `scopes` body or the Agents API is not running the patched
  `google-signin` branch.
- If consent shows Calendar but credentials are missing, inspect
  `apps/web-api/src/handlers/google_signin.ts` callback logs.
- If credentials exist but Calendar webhook fails, verify `API_BASE_URL` is
  reachable by Google webhook delivery. Local-only URLs can block webhook setup.
- If Google does not return a refresh token, revoke the app in the Google
  account permissions page and rerun with `~/desktop-dev.sh --fresh-install
  --with-agents-platform`.

## Relevant Code

- Sage request body:
  `/Users/michaelgeiger/Developer/repos/worktrees/sage/sturdy-dune/sage/src-tauri/src/commands.rs`
- Agents prepare-state:
  `/Users/michaelgeiger/Developer/repos/worktrees/sage/sturdy-dune/agents/apps/web-api/src/handlers/oauth_prepare_state.ts`
- Agents sign-in callback:
  `/Users/michaelgeiger/Developer/repos/worktrees/sage/sturdy-dune/agents/apps/web-api/src/handlers/google_signin.ts`
