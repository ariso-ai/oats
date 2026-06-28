# Windows Full-Parity Design

## Goal

Add Windows support without weakening oats' existing backend boundary:

- Ariso backend works on Windows through the current cloud upload/library flow.
- Local backend remains offline-only and must not transmit audio, transcripts, notes, or model
  inputs after the user has installed local models.
- Windows Local uses a new cpp sidecar path rather than attempting to port the current
  Swift/MLX sidecar.

The public Windows release target is Windows 11 first, NSIS installer first. Windows 10 support
requires at least one smoke pass before public support is claimed.

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

The intended Windows implementation is `whisper.cpp` for STT, a dedicated diarization component
for speaker labels, and `llama.cpp`/GGUF for notes. Until those engines and pinned model bundles
land, the Windows sidecar crate is only a buildable contract placeholder and must fail clearly
rather than emit fake transcripts.

## Build And Release

The validation workflow runs on `macos-15` and `windows-latest`. The Windows lane builds the
placeholder sidecar into Tauri's expected target-named path:

- `src-tauri/binaries/ariso-stt-x86_64-pc-windows-msvc.exe`

The release workflow builds an internal NSIS artifact on `windows-latest`. It is not a public
release artifact until the real Windows Local sidecar, system-audio capture, and installer signing
are complete.

## Remaining Native Work

- Implement and pin the Windows STT/diarization model bundle download path.
- Replace the placeholder Windows sidecar with the real whisper/diarization/llama pipeline.
- Implement WASAPI loopback while preserving the existing `system-audio-data` event contract.
- Implement Windows external microphone activity detection for auto-record.
- Split macOS-only bundle resources out of the shared Tauri config instead of creating CI-only
  placeholders for Windows builds.
