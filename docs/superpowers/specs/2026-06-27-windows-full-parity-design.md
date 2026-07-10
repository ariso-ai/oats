# Windows Full-Parity Design

## Goal

Add Windows support without weakening oats' existing backend boundary:

- Ariso backend works on Windows through the current cloud upload/library flow.
- Local backend remains offline-only and must not transmit audio, transcripts, notes, or model
  inputs after the user has installed local models.
- Windows Local uses a new cpp sidecar path rather than attempting to port the current
  Swift/MLX sidecar.

The public Windows release target is Windows 11 first, NSIS installer first. Windows 10 support
requires at least one smoke pass before public support is claimed. MSI is built as an internal
artifact for enterprise/admin validation, but it is not a public installer until signing, updater,
install, upgrade, and uninstall behavior have been smoke tested.

## Platform Capability Layer

The app exposes a native `platform_capabilities` command with this frontend shape:

```ts
interface PlatformCapabilities {
  os: 'macos' | 'windows' | 'linux';
  localBackend: { supported: boolean; engine: 'swift-mlx' | 'cpp-sidecar' | null };
  systemAudio: { supported: boolean; settingsUrl: string | null };
  autoRecord: { supported: boolean };
  nativeShare: { supported: boolean };
  notificationSettingsUrl: string | null;
  microphoneSettingsUrl: string | null;
}
```

Frontend code should read this capability source rather than inspecting `navigator.userAgent`
directly. Windows settings links must stay exact (`ms-settings:privacy-microphone`,
`ms-settings:sound`, `ms-settings:notifications`); do not add a wildcard opener scope.

## Local Backend Strategy

macOS keeps the existing Swift/MLX sidecar. Windows uses a separate sidecar executable that keeps
the current CLI contract:

- `ariso-stt --audio <path> --models <dir> --format json`
- `ariso-stt download --models <dir>`
- `ariso-stt notes --transcript <path> --models <dir>`

The implemented Windows Local spike uses the Parakeet-family sherpa-onnx runtime for STT,
sherpa-onnx speaker diarization for speaker labels, and `llama.cpp` with a Gemma-family GGUF for
notes. The sidecar still preserves the app-level contract:

- `ariso-stt download --models <dir>` installs the Windows speech bundle after explicit user
  action. It downloads pinned Parakeet ONNX files and pinned sherpa-onnx diarization artifacts,
  then verifies byte size and SHA-256 before files are treated as ready. The canonical spike
  layout is `windows/parakeet-tdt-0.6b-v3/v1` plus `windows/speaker-diarization/v1`. Download
  source URLs can be overridden with `ARISO_WINDOWS_*_URL` environment variables so the same
  verifier can be pointed at R2 mirrors.
- `ariso-stt --audio <path> --models <dir> --format json` emits `language`, `participants`, and
  speaker-attributed `segments` JSON on stdout. Diagnostics stay on stderr.
- `ariso-stt download-notes --models <dir>` installs the Windows notes bundle after explicit user
  action. For the spike this uses the ungated `ggml-org/gemma-3-1b-it-GGUF` Q4_K_M artifact,
  stored as `gemma-3-1b-it-q4_0.gguf`, plus the CPU `llama.cpp` Windows runtime. The official
  Google `gemma-3-1b-it-qat-q4_0-gguf` artifact is gated on Hugging Face and should be mirrored to
  R2 with a pinned digest before production release. The canonical spike layout is
  `windows/gemma-3-1b-it-qat-4bit/v1`.
- `ariso-stt notes --transcript <path> --models <dir>` runs `llama-cli` locally and emits Markdown
  notes on stdout.

CPU is the correctness baseline for the spike. Integrated GPU or other accelerated `llama.cpp` /
ONNX providers may be added later, but a dedicated GPU is not required for the implemented smoke
path.

## Windows Artifact Mirroring

`scripts/sync-windows-local-models.ps1` stages the Windows spike artifacts into the canonical
`models/windows/.../v1` R2 layout and writes `windows-local-manifest.json` with byte sizes,
SHA-256 hashes, public URLs, and sidecar override env vars. It also writes
`windows-local-sidecar-pins.txt` so artifact changes have an explicit Rust pin checklist. It can
upload that staged tree with `-Upload` when `R2_ENDPOINT` and `R2_BUCKET` are available.

The helper validates the official sherpa-onnx segmentation archive and llama.cpp runtime archive
against the same byte size and SHA-256 pins used by the sidecar. In restricted environments, pass
those archives explicitly with `-SegmentationArchive` and `-LlamaRuntimeArchive`; do not re-pack
extracted files, because re-packing changes archive digests and would not satisfy the sidecar pins.
For the gated official Google QAT GGUF, accept the license and download the file manually, then pass
it as `-GemmaGguf`; the helper will stage it under the sidecar's expected filename and emit the new
pin values that must be applied before making the mirror URL a production default.

## Build And Release

The validation workflow runs on `macos-15` and `windows-latest`. The Windows lane tests and builds
the real sidecar into Tauri's expected target-named path:

- `src-tauri/binaries/ariso-stt-x86_64-pc-windows-msvc.exe`

The release workflow builds internal NSIS and MSI artifacts on `windows-latest` from the same
Windows sidecar build:

- `windows-nsis-internal`
- `windows-msi-internal`

NSIS remains the primary consumer installer. MSI remains internal until Windows signing, updater,
install, upgrade, and uninstall behavior are verified. Windows packaging uses
`src-tauri/tauri.windows.conf.json` to remove macOS-only bundle resources instead of creating an
empty `mlx-swift_Cmlx.bundle` placeholder in CI.

## Remaining Native Work

- Run `scripts/sync-windows-local-models.ps1 -Upload` with production R2 credentials and set the
  Windows sidecar download URL defaults to those mirrored paths rather than the current public
  spike sources.
- Replace the ungated Gemma runtime-smoke artifact with the official Google QAT GGUF after license
  acceptance/mirroring.
- Implement WASAPI loopback while preserving the existing `system-audio-data` event contract.
- Implement Windows external microphone activity detection for auto-record.
- Finish public Windows signing, updater, install, upgrade, and uninstall verification for NSIS.
- Decide whether MSI should graduate from internal enterprise/admin validation to a public artifact.
