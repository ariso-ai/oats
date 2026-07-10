# Handoff: Windows Parakeet Local Parity Spike

## Summary

Create a Windows Local spike that preserves oats' macOS Local feature shape as closely as possible: Parakeet-family local transcription, speaker-attributed transcript JSON, and Gemma-family local notes generation, all through the existing `ariso-stt` sidecar contract. The spike should target CPU and integrated GPU on Windows and must not require a dedicated GPU for correctness.

## Created Plan Path

`thoughts/shared/plans/PLAN-windows-parakeet-local-parity-spike.md`

## Key Technical Decisions

- Keep the existing `ariso-stt` CLI/output contract intact.
- Replace the Windows placeholder sidecar in `src-tauri/ariso-stt-cross/` with the real spike implementation.
- Use Windows-specific model artifacts instead of trying to load the existing macOS CoreML/MLX files.
- Keep Parakeet as the Windows speech model family for this spike.
- Keep Gemma as the Windows notes model family, packaged for a Windows-native runtime.
- Preserve the strict Local/offline guarantee: after explicit model downloads, local transcription and notes generation must not make network calls.
- Keep all sidecar execution argv-based through Rust `Command::new`.

## Task Overview

- Define a Windows model layout under `~/.ariso/models` that can hold Parakeet, diarization, and Gemma artifacts.
- Implement real command handling in `src-tauri/ariso-stt-cross/src/main.rs` for transcribe, `download`, and `notes`.
- Extend `src-tauri/src/model_manager.rs` with Windows model metadata, R2 paths, pinned integrity checks, readiness markers, and progress events.
- Enable Windows Local in `src-tauri/src/platform.rs` only after install, transcribe, diarization, and notes work.
- Add sidecar contract tests with small fixtures and keep full performance testing out of normal CI.
- Add a Windows performance harness measuring STT, diarization, notes time, and real-time factor on CPU/integrated GPU.
- Update CI/release jobs to build and copy the real Windows sidecar instead of the placeholder.

## Notable Research Findings

- The current macOS path is FluidAudio/CoreML/MLX-oriented; Windows should preserve the product contract and model family where feasible, not the exact macOS binary artifacts.
- Current repo state already has the right seams:
  - `src-tauri/src/transcribe.rs` resolves `ariso-stt.exe` on Windows.
  - `src-tauri/ariso-stt-cross/` exists as the Windows sidecar crate.
  - `src-tauri/src/model_manager.rs` has hardened R2 download and SHA-256 verification patterns to reuse.
  - Settings already has independent STT/LLM install rows.
- The current branch intentionally blocks Windows Local downloads with pending messages; this spike replaces those pending guards with real Windows model handling once artifacts exist.

## Files To Start With

- `src-tauri/ariso-stt-cross/src/main.rs`
- `src-tauri/ariso-stt-cross/Cargo.toml`
- `src-tauri/src/model_manager.rs`
- `src-tauri/src/transcribe.rs`
- `src-tauri/src/platform.rs`
- `src/views/SettingsView.vue`
- `.github/workflows/desktop.yaml`
- `.github/workflows/release.yaml`
- `docs/superpowers/specs/2026-06-27-windows-full-parity-design.md`

## Verification To Carry Forward

- `npm test`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked`
- `cargo test --manifest-path src-tauri/ariso-stt-cross/Cargo.toml --locked`
- Windows manual pass:
  - Download models.
  - Disconnect network.
  - Record locally.
  - Generate speaker-attributed transcript.
  - Generate Markdown notes.
  - Retry failed transcription and notes from Library.
