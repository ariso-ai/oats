---
name: oats-security
description: Use when changing oats code that touches auth/OAuth, session tokens, the capabilities allowlist, a Tauri invoke command, file paths, URL/deep-link opening, the sidecar, network calls, or the offline-mode privacy guarantee. Encodes this app's real attack surface and a pre-PR review checklist.
---

# oats Security Review

oats records meetings (highly sensitive audio + transcripts), holds an auth session, and
runs a native sidecar — so changes near the surfaces below deserve a deliberate security
pass. Use this alongside superpowers' `requesting-code-review` and the built-in
`/security-review` command. **Default to suspicion**: if a change touches a surface here,
walk its checklist before opening the PR.

## The attack surfaces (where they live)

1. **Tauri `invoke` boundary** — every `#[tauri::command]` in `src-tauri/src/commands.rs`
   is reachable from any loaded webview (incl. the external OAuth page). Treat command
   arguments as **untrusted input**.
2. **File paths from the frontend** — commands like `read_recording_audio` /
   note read-write take a path/id and hit `std::fs::read` / `read_to_string` /
   `create_dir_all`. **Path-traversal risk**: a malicious id (`../../`) could escape
   `~/.ariso/recordings/`. Confirm ids are validated and paths are built from
   `storage::recordings_dir(...)`, never concatenated from caller-supplied absolute paths.
3. **Auth / OAuth** — `auth.googleSignIn` → `/oauth2/prepare-state` (CSRF state),
   an `oauth` **external** webview (`WebviewUrl::External`), result delivered via the
   `oauth-result` event. Verify: state is generated server-side and checked; the external
   window is scoped to the expected origin; the returned `sessionToken` is never logged or
   emitted to other windows.
4. **Session token storage** — tokens/Bearer creds back the `http_client()`
   (`AUTHORIZATION` header). The `@tauri-apps/plugin-store` (`settings.json`) is
   **plaintext on disk** — fine for prefs (`backend`, `onboarded`), **not** for secrets.
   Confirm no token/secret is written to plugin-store; secrets belong in the OS keychain
   or backend-managed session.
5. **Capabilities allowlist** — `src-tauri/capabilities/default.json` is least-privilege
   and should stay that way. Note the scoped `opener:allow-open-url` (only
   `x-apple.systempreferences:*`). **Don't broaden a permission or add a window** without
   justifying it; never add a wildcard URL/open scope.
6. **URL / deep-link opening** — `app.opener().open_url(...)` and notification deep links
   (`meeting_notifications.rs` carries a URL as the request id). Only open URLs whose
   **origin you control** (the `web.ari.ariso.ai` web app). Validate scheme + host before
   opening anything derived from server data or a notification payload.
7. **Sidecar execution** — `ariso-stt` is spawned with `--audio <path>` etc. It uses
   `Command::new` + `.arg(...)` (no shell), so keep it that way — **never** route sidecar
   args through a shell string or interpolate untrusted data into one.
8. **Network / API base** — `DEFAULT_API_BASE_URL` is compiled per feature; release builds
   **intentionally ignore env overrides** (`ARISO_DESKTOP_API_BASE_URL`). Don't add a
   runtime override path that would let an attacker repoint the API. Keep `https://` in
   non-local builds; `http://localhost` is dev-only.
9. **Updater** — R2-hosted, signature-verified with the public key. Updates must stay
   **signature-verified**; never disable the pubkey check or pull updates over plain HTTP.
10. **Offline-mode privacy invariant** — when the Local backend is selected, **nothing
    leaves the machine**. Any new network call (`reqwest`, telemetry, fetch) on a
    code path reachable in Local mode breaks the product's core promise. Gate network I/O
    behind the cloud backend explicitly.
11. **Sensitive data in logs** — transcripts, audio paths, tokens, and meeting content
    must not land in stdout/stderr/log files. The sidecar contract is "stdout = result,
    stderr = logs" — keep secrets out of both.

## Pre-PR checklist

For a change touching any surface above, confirm:

- [ ] New/changed `invoke` command validates and bounds every argument (esp. paths/ids).
- [ ] No secret (token, key) written to plugin-store or any plaintext file; none logged.
- [ ] No capability/permission/window added or widened without a written reason; no
      wildcard URL open scope.
- [ ] Any opened URL/deep link is origin- and scheme-checked against a trusted host.
- [ ] No new network call reachable in Local (offline) mode.
- [ ] Sidecar args stay argv-based (no shell), no untrusted interpolation.
- [ ] Updater signature verification and HTTPS endpoints untouched.
- [ ] Ran `/security-review` (or requested review) on the diff.

When this surfaces a real finding, fix it via `oats-tauri` (backend) / `oats-vue`
(frontend) and re-verify. For runtime reproduction, use `oats-debugging`.
