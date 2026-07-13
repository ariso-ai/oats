# Windows local inference sidecar

`ariso-stt-windows` is the Rust implementation of oats' local transcription and
notes process for Windows. It is a separate executable because native model
runtimes and their dependencies should not run inside the Tauri application
process.

The desktop host uses one `ariso-stt` command contract on both platforms:

- macOS packages the Swift implementation from `src-tauri/ariso-stt-mac`.
- Windows packages this crate as `ariso-stt.exe`.

The language-neutral CLI and transcript contract lives in
`src-tauri/ariso-stt-shared`. The platform package names describe source
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
- `models.rs`: defines the Windows model layout and discovers installed models.
- `transcribe.rs`: runs Parakeet transcription and speaker diarization.
- `notes.rs`: runs Gemma through the packaged llama.cpp runtime.

Once the models have been explicitly downloaded, transcription and notes run
locally without network access.
