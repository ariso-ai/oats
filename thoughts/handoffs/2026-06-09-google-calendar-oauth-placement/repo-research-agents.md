# Repo Research: Agents Google Calendar OAuth Placement

## Question

Should the desktop Google sign-in Calendar permission work live in the Sage desktop repo or the Agents platform repo?

## Checkout Test

The isolated Agents checkout command was tested from the Sage worktree:

```bash
~/desktop-dev.sh checkout-agents
```

Result:

- Reused existing isolated Agents worktree.
- Checkout path: `/Users/michaelgeiger/Developer/repos/worktrees/sage/sturdy-dune/agents`
- Branch: `codex/desktop-platform-dev`
- Status: tracking `origin/main`

## Ownership Recommendation

Put the OAuth scope and credential behavior in Agents, not Sage.

Sage should only select which backend OAuth flow to start and pass any UX-level redirect or service-selection data. Agents owns `/oauth2/prepare-state`, Google OAuth callbacks, access/refresh token storage, granted-scope status, Calendar webhook registration, and initial Calendar sync.

## Evidence

### Sage Desktop

`/Users/michaelgeiger/Developer/repos/worktrees/sage/sturdy-dune/sage/src-tauri/src/commands.rs`

- `api_base_url()` resolves the API origin, including the `ARISO_DESKTOP_API_BASE_URL` override.
- `google_sign_in()` posts to `{api_base_url()}/oauth2/prepare-state`.
- The request body is currently `{"integration":"google-signin"}`.

This means Sage requests an integration name; it does not define the Google permission list.

### Agents Sign-In Flow

`/Users/michaelgeiger/Developer/repos/worktrees/sage/sturdy-dune/agents/apps/web-api/src/handlers/oauth_prepare_state.ts`

- The `google-signin` branch hard-codes identity scopes only:
  - `openid`
  - `https://www.googleapis.com/auth/userinfo.email`
  - `https://www.googleapis.com/auth/userinfo.profile`
- It stores state with `flow: 'signin'`.
- It redirects to `/oauth2/google-signin-callback`.

`/Users/michaelgeiger/Developer/repos/worktrees/sage/sturdy-dune/agents/apps/web-api/src/handlers/google_signin.ts`

- The callback validates the OAuth code and decodes the ID token for email.
- It creates or resolves the user and org membership.
- It creates a magic link and redirects to the web app.
- It does not persist Google Workspace credentials, does not store granted scopes, and does not register Calendar webhooks.

Therefore, adding Calendar scope to `google-signin` alone would change the consent screen but would not make Calendar usable by Ari unless this callback is also changed to store Google credentials.

### Agents Google Workspace Flow

`/Users/michaelgeiger/Developer/repos/worktrees/sage/sturdy-dune/agents/apps/web-api/src/handlers/oauth_prepare_state.ts`

- The `googleWorkspace` branch delegates to `integrationService.prepareGoogleOAuth2State(...)`.
- It accepts `serverId`, `redirect`, and optional `scopes` from the caller.

`/Users/michaelgeiger/Developer/repos/worktrees/sage/sturdy-dune/agents/services/integration-service/src/service.ts`

- `prepareGoogleOAuth2State(...)` defaults to Google Workspace services including `calendar`.
- It converts requested services to scopes via `getScopesForServices(...)`.
- It requests offline access, granular consent, granted-scope inclusion, and `prompt=consent`.
- It stores state with org/user/server context for the callback.

`/Users/michaelgeiger/Developer/repos/worktrees/sage/sturdy-dune/agents/apps/web-api/src/handlers/google_oauth.ts`

- The callback stores access and refresh tokens as `integration_name: 'googleWorkspace'`.
- It stores `tokens.scopes()`.
- It registers a Google Calendar webhook.
- It schedules an initial Calendar sync.
- It redirects to `/my/integrations` or the provided redirect, with `/set-up` support.

This is the actual Calendar-capable OAuth path.

## Best Implementation Shape

Prefer an Agents backend change that lets desktop first-run onboarding use the existing Google Workspace capability instead of widening `google-signin` directly.

Good options:

1. Add a desktop/onboarding-oriented alias in Agents, such as `google-signin-calendar`, that delegates to the Google Workspace OAuth flow and redirects back to the setup/onboarding path.
2. Let Sage request `integration: 'googleWorkspace'` with a Calendar/read-only service selection, once Agents can discover or accept the correct Google Workspace MCP server for desktop-first users.
3. If sign-in and Calendar connection must happen in one click, update Agents so the sign-in callback can also persist Google Workspace credentials and run the existing Calendar webhook/sync path. This is higher risk because it merges authentication and integration-connect semantics.

Sage changes should stay small:

- Choose the supported integration name or payload.
- Pass `redirect: '/set-up'` if needed.
- Keep API/web base URL selection in `~/desktop-dev.sh` and `api_base_url()`.
- Update onboarding/settings copy only after the backend flow is clear.

## Verification Notes

No product code was changed as part of this research pass. The checkout command was tested; full Agents verification was not run because this was a placement/research task.
