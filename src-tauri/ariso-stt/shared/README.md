# Shared ariso-stt contract

This directory owns the language-neutral contract between the oats Tauri host
and its platform-specific local-inference sidecars:

- `../macos`: Swift, FluidAudio, and MLX for macOS.
- `../windows`: Rust, sherpa-onnx, and llama.cpp for Windows.

Both source packages produce the same runtime executable name and accept the
same argv-based commands:

```text
ariso-stt --audio <path> --models <dir> --format json
ariso-stt notes --transcript <path> --models <dir>
ariso-stt llm-complete --prompt <path> --models <dir> \
  --max-tokens <n> --temperature <n> --repetition-penalty <n>
```

Transcription writes one JSON object matching `transcript.schema.json` to
stdout. Notes writes `{"title":"...","notes":"..."}` and completion writes
`{"text":"..."}` to stdout. Diagnostics go to stderr. Model downloads belong
to the Tauri host, so sidecar inference remains offline.

The completion command is intentionally policy-free. Shared Tauri Rust owns
meeting-note prompts, transcript-delta selection, batching, cursoring,
validation, retries, and durable writes. Swift and Windows Rust only adapt that
bounded prompt to their native local-model runtime. This keeps product logic in
one implementation across both platforms.

The host starts sidecars directly with argv values; neither platform executes
a shell. Completion reads only the supplied local prompt/model paths and makes
no external network requests. The Windows adapter's authenticated llama.cpp
transport is bound to an ephemeral loopback port inside the sidecar lifetime.

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
