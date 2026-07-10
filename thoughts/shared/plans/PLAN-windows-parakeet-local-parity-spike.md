# Windows Parakeet Local Parity Spike

## Goal

Implement a focused Windows Local spike that preserves oats' macOS Local feature shape as closely as possible:

- Local/offline transcription on Windows with the Parakeet speech model family.
- Speaker-attributed transcript JSON matching the current `ariso-stt` output contract.
- Local notes generation on Windows with the current Gemma notes model family, packaged for a Windows-native runtime.
- CPU and integrated GPU as the primary Windows target; dedicated GPU may accelerate but must not be required for the spike to be considered viable.

This is a spike, but it should be implemented behind the real product seams so follow-up work can harden it rather than restart it.

## Technical Choices

- Keep the existing Rust sidecar contract:
  - `ariso-stt --audio <path> --models <dir> --format json`
  - `ariso-stt download --models <dir>`
  - `ariso-stt notes --transcript <path> --models <dir>`
- Replace the current Windows placeholder sidecar in `src-tauri/ariso-stt-cross/` with a Windows-native implementation path for:
  - Parakeet STT runtime experiment.
  - Speaker diarization runtime experiment.
  - Gemma notes runtime experiment.
- Use platform-specific model artifacts while preserving the same app-level Local experience:
  - macOS continues to use the current FluidAudio/CoreML/MLX assets.
  - Windows uses Windows-consumable Parakeet, diarization, and Gemma artifacts under the same `~/.ariso/models` root.
- Keep model downloads explicit and integrity-pinned through R2, matching the existing `model_manager.rs` security pattern.
- Keep sidecar execution argv-based through `Command::new`; do not introduce shell strings.

## Current-State Analysis

Confirmed from the current branch:

- `src-tauri/src/transcribe.rs` already resolves `ariso-stt.exe` on Windows and keeps the sidecar invoke contract argv-based.
- `src-tauri/ariso-stt-cross/` currently builds a placeholder executable that only prints "not implemented" errors.
- `src-tauri/src/model_manager.rs` currently blocks Windows STT and LLM downloads with "cpp-sidecar model bundle is still pending" errors.
- Current STT model metadata points to `parakeet-tdt-0.6b-v3` and `speaker-diarization`, but the existing mirrored artifacts are shaped for the macOS FluidAudio/CoreML sidecar.
- Current notes model is `gemma-3-1b-it-qat-4bit`, mirrored as files loaded by the macOS notes path; Windows should publish a runtime-appropriate Gemma artifact rather than reuse macOS loader assumptions.
- `src-tauri/src/platform.rs` reports Windows `localBackend.engine = "cpp-sidecar"` but `supported = false`.
- `src/views/SettingsView.vue` already has independent STT and LLM install rows and uses platform capabilities to mark Local unsupported.
- `.github/workflows/desktop.yaml` and `.github/workflows/release.yaml` already build the Windows placeholder sidecar and copy it to Tauri's expected `src-tauri/binaries/ariso-stt-x86_64-pc-windows-msvc.exe`.

## Key Files And Roles

- `src-tauri/ariso-stt-cross/Cargo.toml`
  - Add dependencies/features for the Windows STT, diarization, and notes runtimes chosen during the spike.
- `src-tauri/ariso-stt-cross/src/main.rs`
  - Replace placeholder command handling with real `download`, transcribe, and `notes` subcommands.
  - Preserve stdout/stderr behavior: JSON transcript on stdout for transcribe, Markdown on stdout for notes, diagnostic logs on stderr.
- `src-tauri/src/transcribe.rs`
  - Keep the current contract. Add tests if the Windows sidecar needs stricter argument or timeout behavior.
- `src-tauri/src/model_manager.rs`
  - Add Windows-specific model readiness, download, path layout, SHA-256 verification, and progress events.
  - Preserve macOS model paths and readiness behavior.
- `src-tauri/src/platform.rs`
  - Flip Windows Local from pending to supported only after the spike proves install + transcribe + notes works locally.
- `src/tauri.ts`
  - Keep existing typed wrappers unless the model status shape needs a Windows model-bundle version field.
