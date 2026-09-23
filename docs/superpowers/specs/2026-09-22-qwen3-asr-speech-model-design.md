# Qwen3-ASR as a selectable local speech model

**Issue:** #437 (Mandarin / mixed Chinese-English local transcription). Supersedes the
Whisper direction in #438, which is blocked on `mlx-audio-swift` dropping speech at its
30 s window boundaries.

## Goal

Add **Qwen3-ASR 0.6B (4-bit)** as a second on-device speech model, selectable in
Settings next to Parakeet, on macOS. It must transcribe Mandarin and mixed
Chinese-English meetings with speaker labels and timestamps, fully offline once
downloaded. Parakeet stays the default and its existing installs stay valid.

## Decisions

| question | decision |
|---|---|
| Engine | `mlx-audio-swift` (`MLXAudioSTT`) — `Qwen3ASRModel` + `Qwen3ForcedAlignerModel` |
| Timestamps / speakers | Qwen3-ASR emits text only; **Qwen3-ForcedAligner-0.6B** produces word timings, fed into the existing `mergeSegments` diarizer merge |
| Language | **Auto-detect** — Qwen3-ASR's detected language drives the aligner and is written to the transcript; no new setting |
| Platforms | macOS only; Windows hides the Qwen3 row |
| Default | Parakeet stays default |

## Model artifacts

Mirrored from Hugging Face at pinned commits into `~/.ariso/model-mirror/` and published
to R2 at the usual content-addressed prefixes (`models/<folder>/<12-char HF sha>/`).

| folder | HF repo @ commit | R2 prefix | `SHA256SUMS` sha256 | size |
|---|---|---|---|---|
| `qwen3-asr-0.6b-4bit` | `mlx-community/Qwen3-ASR-0.6B-4bit` @ `313d850181767edf09f00a9c289becca70e58cd0` | `313d85018176` | `e67be63dffa605fe9332adebeb34a4e53096c0c9a6540ef24ae53a1cb7fef5a3` | 693 MB |
| `qwen3-forcedaligner-0.6b-4bit` | `mlx-community/Qwen3-ForcedAligner-0.6B-4bit` @ `2f652af86ae0c73fe189b9429225c908ce4bf020` | `2f652af86ae0` | `930c0dbb18b0ac19bb2df027f03436062487d91ec9c1ea97125d9cb30db82cdf` | 933 MB |

Each folder holds every repo file except `README.md` / `.gitattributes`:
`model.safetensors`, `model.safetensors.index.json`, `config.json`, `vocab.json`,
`merges.txt`, `tokenizer_config.json`, `preprocessor_config.json`,
`chat_template.json`, `generation_config.json`, plus `SHA256SUMS`. The tokenizer files
**must** ship in the folder — the loader uses `AutoTokenizer.from(modelFolder:)`, and a
missing tokenizer must never become a runtime Hugging Face fetch (offline guarantee).

The largest file (aligner weights, 926 MiB) is under `MODEL_MAX_FILE_BYTES` (1 GiB).

## 1. Sidecar (`src-tauri/ariso-stt/macos`)

**Dependency.** Add `Blaizzy/mlx-audio-swift` (product `MLXAudioSTT`, plus
`MLXAudioCore` for audio loading), pinned to an exact revision. The #438 spike showed it
resolves with no movement in the existing pins (mlx-swift, mlx-swift-lm, FluidAudio).

**Argv.** New optional flag `--asr-model <id>`:

- absent or `parakeet-tdt-0.6b-v3` → today's FluidAudio path, unchanged
- `qwen3-asr-0.6b-4bit` → the Qwen3 path below
- anything else → `fail("unknown --asr-model")`, non-zero exit

Documented in `shared/README.md`. The Windows sidecar rejects the Qwen3 id.

**Qwen3 pipeline.**

1. Downmix/resample the recording to 16 kHz mono `Float` samples (the diarizer already
   needs exactly this; compute once, share).
