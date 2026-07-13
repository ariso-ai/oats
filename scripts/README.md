# Build and model scripts

## Model publishing

Both desktop platforms follow the same model lifecycle:

1. Stage the exact files the platform sidecar loads.
2. Write a deterministic `SHA256SUMS` manifest for each bundle.
3. Publish to an immutable Cloudflare R2 prefix.
4. Pin the manifest hash in `src-tauri/src/model_manager.rs`.
5. Let the Tauri host download and verify the bundle; sidecars never use the network.

macOS stages the FluidAudio CoreML bundles with:

```bash
R2_ENDPOINT=https://<account-id>.r2.cloudflarestorage.com \
R2_BUCKET=<bucket> \
AWS_PROFILE=r2 \
./scripts/sync-stt-models.sh
```

Windows stages the Parakeet ONNX, diarization, Gemma GGUF, and required llama.cpp CPU runtime files with:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\sync-windows-local-models.ps1 `
  -Models "$HOME\.ariso\models" `
  -StageDir "$env:TEMP\oats-windows-models"
```

The command prints the three manifest hashes to pin. To publish after configuring the same S3-compatible R2 variables used by the release workflow, add `-Upload`:

```powershell
$env:R2_ENDPOINT = "https://<account-id>.r2.cloudflarestorage.com"
$env:R2_BUCKET = "<bucket-name>"
$env:AWS_ACCESS_KEY_ID = "..."
$env:AWS_SECRET_ACCESS_KEY = "..."

powershell -ExecutionPolicy Bypass -File scripts\sync-windows-local-models.ps1 `
  -Models "$HOME\.ariso\models" `
  -Upload
```

Published version prefixes are immutable. The helper refuses to overwrite an existing prefix unless `-Force` is explicitly supplied for a byte-for-byte repair. A model update must use a new version/revision prefix and new pinned manifest hash.

## Windows sidecar

The platform source packages live in `src-tauri/ariso-stt-mac` and
`src-tauri/ariso-stt-windows`. Their language-neutral CLI and transcript
contract is documented under `src-tauri/ariso-stt-shared`.

`build-windows-sidecar.ps1` builds the x64 Windows `ariso-stt.exe` and copies it to Tauri's target-specific external-binary path:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-windows-sidecar.ps1
```

The sidecar is inference-only and supports the same runtime commands as the macOS sidecar:

```text
ariso-stt --audio <path> --models <dir> --format json
ariso-stt notes --transcript <path> --models <dir>
```

## Windows installers

`build-windows-installers.ps1` builds both Windows installer formats through Tauri:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw "C:\path\to\tauri.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "..."
powershell -ExecutionPolicy Bypass -File scripts\build-windows-installers.ps1
```

The MSI uses branded WiX bitmaps under `src-tauri/windows/installer`. Regenerate them after changing the source logo:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\generate-windows-installer-art.ps1
```

Release and desktop CI call the same sidecar and installer helpers.

## Windows Local smoke test

`windows-local-smoke.ps1` runs transcription and notes against already-installed model artifacts, reports hardware and timing data, and performs no downloads:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\windows-local-smoke.ps1 `
  -Models "$HOME\.ariso\models" `
  -Audio "$env:TEMP\oats-smoke\audio.wav" `
  -Transcript "$env:TEMP\oats-smoke\transcript.md"
```

Use `-RepeatAudio 4` for a longer real-time-factor sample. To prove offline behavior, install the models through oats first, disconnect networking, and run the same command; no URL overrides are needed because the sidecar has no downloader.
