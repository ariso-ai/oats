# ariso-stt


`ariso-stt` is the local transcription and notes sidecar project for oats. It
contains one shared runtime contract and a native implementation for each
desktop platform:

- `macos/`: Swift, FluidAudio, CoreML, and MLX.
- `windows/`: Rust, sherpa-onnx, and llama.cpp.
- `shared/`: CLI documentation, transcript schema, and contract fixtures.

Both platform targets produce the same runtime command name:

```text
macOS:   ariso-stt
Windows: ariso-stt.exe
```

The Tauri host owns model downloads and launches the target-specific executable
through the shared argv and stdout contract. Platform-native inference remains
separate so each target can use its preferred model runtime and hardware
acceleration without changing the application-facing interface.

## Model layout and versions

The local Gemma path is identical on both platforms:

```text
<models>/llm/gemma-3-1b-it-qat-4bit/
```

Both implementations use the Gemma 3 1B instruction-tuned QAT 4-bit model family
from Hugging Face (`gemma-3-1b-it-qat-4bit`), but their runtime files are
intentionally different. macOS loads the `mlx-community` safetensors mirror;
Windows loads `gemma-3-1b-it-qat-Q4_0.gguf`. Its pinned llama.cpp runtime is an
installer resource, not a downloaded model file. The shared `.complete` marker
records the expected platform bundle identity instead of putting that version
in the local directory name.

STT distribution has two version layers. macOS download URLs use the upstream
commit-hash prefixes returned by `macos_stt_bundles()` in
`src-tauri/src/model_manager.rs`, while its persisted model identity remains
`parakeet-tdt-0.6b-v3`. Windows resolves source revisions, CDN prefixes,
manifests, install paths, and the llama.cpp release from
`shared/windows-models.json`. The host joins the Windows speech bundle
identities with `+`, so changing either Parakeet or diarization invalidates the
speech installation as a unit.
