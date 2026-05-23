# @ariso-ai/desktop

Tauri v2 desktop app for Ariso. Built with [Tauri](https://v2.tauri.app/), [Vite](https://vite.dev/), and [Vue 3](https://vuejs.org/).

This workspace is excluded from the monorepo's Turbo `build`/`lint`/`test` pipeline.

## Prerequisites

- [Rust](https://rustup.rs/) toolchain
- From the monorepo root: `npm install`
- `DEEPGRAM_API_KEY` in `.env` — must have **Member** role or higher (required for `/v1/auth/grant` token provisioning)

## Scripts

```bash
# Development (hot-reload frontend + Rust backend, uses localhost:4000)
npm run tauri:dev

# Debug mode (enables MCP plugin + disables audio filters for loopback testing)
npm run tauri:dev:debug

# Build (compile frontend + Rust, produce distributable)
npm run tauri:build

# Build targeting dev API (https://api-dev.ari.ariso.ai)
npm run tauri:build -- -- --features dev-api

# Build targeting prod API (https://api.ari.ariso.ai)
npm run tauri:build -- -- --features prod-api
```

The API endpoint is controlled by Cargo feature flags and baked into the binary at compile time:

| Feature    | API endpoint                        |
|------------|-------------------------------------|
| *(default)* | `http://localhost:4000`             |
| `dev-api`  | `https://api-dev.ari.ariso.ai`      |
| `prod-api` | `https://api.ari.ariso.ai`          |

Debug mode sets `VITE_DEBUG_AUDIO=true`, which disables echo cancellation and noise suppression. This allows testing transcription with virtual audio devices (e.g., BlackHole) and the `say` command.

## Testing Transcription with a Virtual Audio Device

To test recording without a real microphone, route system audio back as mic input using an aggregate device.

### Setup (one-time)

1. Install [BlackHole](https://existential.audio/blackhole/) (virtual audio driver):
   ```bash
   brew install blackhole-2ch
   ```
2. Open **Audio MIDI Setup** (`/Applications/Utilities/Audio MIDI Setup.app`).
3. Click **+** in the bottom-left and create a **Multi-Output Device** that includes both your speakers (or headphones) and **BlackHole 2ch**. This lets you hear audio while it is also captured.
4. Set your **system output** to the Multi-Output Device (System Settings > Sound > Output, or Option-click the menu bar volume icon).
5. Set your **system input** to **BlackHole 2ch** (System Settings > Sound > Input).

### Running a test

```bash
# Start the app in debug mode (disables echo cancellation)
npm run tauri:dev:debug

# In another terminal, play test audio
say -v Samantha "Hello everyone, welcome to today's standup."
```

Click **Start Recording** in the app before (or while) the `say` command is playing. The transcript should appear in real time.

> **Note:** Debug mode is required because the browser's echo cancellation and noise suppression filters strip out loopback audio. These filters are only disabled in dev builds when `VITE_DEBUG_AUDIO=true`.

## Output

| Path                | Contents                                      |
| ------------------- | --------------------------------------------- |
| `dist/`             | Compiled frontend (Vite output)               |
| `src-tauri/target/` | Rust build artifacts and packaged app bundles |

Both directories are git-ignored.

## Signing & Notarization (CI)

The `Desktop App` workflow can produce a signed + notarized `.app`/`.dmg` for direct distribution. Trigger it from **Actions → Desktop App → Run workflow** (it does not run on PRs or main pushes — those only validate the frontend build).

### Required repo secrets

Add these under **Settings → Secrets and variables → Actions**:

| Secret                       | Value                                                                                          |
| ---------------------------- | ---------------------------------------------------------------------------------------------- |
| `APPLE_CERTIFICATE`          | Base64-encoded `.p12` of your **Developer ID Application** certificate                         |
| `APPLE_CERTIFICATE_PASSWORD` | Password you set when exporting the `.p12`                                                     |
| `APPLE_SIGNING_IDENTITY`     | Identity string, e.g. `Developer ID Application: Your Name (TEAMID)`                           |
| `APPLE_ID`                   | Apple ID email associated with your developer account                                          |
| `APPLE_PASSWORD`             | App-specific password generated at [appleid.apple.com](https://appleid.apple.com) (not your Apple ID password) |
| `APPLE_TEAM_ID`              | 10-character Team ID from [developer.apple.com/account](https://developer.apple.com/account) → Membership |

### One-time setup steps

1. **Create a Developer ID Application certificate** in [Apple Developer → Certificates](https://developer.apple.com/account/resources/certificates/list). Download and double-click to install into the login keychain.
2. **Export it as a `.p12`** from Keychain Access (right-click the cert → Export). Set a password — this becomes `APPLE_CERTIFICATE_PASSWORD`.
3. **Base64-encode the `.p12`** for `APPLE_CERTIFICATE`:
   ```bash
   base64 -i DeveloperID.p12 | pbcopy
   ```
4. **Find the signing identity name** for `APPLE_SIGNING_IDENTITY`:
   ```bash
   security find-identity -v -p codesigning
   ```
   Use the quoted `Developer ID Application: …` string.
5. **Generate an app-specific password** at [appleid.apple.com → Sign-In and Security → App-Specific Passwords](https://appleid.apple.com). This is `APPLE_PASSWORD`.

The workflow's `features` input controls which API endpoint is baked into the binary (`prod-api`, `dev-api`, or `default` for localhost). The signed bundle is uploaded as a workflow artifact and retained for 14 days.

## Troubleshooting

### `tauri:build` fails with `Cannot find module '.../node_modules/dist/node/cli.js'`

If `npm run tauri:build` (or `vite build`) fails with:

```
Error [ERR_MODULE_NOT_FOUND]: Cannot find module
'.../node_modules/dist/node/cli.js' imported from '.../node_modules/.bin/vite'
```

the `node_modules/.bin/vite` entry has been installed as a regular file copy instead of a symlink. Vite's launcher does `import('../dist/node/cli.js')`, which only resolves correctly when `.bin/vite` is a symlink into `node_modules/vite/bin/`. When it's a copy, the relative path resolves to the nonexistent `node_modules/dist/node/cli.js`.

Fix by replacing the file with a symlink:

```bash
rm node_modules/.bin/vite
ln -s ../vite/bin/vite.js node_modules/.bin/vite
```

If `npm install` keeps re-copying instead of symlinking, `npm rebuild vite` should restore the symlink as well.

## **Architecture: WebSocket Streaming**

### Phase 1 — Direct Deepgram Connection

```
┌─────────────┐                       ┌──────────┐
│   Client    │── Audio (WebSocket) ─>│ Deepgram │
│             │<─ Transcripts (WS) ───│          │
└──────┬──────┘                       └──────────┘
       │
       │── HTTP (REST) ──> ┌──────────┐
       │                   │ web-api  │──Store──> Database
       └───────────────────└──────────┘
```

The client connects directly to Deepgram for real-time audio streaming. Meeting lifecycle (start, list, pause, terminate) is managed via HTTP API calls to `web-api`, which persists meeting metadata and transcripts.

### Phase 2 — Ariso Relay

```
┌─────────────┐                       ┌───────────────┐
│   Client    │── Audio (WebSocket) ─>│ Ariso Relay   │
│             │<─ Results (WebSocket)─│               │
└─────────────┘                       └──────┬────────┘
                                            │
                                            ├──WebSocket──> Deepgram
                                            │
                                            └──Store──> Database
```

Introduces the Ariso Relay as a WebSocket proxy between the client and Deepgram. The relay persists transcripts to the database and streams results back over the same WebSocket connection.
