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

> **Note:** the Cargo feature flags above (`dev-api` / `prod-api`) select the **Ariso server build target** and are independent of the runtime **transcription backend** (Ariso vs Local), which the user chooses in Settings.

## Local backend (on-device transcription)

The **Local** transcription backend transcribes recordings entirely on-device — no login, no upload. It uses a bundled Swift sidecar (`ariso-stt`) built on [FluidAudio](https://github.com/FluidInference/FluidAudio) (Parakeet TDT v3 ASR + Pyannote speaker diarization, CoreML on the Apple Neural Engine). After transcription it also generates meeting notes on-device with the [`mlx-community/gemma-3-1b-it-qat-4bit`](https://huggingface.co/mlx-community/gemma-3-1b-it-qat-4bit) LLM via [mlx-swift-lm](https://github.com/ml-explore/mlx-swift-lm), saved as `note.md` next to `transcript.md` (best-effort: a notes failure never fails the recording). **Requires Apple Silicon, macOS 14+.**

Build the sidecar before `tauri:build` / `tauri:dev` (these are build artifacts, not committed):

```bash
cd src-tauri/ariso-stt
swift build -c release
mkdir -p ../binaries
cp .build/release/ariso-stt ../binaries/ariso-stt-aarch64-apple-darwin

# The notes backend uses MLX (Metal). `swift build` CANNOT compile MLX's Metal
# shaders — only xcodebuild can — so build the metallib bundle separately and
# ship it next to the sidecar. Without it, notes fail at runtime with
# "Failed to load the default metallib".
xcodebuild build -scheme ariso-stt -configuration Release \
  -destination 'generic/platform=macOS' -derivedDataPath .xcode -skipMacroValidation
cp -R .xcode/Build/Products/Release/mlx-swift_Cmlx.bundle ../binaries/
```

Tauri ships `binaries/ariso-stt-aarch64-apple-darwin` next to the app as `ariso-stt` (`tauri.conf.json > bundle.externalBin`) and `binaries/mlx-swift_Cmlx.bundle` into `Contents/Resources/` (`bundle.resources`). At runtime the sidecar resolves the metallib from `mlx-swift_Cmlx.bundle` via its containing bundle's resources; the sidecar itself resolves next to the app executable or via the `ARISO_STT_BIN` env override (used in tests). Because `externalBin` is declared, `cargo build` / `cargo test` require the sidecar binary to be present — build it first on a fresh checkout. For `tauri:dev` (no `.app`), also copy the bundle next to the dev sidecar: `cp -R .xcode/Build/Products/Release/mlx-swift_Cmlx.bundle ../target/debug/`.

The sidecar contract (stdout carries only the result — transcript JSON, progress JSON-lines, or notes Markdown; all logs go to stderr):

- `ariso-stt --audio <path> --models <dir> --format json` → one `{language, durationSeconds, participants[], segments[]}` object.
- `ariso-stt download --models <dir>` → JSON-lines `{"type":"progress","fraction":F}` … then `{"type":"done"}`. Downloads the speech models — ASR (`0–0.66`) and diarizer (`0.66–1.0`) — into `<dir>`. The notes LLM is NOT downloaded here.
- `ariso-stt notes --transcript <path> --models <dir>` → meeting-notes Markdown on stdout. Loads the LLM from `<dir>/llm/gemma-3-1b-it-qat-4bit/` (a local directory; no network). The Rust app downloads that model directly from the project CDN (Cloudflare R2) — the published weights are HuggingFace Xet-backed, which the Swift HF client can't fetch.

Storage layout under `~/.ariso/`:

- `models/` — CoreML speech bundles (`asr/`, `diarizer/`) + `manifest.json` ready-marker; the notes LLM in `llm/gemma-3-1b-it-qat-4bit/` (with a `.complete` marker written only after a full download)
- `recordings/<utc-timestamp>/` — `recording.mp3`, `transcript.md`, `note.md` (meeting notes), `meta.json`

In Settings → **Transcription Backend**, switch to **Local**. The **On-device models** section installs each model independently: the speech voice model (ASR + diarizer, from the sidecar) downloads automatically, and the language model (for notes, from the project CDN) installs from its own **Install** button. Each shows a green tick when ready. Past local recordings appear in the tray **Library…** window.

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

## CI: Validation and Signed Releases

The `Desktop App` workflow runs on a self-hosted Mac runner (`[self-hosted, macOS, ARM64]`). It has two jobs:

| Trigger                                | Job        | What it does                                                                  |
| -------------------------------------- | ---------- | ----------------------------------------------------------------------------- |
| PR to `main`, push to `main`           | `validate` | `vite build` + `cargo check`. No signing secrets exposed.                     |
| Publishing a GitHub Release (tag `v*`) | `release`  | Runs after `validate`. Signs + notarizes with `--features prod-api`, publishes the signed DMG, updater tarball, and `latest.json` to Cloudflare R2 (`/desktop/...`), and appends an R2 download link to the GitHub Release. Gated by the `release` GitHub Environment (required reviewer + tag-scoped policy + scoped secrets). |

### One-time setup on the runner Mac

1. **Install the Developer ID Application certificate** into the login keychain:
   - From [Apple Developer → Certificates](https://developer.apple.com/account/resources/certificates/list), create a *Developer ID Application* cert, download the `.cer`, and double-click to add it to the **login** keychain.
   - Verify with `security find-identity -v -p codesigning` — note the quoted identity string (e.g. `Developer ID Application: Your Name (TEAMID)`). This is the value you'll put in `APPLE_SIGNING_IDENTITY` below.
2. **Keep the login keychain unlocked during builds.** The runner must be started by the logged-in user (default `./run.sh`, or `./svc.sh install` under your user account) so it inherits keychain access. If signing fails with `errSecAuthFailed`, the keychain is locked — log back in or run `security unlock-keychain ~/Library/Keychains/login.keychain-db`.

### signing packages for update

```SHELL
npx @tauri-apps/cli signer generate
```

### One-time setup in the repo

1. **Generate an app-specific password** at [appleid.apple.com → Sign-In and Security → App-Specific Passwords](https://appleid.apple.com).
2. **Create the `release` environment** at **Settings → Environments → New environment** → name `release`.
   - Add yourself under **Required reviewers** so signed builds pause for approval.
   - Optionally restrict **Deployment branches and tags** to `Selected branches and tags` → match tags `v*` (prevents accidental use from other refs).
3. **Add these secrets to the `release` environment** (not repo-level secrets):

   | Secret                   | Value                                                                                                |
   | ------------------------ | ---------------------------------------------------------------------------------------------------- |
   | `APPLE_SIGNING_IDENTITY` | The quoted identity string from step 1.1, e.g. `Developer ID Application: Your Name (TEAMID)`        |
   | `APPLE_ID`               | Apple ID email associated with your developer account                                                |
   | `APPLE_PASSWORD`         | App-specific password from the previous step (not your Apple ID password)                            |
   | `APPLE_TEAM_ID`          | 10-character Team ID from [developer.apple.com/account](https://developer.apple.com/account) → Membership |
   | `TAURI_SIGNING_PRIVATE_KEY`          | Ed25519 private key content (or a path to the key file) generated by `tauri signer generate` |
   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for that private key (leave empty if the key has no password) |
   | `R2_ACCESS_KEY_ID`                   | R2 API token Access Key ID (Cloudflare → R2 → Manage R2 API Tokens, scoped Object Read & Write) |
   | `R2_SECRET_ACCESS_KEY`               | R2 API token Secret Access Key |
   | `R2_ENDPOINT`                        | `https://<account-id>.r2.cloudflarestorage.com` |
   | `R2_BUCKET`                          | Bucket backing `pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev` |

   The release job publishes artifacts to the stable keys `desktop/latest.json`, `desktop/Ariso.app.tar.gz`, and `desktop/Ariso.dmg` (overwritten each release, served with `no-cache`). The updater endpoint in `tauri.conf.json` points at `desktop/latest.json`.

### Cutting a release

Releases are automated by [release-please](https://github.com/googleapis/release-please) (the `release` workflow). On every push to `main` it parses conventional commits (`feat:` → minor, `fix:` → patch, `feat!:`/`BREAKING CHANGE:` → major) and maintains a **Release PR** that bumps the version in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`, updates `CHANGELOG.md`, and (via the `sync-lock` job) keeps `package-lock.json` and `Cargo.lock` in sync.

1. **Merge feature/fix PRs to `main`** with conventional-commit messages. release-please keeps the Release PR up to date.
2. **Merge the Release PR** when ready to ship. release-please creates the `vX.Y.Z` tag and publishes the GitHub Release, which triggers the `Desktop App` workflow's signing pipeline.
3. **Approve the `release` environment gate** when the workflow pauses (per the required-reviewer rule). It then builds + signs + notarizes, publishes the artifacts to R2, and adds the R2 download link to the GitHub Release.

Manual fallback: `gh release create v0.2.1 --target main --generate-notes` still works (bump the three version files and lockfiles yourself first), but prefer the Release PR.

> **Note:** The `release` environment is restricted to tags matching `v*`. Publishing a release with a non-matching tag will fail the environment gate.

#### release-please setup

The workflow requires a **repo-level secret `RELEASE_PLEASE_TOKEN`** holding a PAT (or GitHub App token) with `contents: write` and `pull-requests: write` on this repo. The default `GITHUB_TOKEN` cannot be used: events it authors never trigger other workflows, so the GitHub Release it publishes would silently skip the `Desktop App` signing pipeline, and its lockfile pushes would leave the Release PR without `validate` checks.

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
