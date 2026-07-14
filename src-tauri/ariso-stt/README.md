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
Windows loads a GGUF export plus its pinned llama.cpp runtime. The shared
`.complete` marker records the expected platform bundle identity instead of
putting that version in the local directory name.

STT distribution has two version layers. macOS download URLs use the upstream
commit-hash prefixes listed in `MACOS_STT_BUNDLES`, while its persisted model
identity remains `parakeet-tdt-0.6b-v3` for existing installs. Windows publishes
strongly versioned ONNX bundles and joins every `folder@tag` with `+` to form the
readiness and recording version. That join means changing either Parakeet or
diarization invalidates the Windows speech bundle as a unit.
