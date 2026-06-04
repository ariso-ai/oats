# Local Meeting Notes Generation — Design

**Date:** 2026-06-03
**Status:** Approved (pending spec review)

## Summary

Add on-device meeting-notes generation to the **Local** transcription backend. After
the `ariso-stt` sidecar transcribes a recording and Rust writes `transcript.md`, the
backend runs the Gemma 3 LLM over the transcript to produce structured meeting notes,
saved as `note.md` alongside `transcript.md` in the recording directory.

Switching to the Local backend installs two model sets as **separate flows**: the STT
models (ASR + diarizer), auto-started and gated by `manifest.json`, and the Gemma 3 notes
model, installed opt-in from the project CDN and gated by its own `.complete` marker. Both
are retryable from their Install buttons in Settings.

## Model

- **Repo:** `mlx-community/gemma-3-1b-it-qat-4bit` (Gemma 3 1B, QAT 4-bit, `model_type: gemma3`)
- **Size:** ~1.0 GB (4-bit QAT weights)
- **Runtime:** [`mlx-swift-lm`](https://github.com/ml-explore/mlx-swift-lm) `MLXLLM`.
  Confirmed support: `Gemma3Text.swift` exists and `LLMModelFactory` registers
  `"gemma3" -> Gemma3TextModel`. The repo is in the registry as
  `LLMRegistry.gemma3_1B_qat_4bit` (id `mlx-community/gemma-3-1b-it-qat-4bit`, type `.llm`).
- **Why Gemma 3 1B over a larger model:** model size is the dominant on-device cost —
  download footprint, RAM, and generation latency all scale with it. The 1B QAT-4bit
  variant keeps the bootstrap download and memory budget small enough to ship to every
  Apple-Silicon user. A larger model (e.g. Gemma 3 4B, or Gemma 4 E2B ~1.13 GB) is a
  one-line change to the loaded configuration if note quality proves insufficient — the
  code path is identical. To keep the small model from degenerating (echoing the
  transcript / looping), notes generation sets a repetition penalty (see Components).

## Architecture & Data Flow

```
record → write recording.mp3 → ariso-stt (transcribe) → write transcript.md → status=Done
                                                              │
                                                              └─► ariso-stt notes ──► write note.md  (best-effort)
```

Two phases, decoupled:

1. **Download/bootstrap** (one-time, gated): STT models (sidecar) + Gemma 3 model (Rust/R2),
   as two separate flows.
2. **Per-recording**: transcribe → write `transcript.md` → mark `Done` → generate notes →
   write `note.md`.

File IO stays in Rust (`storage`), matching the existing pattern where the Swift sidecar
returns data on stdout and Rust persists it.

## Components

### Swift sidecar (`src-tauri/ariso-stt`)

**Dependency:** add `https://github.com/ml-explore/mlx-swift-lm` to `Package.swift`
(products `MLXLLM`, `MLXLMCommon`, `MLXHuggingFace`), which transitively pulls in
`mlx-swift`. Trade-off: larger sidecar binary and longer CI sidecar build — acceptable,
called out for visibility.

STT (ASR + diarizer) and the notes LLM are **split download flows**: the sidecar
downloads the speech models, while the Rust app downloads the LLM directly from the
project CDN (Cloudflare R2). The published Gemma weights are HuggingFace Xet-backed,
which the Swift HF client cannot fetch, so the LLM is mirrored as plain files on R2 and
pulled by Rust rather than through the sidecar / HF Hub. The LLM lands in
`<models>/llm/gemma-3-1b-it-qat-4bit/`, so notes generation works fully offline.

**The `download` subcommand** (`ariso-stt download --models <dir>`) handles **STT only**:
- Phase 1 — ASR download → progress `0.0–0.66`
- Phase 2 — diarizer download → progress `0.66–1.0`
- Emits the existing JSON-lines progress contract: `{"type":"progress","fraction":F}` …
  then `{"type":"done"}`. The single 0→1 bar advances monotonically across both.
- The notes LLM is **not** downloaded here — see the Rust R2 download below.

**New `notes` subcommand** (`ariso-stt notes --transcript <path> --models <dir>`):
- Reads the transcript markdown at `--transcript`.
- Loads `mlx-community/gemma-3-1b-it-qat-4bit` via `MLXLLM` from the local
  `<models>/llm/gemma-3-1b-it-qat-4bit/` directory (already present after the Rust R2
  download; no network needed).
- Runs a fixed meeting-notes prompt and **writes the generated markdown to stdout**.
  Generation sets a repetition penalty (`repetitionPenalty ≈ 1.15`) — the small model
  otherwise degenerates into a loop and echoes the transcript instead of summarizing.
- Logs / progress / errors go to **stderr**; exit `0` on success, `1` on failure
  (same contract discipline as `transcribe`).

**Prompt:** a fixed instruction asking gemma to produce structured markdown notes from
the transcript — sections for a short summary, key discussion points, decisions, and
action items. Embedded in the `notes` command; tunable later.

### Rust orchestrator (`src-tauri/src`)

**`transcribe.rs`:**
- New `run_notes(transcript_path: &Path, models: &Path) -> Result<String, String>`,
  mirroring `run_transcribe`: spawn `ariso-stt notes …`, capture stdout as the notes
  markdown, surface stderr on failure.
- In `finalize_core`, after `storage::write_transcript` succeeds and `status` is set to
  `Done` and meta is written, call `run_notes`. **Best-effort:**
  - On success: `storage::write_notes(&dir, &md)` writes `note.md`.
  - On failure: log to stderr, set `meta.notes_error = Some(e)`, re-write meta. The
    recording **remains `Done`** — a notes failure never discards a good transcript.

**`storage.rs`:**
- New `write_notes(dir: &Path, markdown: &str)` → writes `<dir>/note.md` (atomic write,
  same as `write_transcript`).
- Add optional field to `RecordingMeta`:
  `#[serde(default, skip_serializing_if = "Option::is_none")] pub notes_error: Option<String>`.

**`model_manager.rs`:** two separate download commands with two readiness signals.
`download_local_stt` runs the sidecar `download` and writes `manifest.json` on exit `0`
(STT readiness). `download_local_llm` fetches the LLM files from the R2 CDN into
`<models>/llm/<name>/` and writes a `.complete` marker on success (LLM readiness, checked
by `llm_is_ready`). STT readiness gates recording; LLM readiness gates notes.

### Frontend (`src/views/SettingsView.vue`)

- **Two model rows**, each with its own Install button and status: Speech (STT) and
  Language (LLM). STT is required to record; the LLM is opt-in for notes.
- **Auto-start STT on switch:** `onSelectBackend('local')` auto-triggers the **STT**
  download when STT is not ready/downloading/unsupported (`shouldAutoDownload`). The LLM
  is **not** auto-downloaded — the user installs it via its own button.
- **Retry:** each Install button re-enables on its `error` state and re-runs its download
  (`download_local_stt` / `download_local_llm`). Already-complete files are skipped, so
  retry resumes rather than restarts.
- **Unsupported devices:** when `state === 'unsupported'`, both rows show "Unsupported on
  this device" and both Install buttons are disabled.

## Error Handling

| Failure | Behavior |
|---|---|
| STT download fails | No `manifest.json` written → STT status `error`/`not_downloaded`. Button re-enabled → user retries; completed files skipped. |
| LLM download fails | No `.complete` marker written → `llmReady` stays false. LLM Install button re-enabled → user retries; completed files skipped. |
| Notes generation fails at record time (e.g. OOM) | Logged + `meta.notes_error` set; recording stays `Done`, `note.md` absent. Transcript unaffected. |
| Transcription fails | Unchanged from today: recording `Failed`, audio retained, notes never attempted. |

## Testing

Rust stub-script tests (the existing `ARISO_STT_BIN` stub pattern, no real model):
- `run_notes` success → stdout captured, `note.md` written with expected content.
- `run_notes` failure (stub exits 1) → `finalize_core` keeps status `Done`, sets
  `notes_error`, and `transcript.md` still present.
- STT download: covered by existing manifest/readiness tests; a stub emitting the
  two-phase (ASR + diarizer) progress then `done` writes the manifest.
- LLM download: `download_llm_files` is exercised against a stub CDN; the `.complete`
  marker is written only after all files finish.

The Swift-side Gemma 3 load/generation is validated manually (requires the real ~1.0 GB
model and Apple Silicon); it is not unit-tested in CI.

## Docs

Update `README.md` sidecar-contract section to document the `notes` subcommand and the
extended three-phase `download` behavior.

## Out of Scope / Follow-ups

- Tuning or templating the notes prompt; per-language prompts.
- Streaming notes generation progress to the UI (generation is currently silent).
- Regenerating notes for existing recordings, or a "regenerate notes" action.
- Surfacing `notes_error` in the UI (stored in meta now; display can come later).
