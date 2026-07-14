# Windows local inference sidecar

The `windows/` target is the Rust implementation of oats' local transcription and
notes process for Windows. It is a separate executable because native model
runtimes and their dependencies should not run inside the Tauri application
process.

The desktop host uses one `ariso-stt` command contract on both platforms:

- macOS packages the Swift implementation from `src-tauri/ariso-stt/macos`.
- Windows packages this crate as `ariso-stt.exe`.

The language-neutral CLI and transcript contract lives in
`src-tauri/ariso-stt/shared`. The platform package names describe source
ownership; the packaged executable name stays `ariso-stt` on both platforms so
the Tauri integration and external CLI contract remain stable.

Tauri starts the packaged sidecar from `src-tauri/src/transcribe.rs`.
`src-tauri/src/model_manager.rs` owns model downloads, verification, progress,
and completion markers for both macOS and Windows. The Windows build script
copies the release binary to
`src-tauri/binaries/ariso-stt-x86_64-pc-windows-msvc.exe`, following Tauri's
target-specific sidecar naming convention.

## Source layout

- `main.rs`: parses the shared CLI contract and dispatches commands.
- `audio.rs`: decodes audio, downmixes it to mono, and resamples clips.
- `models.rs`: validates the lock-defined Windows model layout before inference.
- `transcribe.rs`: runs Parakeet and diarization, preserving raw speaker keys.
- `notes.rs`: runs Gemma through the packaged llama.cpp runtime.

`../shared/windows-models.json` is the source of truth for upstream artifact
hashes, immutable CDN prefixes, installed model files, and the packaged
llama.cpp release. Downloaded Gemma data is installed at
`<models>/llm/gemma-3-1b-it-qat-4bit/`, the same logical path used on macOS.
The executable llama.cpp runtime is staged under Tauri resources at build time
and never enters the downloaded model tree. `model_manager.rs` writes the
platform bundle identity into the shared completion marker after every model
file verifies.

Once the models have been explicitly downloaded, transcription and notes run
locally without network access.