- `src/views/SettingsView.vue`
  - Reuse the existing STT/LLM install flow; adjust copy only if Windows needs clearer model labels.
- `src-tauri/tauri.conf.json`
  - Ensure Windows does not need macOS-only MLX resources to bundle.
- `.github/workflows/desktop.yaml`
  - Build the real Windows sidecar and run Windows contract tests.
- `.github/workflows/release.yaml`
  - Continue treating Windows NSIS as internal until Local parity and signing criteria are met.
- `docs/superpowers/specs/2026-06-27-windows-full-parity-design.md`
  - Update the Local backend section to reflect Parakeet-family Windows Local parity once the spike has an implemented path.

## Task Breakdown

### 1. Define The Windows Model Layout

- Add a Windows model layout under the existing `~/.ariso/models` root:
  - `models/windows/parakeet-tdt-0.6b-v3/<version>/...`
  - `models/windows/speaker-diarization/<version>/...`
  - `models/windows/gemma-3-1b-it-qat-4bit/<version>/...`
- Keep macOS model layout untouched.
- Decide whether Windows should use one readiness marker for the whole local bundle or keep the current separate STT and LLM readiness shape.
- Preserve existing frontend semantics where STT and LLM can be installed independently unless the selected runtime requires a single bundle.

### 2. Build The Parakeet Windows Sidecar Spike

- Replace `src-tauri/ariso-stt-cross/src/main.rs` placeholder logic with structured command parsing.
- Implement transcribe mode:
  - Load Parakeet model artifacts from `--models`.
  - Read the input audio path passed by Rust.
  - Emit JSON with `language`, `participants`, and `segments`.
  - Ensure output parses as `TranscriptResult` in `src-tauri/src/transcribe.rs`.
- Implement diarization integration:
  - Produce stable participant ids and labels compatible with `storage::Participant`.
  - Attach `speaker` ids to transcript segments compatible with `storage::Segment`.
- Keep the spike hardware target explicit:
  - CPU execution must run.
  - Integrated GPU acceleration should be tested where the runtime supports it.
  - Dedicated GPU must not be required for correctness.

### 3. Build The Gemma Notes Path For Windows

- Package a Windows-consumable Gemma artifact from the current `gemma-3-1b-it-qat-4bit` model family.
- Implement `ariso-stt notes --transcript <path> --models <dir>` in `src-tauri/ariso-stt-cross/src/main.rs`.
- Keep notes output Markdown on stdout.
- Keep the current Rust timeout and best-effort notes semantics in `src-tauri/src/transcribe.rs`.
- For long transcripts, implement chunked notes generation inside the sidecar if the runtime context window cannot handle full meeting transcripts.

### 4. Add Windows Model Download And Integrity Verification

- Extend `src-tauri/src/model_manager.rs` with Windows-specific constants for:
  - Model names.
  - R2 base paths.
  - Artifact versions.
  - Pinned SHA-256 values and byte sizes or pinned manifests.
- Reuse the existing safe download pattern:
  - Download to `.part`.
  - Verify SHA-256.
  - Atomically promote only verified files.
  - Write readiness markers only after all required files are present.
- Ensure `download_local_stt` and `download_local_llm` work on Windows after the Windows artifacts are published.
- Keep Local mode network behavior limited to explicit model downloads and the existing updater path.

### 5. Enable Windows Local In Platform Capabilities

- Once the spike can install models, transcribe, diarize, and generate notes:
  - Set Windows `local_backend.supported = true` in `src-tauri/src/platform.rs`.
  - Update tests that currently assert Windows Local is pending.
  - Verify Settings shows Local as selectable on Windows.

### 6. Contract And Fixture Tests

- Add sidecar contract fixtures:
  - Short WAV fixture for transcription.
  - Expected transcript JSON schema validation.
  - Small transcript fixture for notes generation.
- Test cases:
  - Transcribe success.
  - Transcribe malformed/empty model dir failure.
  - Notes success.
  - Notes timeout/failure propagation.
  - Model download interrupted/retry behavior.
  - No readiness marker until all files verify.
