# Backend Abstraction & Local Transcription — Design Spec

**Date:** 2026-06-02
**Status:** Approved (design); pending implementation plan
**Scope:** Introduce a switchable "backend" concept with two implementations — the
existing remote **Ariso** backend and a new fully-offline **Local** backend — plus a
filesystem storage layer under `~/.ariso/`.

---

## 1. Goal

Let the user choose, in Settings, how recordings are processed:

- **Ariso** (default): Google login; recording is uploaded to the remote API
  (`API_BASE_URL` / `WEB_APP_BASE_URL`), which transcribes and generates notes. This is
  the existing behavior, unchanged.
- **Local**: no login; the recording is transcribed on-device by a local STT model and
  saved as a markdown transcript on the local filesystem. No data leaves the machine.

Secondary goal: a storage layer that owns recording files, transcription files, and
model files under `~/.ariso/`.

### In scope (v1)

- Runtime backend switch in Settings.
- Local backend: record → on-device transcription (speaker-attributed) → markdown +
  structured metadata on disk.
- Model download/status management for the Local STT model.
- A read-only **Library** window listing past Local recordings.
- Storage layer (`~/.ariso/`).

### Out of scope (v1, designed for later)

- **Notes generation for Local** (local LLM summarization). The storage layout reserves a
  sibling `notes.md` + `notesStatus` field so this can be added with **no layout change**.
- In-Library **search** and **click-to-open**. v1 Library is a read-only list.
- **Retry transcription** UI. The data model supports it (audio is retained on failure),
  but no UI is built in v1.
- Non-Apple-Silicon / pre-macOS-14 support for the Local backend (see §3).

---

## 2. Architecture decision: hybrid by natural home (with a path to a Rust trait)

Each backend lives where its work already lives:

- **Ariso = TS.** Its orchestration (presign → PUT → confirm) already lives in
  `useMeetingApi.uploadAudio`. We wrap it; we do **not** rewrite it into Rust.
- **Local = Rust.** Filesystem, the Swift/CoreML sidecar, and model download are native
  concerns. TS calls a **single** cohesive command, `local_finalize_recording`, which runs
  the whole pipeline atomically and returns a result.

A thin TS `useBackend()` exposes only what the **views** need. Its method names mirror a
future Rust `Backend` trait 1:1 so the abstraction can later be re-homed entirely into Rust
(the rejected-for-now "symmetric Rust trait" approach) **without changing any view**.

### Rejected alternatives

- **Symmetric Rust trait now** — forces rewriting working, tested TS upload orchestration
  into Rust purely for symmetry. Churn without payoff. (This is the *eventual* target, not
  the starting point — see the migration seam in §5.)
- **Symmetric TS strategy** — makes TS drive a chatty, multi-step native pipeline across the
  FFI boundary, splitting error/partial-state handling awkwardly. Wrong layer for native
  orchestration.

---

## 3. STT engine: FluidAudio via a Swift sidecar