2. Split with the library's public `splitAudioIntoChunks(_, sampleRate: 16000,
   chunkDuration: 300)`, which cuts at low-energy points near each boundary.
3. For each `(chunk, offset)`:
   - `asr.generate(audio: chunk, maxTokens: <per-chunk budget>, language: nil)` →
     text + detected language.
   - `aligner.generate(audio: chunk, text: text, language: <detected, as the
     aligner's language name>)` → word items with start/end.
   - Shift every item by `offset`.
4. Convert the items to the `TokenTiming` shape `mergeSegments` consumes and run the
   existing FluidAudio diarization + merge unchanged.
5. `language` in the output = the merged detected language as an ISO code (`zh`, `en`,
   …). The Parakeet path keeps `"en"`.

Why chunk in the sidecar rather than call `generate` on the whole recording:

- `Qwen3ASRModel.generate`'s `maxTokens` (default 8192) is **one budget shared across
  all chunks**; once spent, later chunks are skipped silently. An hour of Mandarin can
  exceed it. Per-chunk calls give each chunk its own budget, so nothing is dropped.
- The aligner handles at most ~5 minutes per call; 300 s chunks keep every call in
  range.

Chunks where the ASR returns empty text skip alignment. Aligner language mapping: the
aligner splits `Chinese` text into characters (keeping embedded Latin words whole) and
every other language on spaces; any detected language the aligner does not list falls
back to space-splitting.

**Loading.** `Qwen3ASRModel.fromModelDirectory(<models>/qwen3-asr-0.6b-4bit)` and
`Qwen3ForcedAlignerModel.fromModelDirectory(<models>/qwen3-forcedaligner-0.6b-4bit)`.
Never `fromPretrained` — that path downloads.

## 2. Host (`src-tauri/src`)

**`speech_model.rs`** — closed registry mirroring `notes_model.rs`:
`parakeet-tdt-0.6b-v3` (default) and `qwen3-asr-0.6b-4bit` (macOS only). `parse`
coerces anything unknown to the default. The selection is seeded into a process global
at startup and updated by the command that persists it, because the detached
transcribe and checkpoint paths have no `AppHandle`. The persisted key stays the
frontend's `speech:<id>` form in `settings.json` (`speechModel`).

**Sidecar call.** `run_transcribe` passes `--asr-model <selected id>`; checkpoints and
the final pass therefore use the same model.

**Bundles per model** (`model_manager.rs`):

| model | bundles | readiness marker |
|---|---|---|
| Parakeet | `parakeet-tdt-0.6b-v3`, `speaker-diarization` | `manifest.json` at the models root (unchanged — existing installs stay ready) |
| Qwen3-ASR | `qwen3-asr-0.6b-4bit`, `qwen3-forcedaligner-0.6b-4bit`, `speaker-diarization` | `qwen3-asr-0.6b-4bit/.complete` containing the bundle identity |

**Per-model operations.** Readiness, download, size, and delete take a speech model id
instead of treating `LocalModelKind::Speech` as one unit. The commands' argument stays
a closed enum (never a path or free-text id — `oats-security` surface #2). The shared
`speaker-diarization` directory is removed only when no other *installed* speech model
needs it, and its size is counted toward each model that uses it. Download progress
and done/error events carry the model id. Each speech model has its own download guard;
the diarizer is written by whichever download runs first and re-verified by the other
(verified files are skipped).

**Recording gate.** `local_models_ready` = selected speech model ready **and** notes
model ready.

**Transcript identity.** A transcript's `model_version` records the model that produced
it (`qwen3-asr-0.6b-4bit` for the new path).

## 3. Settings UI (`src/`)

- `SPEECH_MODELS` gains **Qwen3-ASR 0.6B**, `runtime: 'local'`, hover details naming
  Mandarin / mixed Chinese-English support and the bundled word-timing aligner.
- Each speech row has its own install / size / remove state, keyed by model id.
- Selecting a speech model that is not installed prompts its download, the same flow
  notes models use.
- The Qwen3 row is hidden on Windows.

## 4. Testing

- **Rust:** registry parse/default/unknown; bundle pins well-formed; per-model
  readiness; deleting one speech model keeps the diarizer while the other is installed
  and removes it when it is the last; `local_models_ready` follows the selection;
  `run_transcribe` passes `--asr-model`.
- **Frontend:** catalog lists both speech models; per-row install/remove state;
  selection round-trip; Qwen3 row hidden on Windows.
- **Sidecar (manual, recorded in the PR):** a real Mandarin or mixed Chinese-English
  recording longer than 5 minutes (crosses a chunk boundary) with two speakers:
  - transcript language is `zh`; English terms are preserved, not translated
  - speakers alternate plausibly; no chunk's text is missing
  - Parakeet output on an English fixture is byte-identical to before
  - run with networking blocked — transcription still succeeds
  - record wall time and peak memory on the M1 / 16 GB baseline from #437
- `cargo test -- --test-threads=1` and `npm test` pass.

## Release prerequisite

The two folders must be uploaded to R2 at the prefixes above before the download
works. Uploads need fresh R2 credentials (the local `[r2]` profile is rejected).

## Out of scope

Real-time Qwen3 captions (checkpoints stay 5-minute batches); a Windows Qwen3 artifact;
a manual language picker; Chinese output for notes/titles (separate concern in #437);
the 1.7B model.