- Keep the fixture small enough for CI. Full-model performance runs should be separate from standard validation.

### 7. Performance Spike Harness

- Add a local benchmark script under `scripts/` or `src-tauri/ariso-stt-cross/benches/` that records:
  - Hardware summary.
  - Runtime backend used: CPU or integrated GPU.
  - Audio duration.
  - STT wall-clock time.
  - Diarization wall-clock time.
  - Notes wall-clock time.
  - Real-time factor for transcription + diarization.
- Benchmark fixtures:
  - 1 minute smoke fixture.
  - 10 minute acceptance fixture.
  - 30 minute stress fixture.
- Candidate viability target for the spike:
  - 10 minute meeting completes end-to-end on a normal Windows laptop CPU/iGPU without requiring a dedicated GPU.
  - Output includes speaker-attributed transcript and Markdown notes.

### 8. CI And Packaging

- Update `.github/workflows/desktop.yaml`:
  - Build the real Windows sidecar.
  - Run sidecar contract tests on `windows-latest`.
  - Keep full model benchmark tests out of normal CI unless models are cached and runtime is predictable.
- Update `.github/workflows/release.yaml`:
  - Copy the real Windows sidecar into Tauri's target-named binary path.
  - Keep NSIS artifact internal until Windows Local, system audio, updater, and signing are release-ready.
- Split macOS-only resources in `src-tauri/tauri.conf.json` if Windows still needs placeholder `mlx-swift_Cmlx.bundle` to bundle.

### 9. Documentation

- Update `docs/superpowers/specs/2026-06-27-windows-full-parity-design.md` after the spike has a concrete implementation path.
- Update `README.md` and `CONTRIBUTING.md` with:
  - Windows Local setup.
  - Model download expectations.
  - How to run the sidecar contract tests.
  - How to run the Windows performance harness.

## Automated Verification

- `npm test`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked`
- `cargo test --manifest-path src-tauri/ariso-stt-cross/Cargo.toml --locked`
- Windows CI:
  - Build `src-tauri/ariso-stt-cross`.
  - Copy `ariso-stt.exe` to `src-tauri/binaries/ariso-stt-x86_64-pc-windows-msvc.exe`.
  - Run sidecar contract tests against small fixtures.
  - Build Tauri on `windows-latest`.

## Manual Verification

- Windows 11 laptop without dedicated GPU:
  - Install or run dev build.
  - Switch Settings backend to Local.
  - Download STT and LLM models through the app.
  - Disconnect network after model install.
  - Record microphone audio.
  - Confirm local transcript with speaker labels.
  - Confirm local Markdown notes.
  - Retry failed transcription and retry notes from Library.
- Windows 11 laptop with integrated GPU:
  - Run the performance harness for 1, 10, and 30 minute fixtures.
  - Record real-time factor and notes generation time.
- macOS regression check:
  - Confirm current FluidAudio/CoreML path still downloads, transcribes, diarizes, and generates Gemma notes.

## Out Of Scope

- Public Windows release readiness.
- Windows system-audio loopback capture.
- Windows auto-record detection.
- Installer signing.
- Exact transcript text parity across macOS and Windows.
- Replacing the current macOS FluidAudio sidecar.

## Assumptions

- The spike is intentionally Parakeet-family-first for Windows speech parity.
- The goal is to preserve the macOS Local product experience, not necessarily the same binary model files.
- The app may publish separate macOS and Windows model artifacts under versioned R2 paths.
- Local/offline privacy remains a hard requirement: after explicit model downloads, Local transcription and notes generation must make no network calls.

## Research Inputs

- NVIDIA Parakeet model family: https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3
- FluidAudio current macOS path: https://github.com/FluidInference/FluidAudio
- Gemma model family: https://huggingface.co/mlx-community/gemma-3-1b-it-qat-4bit
- Windows ONNX Runtime / DirectML path: https://onnxruntime.ai/docs/execution-providers/DirectML-ExecutionProvider.html
- Windows ML / ONNX Runtime guidance: https://onnxruntime.ai/docs/get-started/with-windows.html