The Local STT engine is [FluidAudio](https://github.com/FluidInference/FluidAudio):

- Swift SDK, native CoreML on the Apple Neural Engine; **macOS 14+, Apple Silicon only**.
- ASR = **Parakeet TDT v3 0.6b** (CoreML); ~110× real-time on M4 Pro for batch ASR —
  ideal for whole-file ("batch") transcription, no streaming needed.
- Bundles **speaker diarization (Pyannote)** and **VAD (Silero)**, so Local transcripts are
  **speaker-attributed**, matching Ariso's participant/segment model.
- Models are CoreML bundles pulled from HuggingFace
  (`FluidInference/parakeet-tdt-0.6b-v3-coreml` and the diarizer/VAD bundles FluidAudio
  requires).

### Integration shape

The app is Rust/Tauri; FluidAudio is Swift. We build a small **Swift CLI sidecar**,
`ariso-stt`, that links FluidAudio and is bundled via Tauri `externalBin` (arm64). The Rust
app spawns it per recording. This isolates the Swift/CoreML dependency behind the same
swappable "transcriber" boundary instead of FFI-ing a Swift package into the Rust process.

**Sidecar contract:**

```bash
ariso-stt --audio <path> --models <dir> --format json
```

stdout (on success):

```json
{
  "language": "en",
  "durationSeconds": 2533.4,
  "participants": [{ "id": 0, "label": "Speaker 1" }, ...],
  "segments": [
    { "speaker": 0, "text": "…", "start": 3.2, "end": 9.1 },
    ...
  ]
}
```

Non-zero exit + stderr message on failure.

**Platform gate:** the Local backend (and its Settings card) is enabled only on Apple
Silicon + macOS 14+. Elsewhere the Local Transcription card shows "Requires Apple Silicon,
macOS 14+" and the download/record actions are disabled.

---

## 4. Storage layer (`storage.rs`, `~/.ariso/`)

```text
~/.ariso/
  run/                              # existing (MCP socket) — untouched
  models/
    parakeet-tdt-0.6b-v3-coreml/    # FluidAudio CoreML ASR bundle
    <diarizer / vad bundles>/       # pyannote, silero (as FluidAudio requires)
    manifest.json                   # repo, revision, file list, sizes/checksums
  recordings/
    2026-06-02T14-30-05Z/           # folder id = UTC timestamp (sortable, fs-safe)
      recording.mp3                 # audio (same lamejs mp3 as the Ariso path)
      transcript.md                 # speaker-attributed markdown
      meta.json                     # structured record (below)
```

- **Folder id** = UTC timestamp, filesystem-safe (`:` → `-`) and lexically sortable.
- **Title** = `Recording <local datetime>`, stored in `meta.json` — **not** the folder name,
  so a future rename never moves files.
- **`storage.rs`** centralizes all path resolution and refuses to operate if `HOME` is unset
  (same guard `main.rs` already uses for the MCP socket).

### `meta.json` (structured source of truth for the Library)

```json
{
  "id": "2026-06-02T14-30-05Z",
  "title": "Recording 2026-06-02 14:30",
  "createdAt": "2026-06-02T14:30:05Z",
  "durationSeconds": 2533,
  "status": "done",
  "language": "en",
  "participants": [{ "id": 0, "label": "Speaker 1" }],
  "modelVersion": "parakeet-tdt-0.6b-v3"
}
```

- `status` ∈ `recording | transcribing | done | failed`. A crash mid-transcribe leaves
  `failed` (or `transcribing`) on disk, so it shows in the Library rather than vanishing.
- **Notes-later seam:** notes become a sibling `notes.md` + a `notesStatus` field. No layout
  change required.

### `transcript.md` format

```markdown
---
title: Recording 2026-06-02 14:30
date: 2026-06-02T14:30:05Z
duration: "00:42:13"
participants: ["Speaker 1", "Speaker 2"]
---

**Speaker 1** [00:00:03]
Hello, thanks for joining…

**Speaker 2** [00:00:09]
Happy to be here…
```

YAML front-matter + `**Speaker N** [mm:ss]` blocks. Mirrors Ariso's participant/segment
model closely enough for a future importer to round-trip.

### Atomicity

Write `transcript.md` (and `meta.json` updates) to a temp file then rename. Flip
`status: done` only after `transcript.md` lands.

---

## 5. Backend interface (TS) & the path to a Rust trait

`useBackend()` returns the active backend implementing exactly this — names chosen to mirror
a future Rust `Backend` trait 1:1:

```ts
interface Readiness {
  ready: boolean;
  reason?: 'signed-out' | 'model-missing' | 'unsupported-platform';
}

interface RecordingMeta {
  startAt: string | null;
  endAt: string;
  durationSeconds: number;
  meetingId?: number; // ariso only
}

interface FinalizeResult {
  backend: 'ariso' | 'local';
  // ariso: { meetingId }   local: { id, title, status }
  [k: string]: unknown;
}

interface Backend {
  id: 'ariso' | 'local';
  needsAuth: boolean;            // ariso: true  · local: false
  usesMeetingPicker: boolean;    // ariso: true  · local: false
  isReady(): Promise<Readiness>; // ariso: session valid · local: model present
  finalizeRecording(blob: Blob, meta: RecordingMeta): Promise<FinalizeResult>;
}
```

- **`isReady()`** unifies the two readiness gates. The tray asks `backend.isReady()`; if not
  ready it shows Settings and emits a backend-specific prompt event (reusing the existing
  sign-in-prompt pattern).
- **Today:** `ArisoBackend.finalizeRecording` is TS (wraps existing `uploadAudio`);
  `LocalBackend.finalizeRecording` calls the single Rust command `local_finalize_recording`.

### Migration seam → Approach 1 (documented, enforced)

Because the views only ever touch this interface and never assume HTTP/S3/`meetingId`
semantics, Ariso's orchestration can later move into a Rust `ArisoBackend` behind a generic
`finalize_recording` command. At that point `useBackend()` collapses to a single
`invoke('finalize_recording', …)` for both backends and the trait lives entirely in Rust —
**no view changes required.**

To preserve this, the following must stay backend-agnostic (enforced in review):

- Views (`WaveformView`, tray gating logic) call only `Backend` methods — never
  `uploadAudio`, `api.request`, presign URLs, or `meetingId` directly.
- `RecordingMeta.meetingId` is the only Ariso-specific field; it is optional and ignored by
  Local. A future Rust trait carries it as an opaque per-backend context.
- The two prompt events (`show-sign-in-prompt`, `show-model-prompt`) are the only
  backend-specific UI branches outside the backend implementations themselves.

---

## 6. Local transcription pipeline (`transcribe.rs`)

Single Tauri command:

```rust
local_finalize_recording(audio: Vec<u8>, meta: RecordingMetaInput) -> Result<FinalizeResult, String>
```

Steps (atomic from the caller's perspective):

1. `storage::create_recording_dir()` → `recordings/<utc-ts>/`; write `recording.mp3`
   (the **same lamejs mp3 blob** the Ariso path produces — capture/encoding stays shared,
   only *finalize* diverges).
2. Write `meta.json` with `status: transcribing`.
3. `transcribe::run(audio_path, model_dir)` → spawn `ariso-stt` (§3), capture JSON.
4. Render `transcript.md` from segments (temp-write + rename).
5. Update `meta.json` → `status: done` (+ duration, participants, language, modelVersion).
6. Return `FinalizeResult { backend: "local", id, title, status }`.

**Failure handling:** any step fails → `meta.json` `status: failed` + error message;
**`recording.mp3` is retained** (no lost audio; supports future retry). Batch is fast, so the
UI needs only a single indeterminate "Transcribing…" state — no progress plumbing.

**Testability:** the sidecar path is injectable via an env override
(e.g. `ARISO_STT_BIN`), so `local_finalize_recording` is testable in CI against a stub
binary that emits canned JSON — no real model required.

---

## 7. Model management (`model_manager.rs`)

- `local_model_status() -> ModelStatus { state, version, bytesTotal, bytesDone }`
  where `state` ∈ `not_downloaded | downloading | ready | error`.
- `download_local_model()` — streams the HF-pinned files (repo + revision) into
  `~/.ariso/models/`, verifies checksums, writes `manifest.json`, emits
  `model://progress`, `model://done`, `model://error`. Re-downloading fills only missing
  files.
- **Ready** = `manifest.json` present + all listed files present + checksums match.

---

## 8. Settings UI

New **Transcription Backend** section at the top of `SettingsView.vue`:

- Selector **Ariso · Local**, persisted to `settings.json` key `backend` (default `ariso`).
  Switching writes the key and emits a sync event so the tray and other windows pick it up
  (same mechanism as the notifications sync).
- `backend === 'ariso'` → show the existing **Account** card (Google login) unchanged; hide
  the Local card.
- `backend === 'local'` → hide Account (no login); show **Local Transcription** card:
  - Status line driven by `local_model_status()`: `Not downloaded` / `Downloading 45%` /
    `Ready (v3, 612 MB)`.
  - **Download model** button → `download_local_model`; shows progress; disabled while
    downloading.
  - On unsupported platform: "Requires Apple Silicon, macOS 14+"; actions disabled.

The existing **Audio** (recording mode), **Notifications**, and **About** sections remain for
both backends.

---

## 9. Tray gating & Library window

### Tray

- **Idle menu** gains **`Library…`** beside `Settings…`. Recording menu unchanged.
- **Start Recording** handler generalizes: read `backend` from `settings.json` → compute
  readiness (`ariso`: `is_session_valid`; `local`: model ready).
  - **Not ready** → show Settings + emit prompt (`tray://show-sign-in-prompt` for Ariso,
    `tray://show-model-prompt` for Local).
  - **Ready** → `ariso`: open the meeting-picker as today; `local`:
    `start_recording_window(None)` directly (skip picker; timestamp title).

### WaveformView

- On stop, route through `backend.finalizeRecording(blob, meta)` instead of calling
  `uploadAudio` directly.
- Status text adapts: Ariso "Uploading… / Upload successful"; Local "Transcribing… /
  Transcribed ✓". Then **Close** (the Library is the access path — no reveal button).

### Library window

- `create_library_window` opens a `library` window titled "Library" (`/#/library`).
- `LibraryView.vue` calls `list_local_recordings()` → reads every `recordings/*/meta.json`,
  sorted newest-first → **read-only** rows: title · date · duration · status badge.
- **No search, no click-to-open** in v1. Empty state: "No recordings yet."
- `list_local_recordings() -> Vec<RecordingSummary>` lives in `storage.rs`.

---

## 10. Ariso path & cross-cutting concerns

### Ariso unchanged

Login, meeting-picker, presign/PUT/confirm, and Pusher notifications are all identical. The
only insertions are: (a) `WaveformView` routing through `backend.finalizeRecording` — which
for Ariso just calls the existing `uploadAudio`; and (b) the tray reading `backend` first
(default `ariso` ⇒ behavior identical for current users).

### Naming collision (explicit)

The cargo features `dev-api` / `prod-api` / none(=localhost) are a **build target for the
Ariso server** and are **orthogonal** to the new **runtime backend** (`ariso` vs `local`).
"Local backend" is a runtime user choice; "local dev API" is a build flag. The Ariso backend
still uses `API_BASE_URL` / `WEB_APP_BASE_URL` per build flag.

### Errors

- Model download failure → `error` state + retry.
- Transcription failure → `meta.json` `status: failed`, audio retained.
- Sidecar missing/crash → surfaced as `failed` with the stderr message.
- `HOME` unset → clean command error (no panic).

### Build / dev

- README section: build the Swift sidecar with `swift build -c release`, producing
  `ariso-stt-aarch64-apple-darwin` in `src-tauri/binaries/`.
- Declared in `tauri.conf.json > bundle > externalBin` + the shell capability for spawning
  it.
- Sidecar build is **mac-arm-only**; CI building other targets skips it.

### Testing

- **Rust unit tests:** path resolution, folder naming, `meta.json` (de)serialization,
  markdown rendering from segments, manifest verification.
- **Pipeline test:** `local_finalize_recording` against a stub sidecar (`ARISO_STT_BIN`
  override) — no real model in CI.
- **TS tests:** `useBackend()` selection + finalize routing; `LibraryView` rendering from
  mocked `list_local_recordings()`.
- Existing recorder vitest untouched.

---

## 11. New/changed surface (summary)

**Rust (new):** `storage.rs`, `transcribe.rs`, `model_manager.rs`; commands
`local_finalize_recording`, `list_local_recordings`, `local_model_status`,
`download_local_model`, `create_library_window`. Sidecar crate/target `ariso-stt` (Swift).

**Rust (changed):** `tray.rs` (Library item + backend-aware gating), `main.rs` (register new
commands), `tauri.conf.json` (externalBin + capability).

**TS (new):** `composables/useBackend.ts` (+ `ArisoBackend`, `LocalBackend`),
`views/LibraryView.vue`, route `/library`.

**TS (changed):** `WaveformView.vue` (route through `finalizeRecording`),
`SettingsView.vue` (backend selector + Local Transcription card), `tauri.ts` (new invoke
wrappers), `main.ts` (register `/library` route).

**Storage:** `~/.ariso/{models,recordings}/`.
