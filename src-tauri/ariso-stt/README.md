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
