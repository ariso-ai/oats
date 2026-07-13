# Shared ariso-stt contract

This directory owns the language-neutral contract between the oats Tauri host
and its platform-specific local-inference sidecars:

- `../ariso-stt-mac`: Swift, FluidAudio, and MLX for macOS.
- `../ariso-stt-windows`: Rust, sherpa-onnx, and llama.cpp for Windows.

Both source packages produce the same runtime executable name and accept the
same argv-based commands:

```text
ariso-stt --audio <path> --models <dir> --format json
ariso-stt notes --transcript <path> --models <dir>
```

Transcription writes one JSON object matching `transcript.schema.json` to
stdout. Notes writes Markdown to stdout. Diagnostics go to stderr. Model
downloads belong to the Tauri host, so sidecar inference remains offline.

`fixtures/transcript.json` is the canonical contract fixture. Host-side and
Windows-sidecar tests both consume it. Keep the schema, fixture, Swift output,
Rust output, and host parser aligned when the contract changes.
