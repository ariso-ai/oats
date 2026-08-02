# Contributing to oats

Thanks for your interest in contributing to **oats**! <img src="src/assets/oats-dark.png" alt="oats icon" width="16" height="16" valign="middle" /> This guide covers everything you need to develop, build, test, and release the app.

oats is a [Tauri v2](https://v2.tauri.app/) desktop app built with [Vite](https://vite.dev/) and [Vue 3](https://vuejs.org/) on the frontend and Rust on the backend. It is part of the Ariso monorepo and is excluded from the monorepo's Turbo `build`/`lint`/`test` pipeline.

Much of oats was built with [Claude Code](https://claude.com/claude-code) and the [Superpowers](https://github.com/obra/superpowers) plugin, and we lean on the same tooling day to day — contributions developed with it are very welcome.

## Table of contents

- [Code of conduct](#code-of-conduct)
- [Ways to contribute](#ways-to-contribute)
- [Working with Claude Code](#working-with-claude-code)
- [Prerequisites](#prerequisites)
- [Development scripts](#development-scripts)
- [API build targets vs. transcription backend](#api-build-targets-vs-transcription-backend)
- [Local backend (on-device transcription)](#local-backend-on-device-transcription)
- [Testing transcription with a virtual audio device](#testing-transcription-with-a-virtual-audio-device)
- [Build output](#build-output)
- [Submitting changes](#submitting-changes)
- [Commit conventions](#commit-conventions)
- [CI: validation and signed releases](#ci-validation-and-signed-releases)
- [Cutting a release](#cutting-a-release)
- [Troubleshooting](#troubleshooting)

## Code of conduct

Be kind, be constructive, and assume good intent. We want oats to be a welcoming project for contributors of all backgrounds.

## Ways to contribute

- 🐛 **Report bugs** — open an [issue](https://github.com/ariso-ai/oats/issues) with steps to reproduce, your OS version, and which transcription backend you were using.
- 💡 **Suggest features** — open an issue describing the problem you'd like solved.
- 🔧 **Send pull requests** — fix a bug, improve docs, or build a feature. See [Submitting changes](#submitting-changes).

## Working with Claude Code

This repo is set up for [Claude Code](https://claude.com/claude-code). On clone, trust
the folder and you get the **superpowers** plugin plus five repo-specific skills
(`oats-architecture`, `oats-vue`, `oats-tauri`, `oats-security`, `oats-debugging`) that
encode our conventions. See [`CLAUDE.md`](CLAUDE.md) for the full guide. Plugin/skill
config lives in `.claude/` and `.claude-plugin/`.

## Prerequisites

- [Rust](https://rustup.rs/) toolchain
- Node.js + npm
- From the monorepo root: `npm install`
- **macOS Local backend:** [Xcode](https://apps.apple.com/app/xcode/id497799835) (full install, not just Command Line Tools) and **Apple Silicon, macOS 14+**. `xcodebuild` is required to compile the sidecar's MLX Metal shaders (`mlx-swift_Cmlx.bundle`); `swift build` alone cannot.
- **Windows Local backend:** Windows 11, Visual Studio Build Tools with the C++ workload, and the `x86_64-pc-windows-msvc` Rust target. The build helper discovers the newest compatible Visual Studio installation with `vswhere`.

## Development scripts

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

# Run the frontend test suite
npm test
```

## API build targets vs. transcription backend

The API endpoint is controlled by Cargo feature flags and baked into the binary at compile time:

| Feature     | API endpoint                   |
| ----------- | ------------------------------ |
| *(default)* | `http://localhost:4000`        |
| `dev-api`   | `https://api-dev.ari.ariso.ai` |
| `prod-api`  | `https://api.ari.ariso.ai`     |

Debug mode sets `VITE_DEBUG_AUDIO=true`, which disables echo cancellation and noise suppression. This allows testing transcription with virtual audio devices (e.g., BlackHole) and the `say` command.

> **Note:** the Cargo feature flags above (`dev-api` / `prod-api`) select the **Ariso server build target** and are independent of the runtime **transcription backend** (Ariso vs Local), which the user chooses in Settings.

## Local backend (on-device transcription)

The **Local** transcription backend transcribes recordings entirely on-device — no login, no upload. On macOS it uses a bundled Swift sidecar (`ariso-stt`) built on [FluidAudio](https://github.com/FluidInference/FluidAudio) (Parakeet TDT v3 ASR + Pyannote speaker diarization, CoreML on the Apple Neural Engine). After transcription it also generates meeting notes on-device with the [`mlx-community/gemma-3-1b-it-qat-4bit`](https://huggingface.co/mlx-community/gemma-3-1b-it-qat-4bit) LLM via [mlx-swift-lm](https://github.com/ml-explore/mlx-swift-lm), saved as `ari-note.md` next to `transcript.md` (best-effort: a notes failure never fails the recording).

On Windows, the same contract is implemented by the Rust `src-tauri/ariso-stt/windows` target using Parakeet and speaker diarization through sherpa-onnx, plus Gemma GGUF through llama.cpp. The macOS Swift target lives in `src-tauri/ariso-stt/macos`, while language-neutral contract artifacts live in `src-tauri/ariso-stt/shared`. Both targets are inference-only: the Tauri host downloads immutable, hash-pinned model data directly from Cloudflare R2 and writes the same readiness markers on both platforms. Windows executable runtime files are pinned in `shared/windows-models.json` and packaged as Tauri installer resources.

Build the sidecar before `tauri:build` / `tauri:dev` (these are build artifacts, not committed):

```bash
cd src-tauri/ariso-stt/macos
swift build -c release
mkdir -p ../../binaries
cp .build/release/ariso-stt ../../binaries/ariso-stt-aarch64-apple-darwin

# The notes backend uses MLX (Metal). `swift build` CANNOT compile MLX's Metal
# shaders — only xcodebuild can — so build the metallib bundle separately and
# ship it next to the sidecar. Without it, notes fail at runtime with
# "Failed to load the default metallib".
xcodebuild build -scheme ariso-stt -configuration Release \
  -destination 'generic/platform=macOS' -derivedDataPath .xcode -skipMacroValidation
cp -R .xcode/Build/Products/Release/mlx-swift_Cmlx.bundle ../../binaries/
```

Tauri ships `binaries/ariso-stt-aarch64-apple-darwin` next to the app as `ariso-stt` (`tauri.conf.json > bundle.externalBin`) and `binaries/mlx-swift_Cmlx.bundle` into `Contents/Resources/` (`bundle.resources`). At runtime the sidecar resolves the metallib from `mlx-swift_Cmlx.bundle` via its containing bundle's resources. Because `externalBin` is declared, `cargo build` / `cargo test` require the sidecar binary to be present — build it first on a fresh checkout. For `tauri:dev` (no `.app`), also copy the bundle next to the dev sidecar: `cp -R .xcode/Build/Products/Release/mlx-swift_Cmlx.bundle ../../target/debug/`.

For Windows validation, build the sidecar into Tauri's expected target-specific name:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-windows-sidecar.ps1
```

The Windows helper also stages the pinned llama.cpp runtime under Tauri's
resources. It does not download model data.

The sidecar contract (stdout carries only transcript JSON or notes Markdown; all logs go to stderr):

- `ariso-stt --audio <path> --models <dir> --format json` → one `{language, durationSeconds, segments[]}` object whose segment `speaker` values are raw strings. The Tauri host sorts segments and creates numeric speaker IDs and participants for storage.
- `ariso-stt notes --transcript <path> --models <dir>` → meeting-notes Markdown on stdout. Loads the platform-native Gemma model from `<dir>` with no network access.

Storage layout under `~/.ariso/`:

- `models/` — platform-native speech and notes bundles plus their readiness markers; `manifest.json` marks speech ready and a versioned `.complete` file marks notes ready
- `recordings/<utc-timestamp>/` — `recording.mp3`, `transcript.md`, `ari-note.md` (meeting notes), `meta.json`

In Settings → **Transcription Backend**, switch to **Local**. The **On-device models** section installs speech (ASR + diarizer) and notes independently from the project R2 CDN. Each shows a green tick after the host has verified every file and written its completion marker. Past local recordings appear in the tray **Library…** window.

## Testing transcription with a virtual audio device

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

## Build output

| Path                | Contents                                      |
| ------------------- | --------------------------------------------- |
| `dist/`             | Compiled frontend (Vite output)               |
| `src-tauri/target/` | Rust build artifacts and packaged app bundles |

Both directories are git-ignored.

## Submitting changes

1. Fork the repo and create a feature branch.
2. Make your change, with a focused scope.
3. Run `npm test` and make sure the app builds (`npm run tauri:build`).
4. Commit using [conventional commit](#commit-conventions) messages.
5. Open a pull request against `main`. CI (`Desktop App`) will validate it.

## Commit conventions

Releases are automated by [release-please](https://github.com/googleapis/release-please), which parses [Conventional Commits](https://www.conventionalcommits.org/):

- `feat:` → minor version bump
- `fix:` → patch version bump
- `feat!:` / `BREAKING CHANGE:` → major version bump

Use these prefixes so the changelog and version bumps stay accurate.

## CI: validation and signed releases

Three workflows validate and package the desktop targets:

- **`Desktop App`** — CI validation. Its `macos-15` and `windows-latest` matrix builds the frontend, platform sidecar, and Tauri host without exposing signing secrets.
- **`Release`** — the **macOS** release pipeline on push to `main` (see [Cutting a release](#cutting-a-release)).
- **`Release (Windows)`** — the **Windows** release pipeline, dispatched by `Release` and re-runnable on its own.

The two platforms ship independently: they build on different runners, hold different signing credentials, and publish separate R2 objects. An eSigner/Authenticode failure therefore cannot hold back a macOS build that already signed and notarized — which is exactly what used to happen when both lived in one workflow.

**`Release`** (`.github/workflows/release.yaml`):

| Job                | Runs when                        | What it does                                                                                                       |
| ------------------ | -------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `release-please`   | every push to `main`             | Maintains the Release PR / cuts the GitHub Release + tag. Uses the default `GITHUB_TOKEN`.                          |
| `sync-lock`        | a Release PR was created/updated | Keeps `package-lock.json` and `Cargo.lock` in sync with the bumped version on the Release PR branch.                |
| `dispatch-windows` | a GitHub Release was just cut    | `gh workflow run release-windows.yaml --ref main -f tag=<tag>`. Does not wait on it and cannot be failed by it.     |
| `release`          | a GitHub Release was just cut    | Signs + notarizes the macOS app with `--features prod-api`. Uses the `release` GitHub Environment.                  |
| `publish`          | after `release`                  | Uploads the immutable macOS payload, the DMG alias, and `latest-darwin-aarch64.json`. Uses the `release` environment. |

**`Release (Windows)`** (`.github/workflows/release-windows.yaml`), `workflow_dispatch` only, with a required `tag` input:

| Job                           | Runs when                  | What it does                                                                                                                        |
| ----------------------------- | -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `package-windows-authenticode`| **default** (`sign`)       | Builds x64 NSIS + MSI and has Tauri route every PE through SSL.com eSigner, then verifies the result against the signing audit. Uses the `windows-release` Environment. |
| `package-windows`             | only with `-f sign=false`  | Escape hatch for shipping while eSigner is down: the same installers, **unsigned**. Needs no signing credentials.                       |
| `sign-windows-updater`        | after whichever ran        | Generates the Tauri `.sig` from the final NSIS bytes and independently verifies minisign. Uses `release`.                               |
| `publish-windows`             | unless `skip_publish`      | Uploads the immutable Windows payload, the `.exe` alias, and `latest-windows-x86_64.json`, and attaches the installers to the Release. Uses `release`. |

The two packaging jobs are mutually exclusive and upload the **same artifact name** (`windows-installers`), so everything downstream is identical whichever one ran. `sign-windows-updater` therefore depends on both and proceeds when either succeeded — without that explicit `if`, a skipped dependency would skip the whole publish chain while the run still reported success.

Dispatch it directly against an existing tag to re-run or debug without cutting a throwaway release:

```shell
# Full run, including the R2 publish.
gh workflow run release-windows.yaml --ref main -f tag=v0.18.0

# Dry run: build, sign, updater-sign and verify, but touch neither R2 nor the
# GitHub Release. The signed installers are still uploaded to the run as the
# `windows-release-bundle` artifact, alongside `windows-signing-audit` — the
# record of every PE Tauri routed through signCommand.
gh workflow run release-windows.yaml --ref main -f tag=v0.18.0 -f skip_publish=true

# Emergency unsigned build, when eSigner is down and a release cannot wait.
gh workflow run release-windows.yaml --ref main -f tag=v0.18.0 -f sign=false

# Build a branch's scripts while still labelling the release from `tag`. Without
# `ref`, a dispatch always tests the *tag's* copy of the signing scripts.
gh workflow run release-windows.yaml --ref main -f tag=v0.18.0 -f ref=my-branch
```

Always pass `--ref main`: the `release` and `windows-release` environments restrict deployments to the protected `main` branch. Note that `--ref` selects the workflow **file** while `ref`/`tag` selects the **scripts** it runs.

Because the signing job deploys to `windows-release`, which has required reviewers, **a release now pauses for approval** before Windows packaging starts. Drop that protection rule if you would rather releases run unattended.

No PAT is required. The macOS build runs as downstream jobs in the same `Release` run, and `workflow_dispatch` is one of the two events the default `GITHUB_TOKEN` is still allowed to trigger, so it can reach the Windows workflow too.

### One-time setup on the runner Mac

1. **Install the Developer ID Application certificate** into the login keychain:
   - From [Apple Developer → Certificates](https://developer.apple.com/account/resources/certificates/list), create a *Developer ID Application* cert, download the `.cer`, and double-click to add it to the **login** keychain.
   - Verify with `security find-identity -v -p codesigning` — note the quoted identity string (e.g. `Developer ID Application: Your Name (TEAMID)`). This is the value you'll put in `APPLE_SIGNING_IDENTITY` below.
2. **Keep the login keychain unlocked during builds.** The runner must be started by the logged-in user (default `./run.sh`, or `./svc.sh install` under your user account) so it inherits keychain access. If signing fails with `errSecAuthFailed`, the keychain is locked — log back in or run `security unlock-keychain ~/Library/Keychains/login.keychain-db`.

### Signing packages for update

```shell
npx @tauri-apps/cli signer generate
```

### One-time setup in the repo

1. **Generate an app-specific password** at [appleid.apple.com → Sign-In and Security → App-Specific Passwords](https://appleid.apple.com).
2. **Create the `release` environment** at **Settings → Environments → New environment** → name `release`.
   - Add yourself under **Required reviewers** so signed builds pause for approval.
   - Restrict **Deployment branches and tags** to the protected `main` branch. The workflow checks out the release tag, but the environment deployment ref is the `main` push that created it.
3. **Add these secrets to the `release` environment** (not repo-level secrets):

   | Secret                               | Value                                                                                                     |
   | ------------------------------------ | --------------------------------------------------------------------------------------------------------- |
   | `APPLE_SIGNING_IDENTITY`             | The quoted identity string from step 1.1, e.g. `Developer ID Application: Your Name (TEAMID)`              |
   | `APPLE_ID`                           | Apple ID email associated with your developer account                                                     |
   | `APPLE_PASSWORD`                     | App-specific password from the previous step (not your Apple ID password)                                 |
   | `APPLE_TEAM_ID`                      | 10-character Team ID from [developer.apple.com/account](https://developer.apple.com/account) → Membership  |
   | `TAURI_SIGNING_PRIVATE_KEY`          | Ed25519 private key content (or a path to the key file) generated by `tauri signer generate`              |
   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for that private key (leave empty if the key has no password)                                     |
   | `R2_ACCESS_KEY_ID`                   | R2 API token Access Key ID (Cloudflare → R2 → Manage R2 API Tokens, scoped Object Read & Write)           |
   | `R2_SECRET_ACCESS_KEY`               | R2 API token Secret Access Key                                                                             |

4. **Add these variables to the same environment:**

   | Variable                | Value                                                           |
   | ----------------------- | --------------------------------------------------------------- |
   | `R2_ENDPOINT`           | `https://<account-id>.r2.cloudflarestorage.com`                  |
   | `R2_BUCKET`             | Bucket backing the public `r2.dev` desktop download domain      |

5. **Create a separate `windows-release` environment** and give it the same
   required-reviewer and `main` deployment-branch protections. Keep the
   Authenticode provider credentials out of `release`:

   | Secret / variable  | Kind     | Value                                                         |
   | ------------------ | -------- | ------------------------------------------------------------- |
   | `ES_USERNAME`      | secret   | SSL.com eSigner account username                              |
   | `ES_PASSWORD`      | secret   | SSL.com eSigner account password                              |
   | `ES_TOTP_SECRET`   | secret   | eSigner TOTP seed                                             |
   | `ES_CREDENTIAL_ID` | variable | Credential ID for the reviewed OV/EV code-signing certificate |

   The Windows job intentionally cannot read the Tauri updater private key.
   Tauri first Authenticode-signs the main executable, sidecar, executable
   resources, uninstaller, and final NSIS installer through `signCommand`.
   Only the downstream `sign-windows-updater` job can generate the detached
   updater signature, so that signature always covers the final Authenticode
   bytes.

   Each publish job uploads its immutable payload below
   `desktop/releases/<version>/`, refreshes its human download alias
   (`desktop/oats.dmg` / `desktop/oats-windows-x86_64.exe`), then overwrites its
   own updater manifest last with `no-cache`.

   **Manifests are per-platform**, because a Tauri static manifest carries a
   single top-level `version` and could not honestly describe macOS on 0.19.0
   while Windows is still on 0.18.0:

   | Object                                | Written by | Read by                                                    |
   | ------------------------------------- | ---------- | ---------------------------------------------------------- |
   | `desktop/latest-darwin-aarch64.json`  | macOS      | macOS clients (`endpoints[0]`, `{{target}}`/`{{arch}}`)    |
   | `desktop/latest-windows-x86_64.json`  | Windows    | Windows clients (`endpoints[0]`)                           |
   | `desktop/latest.json`                 | macOS only | macOS clients installed before per-platform manifests existed |

   Updater endpoints are compiled into the app, so every client shipped before
   this split still polls the bare `desktop/latest.json`. The macOS publisher
   keeps writing a macOS-only copy there; the Windows publisher never touches
   it, so that object can never advertise a version macOS has not shipped.

   **When can `desktop/latest.json` go?** Not on a date — we have no client
   version telemetry, so we cannot observe the remaining population. Treat it as
   permanent: it costs one extra ~2 KB `PUT` per release. Drop it only if
   telemetry later shows no client older than the first release carrying
   per-platform endpoints (0.19.0) still checking in.

   `latest.json` stays second in `endpoints` purely as a safety net: Tauri walks
   the list and skips any endpoint that does not return 2xx. Do not delete a
   published `latest-<target>-<arch>.json` — a client that falls through to
   `latest.json` and finds no entry for its platform surfaces an update-check
   error rather than a clean "no update available".

   MSI ships only as a direct-download GitHub Release asset: it never reaches R2,
   is never included in the consumer updater, and carries no Tauri updater
   signature. For Partner Center only, explicitly pass
   `src-tauri/tauri.microsoftstore.conf.json`; it embeds the offline WebView2
   installer and is not used by normal NSIS releases.

## Cutting a release

Releases are automated by [release-please](https://github.com/googleapis/release-please) (the `Release` workflow). On every push to `main` it parses conventional commits (`feat:` → minor, `fix:` → patch, `feat!:`/`BREAKING CHANGE:` → major) and maintains a **Release PR** that bumps the version in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`, updates `CHANGELOG.md`, and (via the `sync-lock` job) keeps `package-lock.json` and `Cargo.lock` in sync.

1. **Merge feature/fix PRs to `main`** with conventional-commit messages. release-please keeps the Release PR up to date.
2. **Merge the Release PR** when ready to ship. That merge is a push to `main`, so release-please creates the `vX.Y.Z` tag and GitHub Release, the macOS pipeline runs in the same workflow, and the Windows pipeline is dispatched as a separate `Release (Windows)` run.
3. **Approve the protected deployments** if required-reviewer protection is configured: `release` for macOS/updater signing/publication and `windows-release` for eSigner access. The two runs approve independently.
4. **If Windows fails**, macOS has already shipped. Fix the cause and re-dispatch just the Windows workflow against the same tag — see [CI: validation and signed releases](#ci-validation-and-signed-releases).

> **Note:** The macOS signing/publish jobs run from the `Release` workflow on push to `main`, so creating a GitHub Release by hand (e.g. `gh release create`) no longer triggers the build. To ship, merge the Release PR (or push the version bumps to `main`). Windows is reached only by the dispatch that same run makes — a hand-made Release triggers neither platform.

> **Note:** The `release` and `windows-release` environment deployment policies allow the protected `main` branch. The workflow checks out the generated `v*` tag inside each job, but the protected deployment ref remains the `main` push that cut the release.

### release-please setup

No PAT is required: the `Release` workflow uses the default `GITHUB_TOKEN` (with `contents: write` + `pull-requests: write` granted at the job level). This works because the macOS signing pipeline runs as downstream jobs in the same run rather than relying on the published Release to trigger a separate workflow — the one thing the default token can't do. The Windows workflow is reached by `workflow_dispatch` (with `actions: write`), which is explicitly exempt from that restriction.

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
