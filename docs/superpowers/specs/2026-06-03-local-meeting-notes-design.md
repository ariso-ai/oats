# Local Meeting Notes Generation — Design

**Date:** 2026-06-03
**Status:** Approved (pending spec review)

## Summary

Add on-device meeting-notes generation to the **Local** transcription backend. After
the `ariso-stt` sidecar transcribes a recording and Rust writes `transcript.md`, the
backend runs the gemma4 LLM over the transcript to produce structured meeting notes,
saved as `note.md` alongside `transcript.md` in the recording directory.

The notes model is bundled into the existing bootstrap **download** flow: switching to
the Local backend now downloads the STT models (ASR + diarizer) **and then** the gemma4
notes model, gated behind a single readiness manifest and retryable from the existing
Settings button.

## Model

- **Repo:** `mlx-community/gemma-4-e2b-it-4bit` (Gemma 4 E2B, 4-bit, `model_type: gemma4`)
- **Size:** ~1.13 GB (single `model.safetensors`)
- **Runtime:** [`mlx-swift-lm`](https://github.com/ml-explore/mlx-swift-lm) `MLXLLM`.
  Confirmed support: `Gemma4.swift` / `Gemma4Text.swift` exist and `LLMModelFactory`
  registers `"gemma4" -> Gemma4Model`. The repo is in the registry as
  `LLMRegistry.gemma4_e2b_it_4bit` (id `mlx-community/gemma-4-e2b-it-4bit`, type `.llm`).
- The E2B variant was chosen over E4B (~1.67 GB) for smaller download / lower RAM /
  faster generation. Switching to E4B later is a one-line change to the loaded
  configuration — the code path is identical.

## Architecture & Data Flow

```
record → write recording.mp3 → ariso-stt (transcribe) → write transcript.md → status=Done
                                                              │
                                                              └─► ariso-stt notes ──► write note.md  (best-effort)
```

Two phases, decoupled:

1. **Download/bootstrap** (one-time, gated): STT models + gemma4 model.
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

All Hub access (download and load) uses a single download base under the `--models` dir
(e.g. `<models>/hub`) so the model downloaded during bootstrap is the same one loaded at
notes time, and notes generation works fully offline.

**Extend the `download` subcommand** (`ariso-stt download --models <dir>`):
- Phase 1 — ASR download → progress `0.0–0.33`
- Phase 2 — diarizer download → progress `0.33–0.5`
- Phase 3 — gemma4 model download → progress `0.5–1.0`
- Emits the existing JSON-lines progress contract: `{"type":"progress","fraction":F}` …
  then `{"type":"done"}`. The single 0→1 bar advances monotonically across all three.
- Gemma download triggers via the `MLXLLM` model-load/snapshot path against the shared
  Hub base, mapping its fractional progress into the `0.5–1.0` band.

**New `notes` subcommand** (`ariso-stt notes --transcript <path> --models <dir>`):
- Reads the transcript markdown at `--transcript`.
- Loads `mlx-community/gemma-4-e2b-it-4bit` via `MLXLLM` from the shared Hub base
  (already present after bootstrap; no network needed).
- Runs a fixed meeting-notes prompt and **writes the generated markdown to stdout**.
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

**`model_manager.rs`:** no structural change. The extended `download` subcommand simply
emits more progress on the existing `model://progress` stream; `manifest.json` is still
written only after the sidecar exits `0` — which now means **all three** models
downloaded. `is_ready` (the recording gate) therefore implies STT *and* gemma are present.

### Frontend (`src/views/SettingsView.vue`)

- **Auto-start download on switch:** `onSelectBackend('local')` triggers `onDownloadModel()`
  when `modelStatus` is not `ready`/`downloading`/`unsupported` (today the user must click
  the button manually).
- **Retry:** the existing **Download model** button stays. Because it re-enables on the
  `error` state (error is not in the disabled set), it already serves as the retry control
  — clicking re-runs `download_local_model`. Hub caching means already-fetched files are
  not re-downloaded, so retry resumes rather than restarts.
- The combined download is shown as a **single progress bar** for all three models (no
  separate "STT" vs "notes model" phases in the UI).

## Error Handling

| Failure | Behavior |
|---|---|
| Download fails (any phase, incl. gemma) | No manifest written → status `error`/`not_downloaded`. Button re-enabled → user clicks to retry; Hub resumes from cache. |
| Notes generation fails at record time (e.g. OOM) | Logged + `meta.notes_error` set; recording stays `Done`, `note.md` absent. Transcript unaffected. |
| Transcription fails | Unchanged from today: recording `Failed`, audio retained, notes never attempted. |

## Testing

Rust stub-script tests (the existing `ARISO_STT_BIN` stub pattern, no real model):
- `run_notes` success → stdout captured, `note.md` written with expected content.
- `run_notes` failure (stub exits 1) → `finalize_core` keeps status `Done`, sets
  `notes_error`, and `transcript.md` still present.
- Extended download: covered by existing manifest/readiness tests; a stub emitting the
  three-phase progress then `done` writes the manifest.

The Swift-side gemma load/generation is validated manually (requires the real ~1.13 GB
model and Apple Silicon); it is not unit-tested in CI.

## Docs

Update `README.md` sidecar-contract section to document the `notes` subcommand and the
extended three-phase `download` behavior.

## Out of Scope / Follow-ups

- Tuning or templating the notes prompt; per-language prompts.
- Streaming notes generation progress to the UI (generation is currently silent).
- Regenerating notes for existing recordings, or a "regenerate notes" action.
- Surfacing `notes_error` in the UI (stored in meta now; display can come later).
