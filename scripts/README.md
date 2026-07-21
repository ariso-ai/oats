# Build and model scripts

## Model publishing

Both desktop platforms follow the same model lifecycle:

1. Stage the exact files the platform sidecar loads.
2. Write a deterministic `SHA256SUMS` manifest for each bundle.
3. Publish to an immutable Cloudflare R2 prefix.
4. Pin the bundle and upstream identities in the platform's reviewed metadata.
5. Let the Tauri host download and verify the bundle before invoking inference.

macOS stages the FluidAudio CoreML bundles with:

```bash
R2_ENDPOINT=https://<account-id>.r2.cloudflarestorage.com \
R2_BUCKET=<bucket> \
AWS_PROFILE=r2 \
./scripts/sync-stt-models.sh
```

Windows metadata is centralized in
`src-tauri/ariso-stt/shared/windows-models.json`. The publisher downloads the
exact pinned upstream Parakeet, diarization, and Gemma artifacts and stages only
model data:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\sync-windows-local-models.ps1 `
  -StageDir "$env:TEMP\oats-windows-models"
```

The command prints the three manifest hashes to pin. To publish after configuring the same S3-compatible R2 variables used by the release workflow, add `-Upload`:

```powershell
$env:R2_ENDPOINT = "https://<account-id>.r2.cloudflarestorage.com"
$env:R2_BUCKET = "<bucket-name>"
aws configure --profile oats-r2
$env:AWS_PROFILE = "oats-r2"

powershell -ExecutionPolicy Bypass -File scripts\sync-windows-local-models.ps1 `
  -Upload
```

Published version prefixes are immutable. If an existing prefix does not match
the lock, the helper fails. A model update requires a new prefix and pinned
manifest hash; there is no overwrite mode.

## Windows sidecar

The platform targets live in `src-tauri/ariso-stt/macos` and
`src-tauri/ariso-stt/windows`. Their language-neutral CLI and transcript
contract is documented under `src-tauri/ariso-stt/shared`.

`build-windows-sidecar.ps1` builds the x64 Windows `ariso-stt.exe`, copies it to
Tauri's target-specific external-binary path, and stages the exact llama.cpp
runtime pinned by the shared lock as an installer resource:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-windows-sidecar.ps1
```

Run `prepare-windows-llama-runtime.ps1` directly only when a host build needs
the resource without rebuilding the sidecar. The runtime is never downloaded
into the user's model directory; it ships inside the local QA application
installer.

The sidecar is inference-only and supports the same runtime commands as the macOS sidecar:

```text
ariso-stt --audio <path> --models <dir> --format json
ariso-stt notes --transcript <path> --models <dir>
```

## Tauri schemas

Regenerate tracked Tauri schemas from the production feature set, after staging
the platform sidecar/resources required by the host build:

```powershell
cargo build --manifest-path src-tauri\Cargo.toml --locked --features prod-api
```

`desktop-schema.json`, `capabilities.json`, and `acl-manifests.json` are the
canonical tracked outputs. The target-local `windows-schema.json` duplicates
the desktop schema and is intentionally ignored. Production ACL output must not
contain the optional debug-only `mcp` plugin entry.

## Windows installers

`build-windows-installers.ps1` builds both Windows installer formats through Tauri:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-windows-installers.ps1
```

Set `TAURI_SIGNING_PRIVATE_KEY` only when testing Tauri updater signatures; it
does not Authenticode-sign the installer.

The Windows release job and public Windows publication are removed until a
production signing provider is provisioned and reviewed. The script keeps
`-TauriConfig` optional so local QA builds can remain unsigned without creating
public artifacts.
When `TAURI_SIGNING_PRIVATE_KEY` is absent, the helper uses a temporary config
overlay to disable updater artifacts for that local build only; the checked-in
production setting remains enabled.

The MSI uses committed WiX bitmaps under `src-tauri/windows/installer`:

- `wix-dialog.bmp` is the 493 x 312 dialog image.
- `wix-banner.bmp` is the 493 x 58 banner image.

Replace these assets directly when updating the installer artwork, preserving
their dimensions and 24-bit BMP format.

Desktop CI calls the same sidecar helper. Release CI does not call this installer
helper; local/internal QA can invoke it directly.

## Windows Local benchmark

`tools/bench/windows-local-sidecar.ps1` measures transcription and notes against
already-installed model artifacts. It reports hardware and timing data and
performs no downloads. This is a sidecar benchmark, not an installed-app smoke
test:

```powershell
powershell -ExecutionPolicy Bypass -File tools\bench\windows-local-sidecar.ps1 `
  -Models "$HOME\.ariso\models" `
  -Audio "$env:TEMP\oats-smoke\audio.wav" `
  -Transcript "$env:TEMP\oats-smoke\transcript.md"
```

Use `-RepeatAudio 4` for a longer real-time-factor sample. To prove offline behavior, install the models through oats first, disconnect networking, and run the same command; no URL overrides are needed because the sidecar has no downloader.
