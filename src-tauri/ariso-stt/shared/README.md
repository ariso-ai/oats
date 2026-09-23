# Shared ariso-stt contract

This directory owns the language-neutral contract between the oats Tauri host
and its platform-specific local-inference sidecars:

- `../macos`: Swift, FluidAudio, and MLX for macOS.
- `../windows`: Rust, sherpa-onnx, and llama.cpp for Windows.

Both source packages produce the same runtime executable name and accept the
same argv-based commands:

```text
ariso-stt --audio <path> --models <dir> --format json [--asr-model <id>]
ariso-stt notes --transcript <path> --models <dir>
```

Transcription writes one JSON object matching `transcript.schema.json` to
stdout. Notes writes Markdown to stdout. Diagnostics go to stderr. Model
downloads belong to the Tauri host, so sidecar inference remains offline.

`--asr-model` defaults to `parakeet-tdt-0.6b-v3`. The macOS sidecar also accepts
`qwen3-asr-0.6b-4bit` (Qwen3-ASR + Qwen3-ForcedAligner, loaded from
`<models>/qwen3-asr-0.6b-4bit` and `<models>/qwen3-forcedaligner-0.6b-4bit`;
language auto-detected and reported as an ISO code such as `zh` or `en`). Any
other value exits non-zero. The Windows sidecar ships Parakeet only and the
host never passes the flag there. On first load the macOS sidecar writes a
`tokenizer.json` into each Qwen3 model directory (generated locally from
`vocab.json`/`merges.txt`, atomic write, no network), so those directories must
be writable and host integrity/size checks must tolerate the extra file.

`windows-models.json` is Windows distribution metadata, shared by the Tauri
host, native sidecar, publisher, and installer build. It pins model data and the
llama.cpp runtime separately so downloaded model bundles cannot introduce
executable code.

Each segment carries the inference engine's transcript-local speaker key as a
string. Sidecars do not sort the final transcript, assign numeric speaker IDs,
deduplicate participants, or create labels. The Tauri host performs that shared
normalization after parsing, so FluidAudio and sherpa-onnx cannot drift into
different user-visible speaker policies.

`fixtures/transcript.json` is the canonical raw sidecar fixture. Host-side and
Windows-sidecar tests both consume it. Keep the schema, fixture, Swift output,
Rust output, and host parser aligned when the contract changes.
