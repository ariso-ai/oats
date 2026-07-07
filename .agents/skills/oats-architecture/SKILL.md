---
name: oats-architecture
description: Use to orient in the oats codebase or plan cross-cutting features — the cloud-vs-offline backend split, the speech-to-text pipeline, window topology, and where specs live.
---

# oats Architecture Map

oats is a macOS menu-bar meeting recorder/notetaker: hit record, get a transcript and
notes. Tauri v2 (Rust) + Vue 3 (TypeScript). Apple Silicon only.

## Two backends, one app

The defining design choice is a **backend abstraction** (`src/composables/useBackend.ts`
on the frontend; STT/storage modules on the Rust side):

- ☁️ **Cloud (Ariso)** — sign in; transcription and notes run on Ariso's servers.
- 🔒 **Offline (local)** — flip a switch; recording, transcription, speaker labels, and
  notes all run on-device, nothing leaves the Mac.

When designing a feature, ask which backend(s) it touches and keep the abstraction intact —
don't hardcode cloud-only or local-only paths into shared code.

## Speech-to-text pipeline

Local transcription uses **`ariso-stt`**, a Swift/MLX speech engine shipped as a sidecar
binary (`src-tauri/binaries/ariso-stt-aarch64-apple-darwin`, plus `mlx-swift_Cmlx.bundle`).
Rust side: `audio_capture.rs` (Core Audio process taps for system audio + mic),
`transcribe.rs`, `model_manager.rs` (model download/management), `storage.rs` (recordings
+ meeting data on disk under `~/.ariso`).

## Window topology

All windows load one Vue bundle and render a hash route (`src/main.ts`):
- `main` (`/`) — **headless** BootstrapView; runs sync/startup logic, intentionally blank.
- `settings` (`/settings`) — the settings window (pre-created hidden at startup).
- `library` (`/library`) — titled **"Meetings"**; the main user-facing window
  (created on demand, destroyed on close).
- plus `waveform`, `update`, `meeting-picker`, `onboarding`, `oauth`.

The tray (`tray.rs`, `tray_meeting.rs`) and recorder pill (`recorder_pill.rs`) are the
menu-bar surface.

## Where things live

- Frontend: `src/` (`views/`, `composables/`, `tauri.ts`) — see `oats-vue`.
- Backend: `src-tauri/src/` — see `oats-tauri`.
- **Design specs: `docs/superpowers/specs/`** (`YYYY-MM-DD-<topic>-design.md`). Read
  recent specs before a related feature; write a new one (via the brainstorming workflow)
  before non-trivial work.

For runtime inspection of any window, use `oats-debugging`.
