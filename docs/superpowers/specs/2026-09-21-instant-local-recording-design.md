# Instant Local Recording (issue #220)

## Problem

On the Local (offline) backend, recording is currently gated on **both** on-device
models being fully downloaded — the STT/transcript model (FluidAudio-derived, macOS
MLX / Windows GGUF) and the notes LLM (Gemma). A brand-new user who switches to Local
and needs to record right away (e.g. at a conference on slow wifi) cannot: they hit
"Record" and instead of capturing audio, oats surfaces Settings and blocks until a
20+ minute download finishes. This defeats the point of an offline-first recorder —
the one thing it should always be able to do immediately is capture audio.

The gate is implemented once, centrally
(`commands::ensure_recording_allowed`, `src-tauri/src/commands.rs:1626-1643`), and
duplicated once more, independently, in the auto-record path
(`mic_monitor.rs:397-415`) — so both need to change together.

## Goal

Local recording starts immediately regardless of on-device model state, consistently
across every entry point (tray/menu "Record", Library's Record button and continue-
meeting flow, the meeting picker, and mic-monitor auto-record). Audio is always saved
right away (this already happens today — unaffected). A recording whose transcript
and/or notes can't be generated yet because a required model isn't downloaded shows an
explicit "pending" state in the Library row and the meeting detail panel, distinct from
both "actively generating" and "failed." Missing model downloads start in the
background (idempotently — safe to nudge on every recording attempt) without stealing
window focus. The moment a model finishes downloading, every recording that was
waiting on it resumes automatically — no user action required.

## Non-goals

- Changing anything about the Ariso (cloud) backend's recording-start gate
  (`ensure_recording_allowed`'s non-local branch, the session/sign-in check) —
  untouched.
- `append_recording_core` (the "continue this meeting" clip-append path): it only ever
  targets a recording whose status is already `Done`, which is only reachable once STT
  successfully ran once — i.e. STT was ready at least once already, and model files are
  never deleted out from under a running app. So an append can never hit "STT not
  ready." No change needed there (see Design).
- The mid-recording checkpoint pipeline (`transcribe.rs`'s `checkpoint_core`,
  issue #123's preview-notes machinery): it already treats an STT failure on a chunk
  as non-fatal (`preview.last_error`, chunk skipped, "the final pass will cover it" —
  `transcribe.rs:515-530`). Once the STT model finishes downloading mid-recording,
  later checkpoints simply start succeeding. No change needed.
- A remote-model / bring-your-own-key notes path (`2026-09-19-local-notes-model-picker-design.md`)
  — orthogonal, not yet implemented on this branch's base; this spec's notes-model
  readiness check is written against the single on-device Gemma path that exists today.
- Resuming a download automatically on every app launch just because Local is active.
  Downloads already only start when something asks for them (first-switch confirm,
  a Settings "Install" click, or — after this change — any recording attempt while a
  model is missing); this spec keeps that trigger model rather than adding a
  startup-timer/background-service. It does add one narrow startup-time addition (see
  Design → Startup reconciliation) to pick up recordings left pending across a restart.

## Decisions

Clarify questions, decided non-interactively per the autopilot run (no human present);
`shawnzhu`'s trusted comment ("model download should not block recording... transcript/
notes will be pending generation until models files are fully downloaded") confirms the
overall shape but doesn't settle these implementation-level questions, so all five are
defaults derived from reading the existing gate, pipeline, and status-model code
(`commands.rs`, `mic_monitor.rs`, `model_manager.rs`, `transcribe.rs`, `storage.rs`).

1. **How is "blocked on a missing model" represented in the data model — a new
   first-class status, or reuse of the existing `Failed`/`error`-sentinel pattern?**
   Default (chosen): two new first-class variants — `RecordingStatus::PendingModels`
   (transcript blocked on the STT model) and `NotesStatus::PendingModel` (notes blocked
   on the notes model) — mirroring the existing `NotesStatus::EmptyTranscript` sentinel
   pattern (a `notes_error` string the derive function recognizes) rather than
   overloading `Failed`. Reusing `Failed` would show "Transcript failed" / "AI Notes
   failed" with a manual Retry button for something that isn't a fault and resolves on
   its own — actively misleading.
2. **What triggers a pending recording to resume once the model it was waiting on
   finishes downloading?** Default (chosen): hook it into the download commands
   themselves (`model_manager::download_local_stt` / `download_local_llm`), Rust-side,
   on their own success path — scan `storage::list_recordings` for the matching pending
   state and call `retry_transcription_core` / `retry_notes_core` for each, spawned
   detached. This works regardless of which window (if any) is open or focused, matches
   this codebase's existing pattern of Rust owning all on-disk state transitions (no
   precedent anywhere of Rust listening to its own emitted events), and reuses the
   already-tested retry functions instead of adding a second execution path.
3. **Does starting a local recording while a model is missing still open/focus
   Settings, the way the old blocking gate did?** Default (chosen): no — that was the
   right UX for a hard block, but would now interrupt a recording that already
   succeeded. Keep nudging the download in the background (idempotent via the existing
   `DownloadGuard`, so calling it on every attempt is harmless) by emitting the same
   `tray://show-model-prompt` event the old gate used. Settings is pre-created and
   mounted at startup (hidden) regardless of visibility, so its existing listener still
   catches the event and calls `startMissingDownloads()` — the event just no longer
   also raises the window.
4. **Does auto-record's Record/Dismiss prompt behavior change?** Default (chosen): no
   — only the `local_models_ready()` skip-and-`return` check in `mic_monitor.rs`'s
   `Action::Start` branch is deleted (replaced with the same non-blocking nudge from
   decision 3). A detected meeting is never silently skipped for model reasons again;
   the existing prompt-or-auto-record logic is otherwise unchanged.
5. **Exact copy for the new pending states?** Default (chosen), see Design → Frontend
   copy below — Settings banner drops the blocking framing; Library row gets a distinct
   "Waiting for on-device models…" label (not reused from the generic "Processing…",
   and deliberately **not** time-bounded the way the existing notes-pending inference
   is, since waiting on a slow download can legitimately run well past an hour); detail
   panel gets two new non-terminal, no-Retry chip stages.

## Design

### Rust — `storage.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RecordingStatus {
    Recording,
    Transcribing,
    Done,
    Failed,
    /// Capture finished and audio is saved, but the on-device STT model isn't
    /// downloaded yet, so transcription hasn't started. Resolved automatically
    /// (see `retry_recordings_pending_stt`) once the model finishes downloading —
    /// never a manual Retry target.
    #[serde(rename = "pending-models")]
    PendingModels,
}
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NotesStatus {
    Pending,
    Ready,
    Failed,
    #[serde(rename = "empty-transcript")]
    EmptyTranscript,
    /// The transcript exists, but the on-device notes model isn't downloaded
    /// yet. Resolved automatically once it finishes (`retry_recordings_pending_llm`).
    #[serde(rename = "pending-model")]
    PendingModel,
}

pub const MODELS_NOT_READY_NOTES_ERROR: &str =
    "on-device notes model not downloaded yet — notes generation pending";

pub fn derive_notes_status(...) -> NotesStatus {
    if notes_in_progress { NotesStatus::Pending }
    else if has_note { NotesStatus::Ready }
    else if notes_error == Some(NO_SPEECH_NOTES_ERROR) { NotesStatus::EmptyTranscript }
    else if notes_error == Some(MODELS_NOT_READY_NOTES_ERROR) { NotesStatus::PendingModel } // new
    else if notes_error.is_some() { NotesStatus::Failed }
    else { NotesStatus::Pending }
}
```

`reconcile_interrupted_recordings`'s `match meta.status { Recording => .., Transcribing
=> .., _ => {} }` (`storage.rs:743-782`, runs once at app startup) needs **no change**:
its catch-all already leaves `PendingModels` untouched, which is correct —
unlike `Recording`/`Transcribing`, `PendingModels` isn't "something was actively
running when oats quit," it's a legitimate at-rest state that must survive a restart
unchanged.

Both new variants are additive to enums that are never matched exhaustively elsewhere
in the codebase (confirmed by grep: every other use is `==`/`!=` or an
`Recording | Transcribing` two-way `matches!`, never a full match) — no other call site
needs updating to keep compiling. In particular the delete/rename "still being
processed" guards (`commands.rs:2417-2420`, `commands.rs:2563-2566`,
`matches!(meta.status, RecordingStatus::Recording | RecordingStatus::Transcribing)`)
deliberately do **not** gain `PendingModels`: nothing is actively writing to a pending
recording, so deleting or renaming one is exactly as safe as doing so to a `Failed`
one, and both already fall through those guards untouched.

The second, `Done`-scoped part of those same two guards
(`commands.rs:2424-2437`, `commands.rs:2575-2585`) blocks delete/rename only when
`notes_status == NotesStatus::Pending` ("AI notes are still generating... try again
once they finish"). `NotesStatus::PendingModel` is a distinct variant, so this `==`
check does not match it — delete/rename are allowed while notes are pending on the
model, same reasoning as above: `process_notes` sets `notes_in_progress = false` in
that branch, so nothing is mid-write. This is intentional, not a gap to close.

### Rust — `transcribe.rs`

**Implementation strategy — reclassify on failure, don't pre-check.** `fresh_recording_core`,
`finalize_core`/`finalize_core_with_target`, and `retry_transcription_core` are exercised
directly (bypassing any command layer, with a stubbed sidecar via the `ARISO_STT_BIN`
test seam) by ~40 existing tests in this file, none of which set up a real STT-ready
marker in their tempdir — they rely purely on the stub's exit code/stdout, not on
`model_manager::is_ready`. Gating *before* the `run_transcribe`/`run_notes` call (i.e.
skip the call entirely when not ready) would silently flip every one of those tests
from exercising real transcribe/notes logic to hitting the new pending branch instead,
since none of their tempdirs have a `manifest.json`. Gating *after* an actual failure
instead — attempt the call exactly as today, and only when it returns `Err` ask "is
this because the model isn't downloaded, or a genuine failure?" — preserves every
existing success-path test unchanged (they never reach the `Err` arm) and only touches
tests that specifically stub a *failure* and then assert the resulting status. A repo
grep confirms there's exactly one such test to update (see Testing). This is also
cheap at runtime: a sidecar invoked against a missing/incomplete model directory fails
fast (missing files), not after real model-load work.

**`fresh_recording_core`** (`transcribe.rs:633-743`), inside the existing
`match run_transcribe(&audio_path, &models).await { ... }`, change only the `Err` arm
(`:736-741`):

```rust
Err(e) => {
    if !crate::model_manager::is_ready(&root) {
        meta.status = RecordingStatus::PendingModels;
        let _ = storage::write_meta(&dir, &meta);
        return Ok((
            FinalizeResult {
                backend: "local".to_string(), id, title,
                status: RecordingStatus::PendingModels,
            },
            tokio::spawn(async {}), // nothing to await
        ));
    }
    meta.status = RecordingStatus::Failed;
    meta.error = Some(e.clone());
    let _ = storage::write_meta(&dir, &meta);
    Err(e)
}
```

(`root` is already in scope a few lines up, from the existing
`let models = storage::models_dir(&storage::ariso_root()?);` — bind it to a local
`root` first so both that line and the new check can use it.) The `Ok` arm is
untouched. Nothing here calls `download_local_stt`/`download_local_llm` — the download
was already nudged when the recording *started* (decision 3), not when it finalizes,
so by Stop time it's either already in flight or already done.

**`process_notes`** (`transcribe.rs:264-377`), same strategy, inside the existing
`match outcome { ... }`, change only the `Err` arm (`:351-355`):

```rust
Err(e) => {
    eprintln!("notes generation: {e}");
    let models_ready = crate::storage::ariso_root()
        .is_ok_and(|root| crate::model_manager::llm_is_ready(&root));
    meta.notes_error = Some(if models_ready {
        e
    } else {
        storage::MODELS_NOT_READY_NOTES_ERROR.to_string()
    });
    let _ = storage::write_meta(&dir, &meta);
}
```

`meta.notes_in_progress = false` a few lines above this match is untouched — it already
runs unconditionally before the match, exactly as today.

**New retry-scan helpers**, next to `retry_transcription_core`/`retry_notes_core`:

```rust
/// Resume every local recording that finished capture but couldn't transcribe
/// because the STT model wasn't downloaded yet. Called after
/// `download_local_stt` succeeds; spawns each retry detached so a slow
/// transcription doesn't block the download command's own return.
pub async fn retry_recordings_pending_stt(root: &Path) {
    let Ok(recordings) = storage::list_recordings(root) else { return };
    for r in recordings {
        if r.status == storage::RecordingStatus::PendingModels {
            let root = root.to_path_buf();
            tauri::async_runtime::spawn(async move {
                let _ = retry_transcription_core(&root, &r.id).await;
            });
        }
    }
}

/// Resume notes generation for every local recording whose transcript exists
/// but whose notes were blocked on the LLM model. Called after
/// `download_local_llm` succeeds.
pub async fn retry_recordings_pending_llm(root: &Path) {
    let Ok(recordings) = storage::list_recordings(root) else { return };
    for r in recordings {
        if r.notes_status == storage::NotesStatus::PendingModel {
            let root = root.to_path_buf();
            tauri::async_runtime::spawn(async move {
                let _ = retry_notes_core(&root, &r.id).await;
            });
        }
    }
}
```

(`r.id` — capture by value before the `move` closure per the existing borrow shape used
elsewhere in this file; adjust as needed to satisfy the borrow checker.)

### Rust — `model_manager.rs`

In `download_local_stt`'s `Ok(())` arm (`model_manager.rs:270-274`), after emitting
`model://stt/done`, spawn the STT retry-scan:

```rust
Ok(()) => {
    let _ = app.emit("model://stt/done", ());
    let root2 = root.clone();
    tauri::async_runtime::spawn(async move {
        crate::transcribe::retry_recordings_pending_stt(&root2).await;
    });
    Ok(())
}
```

Symmetrically in `download_local_llm`'s `Ok(())` arm (`model_manager.rs:458-462`), spawn
`retry_recordings_pending_llm`.

### Rust — `commands.rs`

Replace the blocking local branch of `ensure_recording_allowed`
(`commands.rs:1626-1643`) with a non-blocking nudge, and rename/slim
`surface_model_download` (`commands.rs:149-152`) since it no longer needs to touch a
window:

```rust
/// Emit `tray://show-model-prompt` so the (always-mounted) Settings window
/// auto-starts any missing on-device model download. Safe to call on every
/// local recording attempt: the per-target `DownloadGuard`s de-dupe a download
/// already in flight. Deliberately does not open/focus Settings — recording
/// itself is never blocked on this anymore.
pub(crate) fn request_local_model_downloads(app: &tauri::AppHandle) {
    let _ = app.emit("tray://show-model-prompt", ());
}

async fn ensure_recording_allowed(app: &tauri::AppHandle) -> bool {
    if active_backend(app) == "local" {
        if !local_models_ready() {
            request_local_model_downloads(app);
        }
        return true;
    }
    if is_session_valid(app).await {
        return true;
    }
    let _ = open_settings_window(app);
    let _ = app.emit("tray://show-sign-in-prompt", ());
    false
}
```

The Ariso (non-local) branch is untouched.

### Rust — `mic_monitor.rs`

Delete the skip-and-`return` block in `Action::Start` (`mic_monitor.rs:397-410`),
replacing it with a non-blocking nudge that does **not** return early:

```rust
if crate::commands::active_backend(&app2) == "local"
    && !crate::commands::local_models_ready()
{
    crate::commands::request_local_model_downloads(&app2);
}
```

(`request_local_model_downloads` only emits an event, so the `run_on_main_thread`
wrapping the old `surface_model_download` call is no longer needed here.) Execution
falls through to the existing `prompt_auto_record` call exactly as it does today when
models are already ready.

### Rust — startup reconciliation (`main.rs`)

Right after the existing `reconcile_interrupted_recordings` call
(`main.rs:311-318`), add a best-effort resume sweep for the local backend, covering the
case where oats was closed while a download was in flight and, by the next launch,
happens to already be ready (e.g. it silently finished, or the user re-triggers it via
Settings and this spec's existing per-download hook doesn't apply because the app
wasn't running to receive the `Ok(())`):

```rust
if let Ok(root) = crate::vault::meta_root() {
    if crate::model_manager::is_ready(&root) {
        tauri::async_runtime::spawn(async move {
            crate::transcribe::retry_recordings_pending_stt(&root).await;
        });
    }
    if crate::model_manager::llm_is_ready(&root) {
        let root2 = root.clone();
        tauri::async_runtime::spawn(async move {
            crate::transcribe::retry_recordings_pending_llm(&root2).await;
        });
    }
}
```

This is additive robustness, not required for the primary flow (which is fully covered
by the per-download hooks in `model_manager.rs`) — safe to defer if it turns out to
complicate the plan disproportionately, but cheap enough to include.

### Frontend — `src/tauri.ts`

Extend the two status literal unions (everything else — `RecordingStatusView.status`,
`MeetingListItem.status` in `useBackend.ts` — derives from
`RecordingSummary['status']`, so this is the only edit needed for the wire type):

```ts
export interface RecordingSummary {
  ...
  status: 'recording' | 'transcribing' | 'done' | 'failed' | 'pending-models';
  ...
}
export type NotesStatus = 'pending' | 'ready' | 'failed' | 'empty-transcript' | 'pending-model';
export interface LocalFinalizeResult {
  ...
  status: 'recording' | 'transcribing' | 'done' | 'failed' | 'pending-models';
}
```

### Frontend — `src/composables/useLocalRecordingProgress.ts`

```ts
export type LocalProgressStage =
  | 'idle' | 'recording' | 'transcribing'
  | 'pending-models'        // new: transcript blocked on STT
  | 'transcript-failed'
  | 'notes-pending'
  | 'notes-pending-model'   // new: notes blocked on LLM
  | 'notes-failed'
  | 'notes-empty-transcript'
  | 'ready';

export function deriveStage(s: RecordingStatusView | null): LocalProgressStage {
  if (!s) return 'idle';
  if (s.status === 'failed') return 'transcript-failed';
  if (s.status === 'pending-models') return 'pending-models';
  if (s.status === 'recording') return 'recording';
  if (s.status === 'transcribing') return 'transcribing';
  // status === 'done'
  if (s.notesStatus === 'pending') return 'notes-pending';
  if (s.notesStatus === 'pending-model') return 'notes-pending-model';
  if (s.hasNote || s.notesStatus === 'ready') return 'ready';
  if (s.notesStatus === 'empty-transcript') return 'notes-empty-transcript';
  if (s.notesStatus === 'failed') return 'notes-failed';
  return 'notes-pending';
}

const IN_FLIGHT_STAGES: readonly LocalProgressStage[] =
  ['recording', 'transcribing', 'pending-models', 'notes-pending', 'notes-pending-model'];
```

(Both new stages join `IN_FLIGHT_STAGES` so the 2s poll keeps ticking while waiting —
that's how the UI notices the automatic resume without the user doing anything.)

The `retryTranscription`/`retryNotes` optimistic-status object literals need no change:
they're unreachable for these two new stages since (per decision 5 / the chip design
below) no Retry button is ever shown for them.

### Frontend — `src/views/MeetingDetailView.vue`

`showStatusChip` (`:815-819`): add `'pending-models'` and `'notes-pending-model'` to the
local stage list.

`statusGenerating` (`:823-828`): add
`|| progress.stage.value === 'pending-models' || progress.stage.value === 'notes-pending-model'`
— spinner, no Retry (via the existing `showRetry = !statusGenerating.value && ...`).

`statusLabel` (`:829-844`): two new switch cases:

```ts
case 'pending-models':
  return 'Waiting for on-device models to finish downloading…';
case 'notes-pending-model':
  return 'Waiting for the notes model to finish downloading…';
```

`TERMINAL_STAGES` (`:870-875`): unchanged — both new stages are deliberately excluded
(non-terminal), consistent with them joining `IN_FLIGHT_STAGES`.

### Frontend — `src/views/LibraryView.vue`

`rowProcessingLabel` (`:904-928`): add a distinct label, checked before the existing
generic "Processing…" cases, and **not** run through `finishedRecently`'s one-hour
bound (see decision 5 — waiting on a slow download is expected to outlast that window):

```ts
const PENDING_MODELS_LABEL = 'Waiting for on-device models…';
...
function rowProcessingLabel(m: MeetingListItem): string | null {
  if (activeBackend.value?.id === 'local') {
    if (m.status === 'pending-models') return PENDING_MODELS_LABEL;
    if (m.status === 'recording' || m.status === 'transcribing') return PROCESSING_LABEL;
    if (
      m.status === 'done' &&
      m.files?.hasTranscript &&
      !m.files.hasNote &&
      m.files.notesStatus === 'pending-model'
    ) {
      return PENDING_MODELS_LABEL;
    }
    if (
      m.status === 'done' &&
      m.files?.hasTranscript &&
      !m.files.hasNote &&
      (m.files.notesStatus ?? 'pending') === 'pending'
    ) {
      return finishedRecently(m) ? PROCESSING_LABEL : null;
    }
    return null;
  }
  return processingMeetings.isProcessing(m.id) ? PROCESSING_LABEL : null;
}
```

`MeetingDetailView.vue`'s existing `content-ready` emit (keyed on crossing into
`TERMINAL_STAGES`) already covers refreshing this row once a pending recording resolves
— no separate wiring needed.

### Frontend — `src/views/SettingsView.vue`

Replace the blocking banner copy (`:107-109`):

```
- Both on-device models must finish downloading before you can record.
+ Recording works right away. On-device models are finishing their download in
+ the background — transcripts and notes for new recordings will be generated
+ once they're ready.
```

`showModelBanner`/`modelBannerVisible` logic (`settingsDownload.ts:63-69`) stays as-is
— it already shows the banner whenever prompted and either model is incomplete, which
is still the right condition; only the wording it introduces changes. Update the doc
comments in `settingsDownload.ts` and around `startMissingDownloads`
(`SettingsView.vue:776-783`) that describe this as "the recording gate" — it isn't a
gate anymore, just a background nudge; reword to avoid leaving stale, misleading
comments.

### Frontend — `src/composables/recordingStartError.ts`

The `/sign-in required/i` branch (`:34-36`) is now reachable only from the Ariso
session-gate path (the local branch of `ensure_recording_allowed` never returns
`false`/errors anymore). Drop the local-models clause from its copy:

```
- return 'Recording is not ready. Sign in or finish installing the local models in Settings, then try again.';
+ return 'Recording is not ready. Sign in to Ariso in Settings, then try again.';
```

## Cloud vs offline

Local-only change end to end. The Ariso backend's `ensure_recording_allowed` branch,
its session gate, and its meeting-picker flow are untouched; `RecordingStatus`/
`NotesStatus` are Local-only types with no cloud equivalent.

## Error handling

| Situation | Behavior |
| --- | --- |
| Recording stops, STT model not downloaded | Audio saved (unchanged); `run_transcribe` is attempted and fails fast (missing model files) exactly as any STT failure does today, but is reclassified: `meta.status = PendingModels` instead of `Failed`; Library row + detail chip show "Waiting for on-device models…"; auto-resumes when `download_local_stt` next succeeds. |
| Recording stops, STT ready but notes/LLM model not downloaded | Transcript generates normally (`status = Done`); `run_notes` is attempted and fails fast, reclassified: `process_notes` sets `notes_error = MODELS_NOT_READY_NOTES_ERROR` instead of the raw sidecar error; row/chip show the notes-specific pending copy; auto-resumes when `download_local_llm` next succeeds. |
| Both models missing | Both behaviors apply independently — transcript is `PendingModels` first; once STT finishes and transcription runs, if the notes model is *still* missing, `process_notes` (spawned from the now-successful `fresh_recording_core`) attempts notes, fails fast, and lands in the `MODELS_NOT_READY_NOTES_ERROR` case. |
| Model download itself fails (network error, corrupt download) | Unchanged existing behavior (`model://stt/error` / `model://llm/error`, Settings shows "Download failed"). A recording stays `PendingModels`/notes-`pending-model` until the user retries the install from Settings — this spec adds no new give-up path, matching "recording works, the rest resolves whenever the download eventually succeeds." |
| User manually deletes a `PendingModels` recording | Falls through the existing delete guard exactly like a `Failed` recording (guard only blocks `Recording`/`Transcribing`) — succeeds immediately, no special-casing needed. If a delayed retry-scan spawn already fired for that id before the delete, `retry_transcription_core` re-reads meta from a now-missing directory and returns an `Err` that its caller (the detached spawn) already discards — no crash, no resurrected recording. |
| App restarted while `PendingModels` | `reconcile_interrupted_recordings`'s catch-all leaves it untouched (correct — it's an at-rest state, not an interrupted one); the new startup sweep additionally retries it immediately if the model in question is, by then, already ready. |
| Two recordings both `PendingModels` when a download finishes | Both retried independently, each its own spawned task — matches the existing "retry is per-id, no shared state" shape of `retry_transcription_core`. |

## Testing

**Rust (`storage.rs`)**: extend the existing marker-combination style tests —
`derive_notes_status` returns `PendingModel` for `notes_error ==
Some(MODELS_NOT_READY_NOTES_ERROR)` (checked ahead of the generic `Failed` branch, same
as the existing `EmptyTranscript` case); `reconcile_interrupted_recordings` leaves a
`PendingModels` recording's status untouched (extend
`reconcile_fails_recordings_interrupted_by_a_quit`'s fixture with a `PendingModels`
entry and assert it survives unchanged, mirroring how `Done` already does).

**Rust (`transcribe.rs`)**: `fresh_recording_core` (via `finalize_core`) against a
tempdir with a failing stub and no STT manifest present → returns `Ok` with
`status: PendingModels`, writes the audio attachment (assert the vault attachment
round-trips exactly as `finalize_writes_transcript_and_marks_done` already checks for
the success case), does not create `transcript.md`. Same setup but *with* a manifest
present (`model_manager::write_manifest(root, &model_manager::stt_model_version())`) →
still `Failed` with the stub's error (regression guard for the reclassify branch).
`process_notes` (via `finalize_core` with a notes-failure stub) against a tempdir with
no LLM `.complete` marker → `notes_error == Some(MODELS_NOT_READY_NOTES_ERROR)`; same
setup with the marker present → `notes_error` is the stub's raw error text (unchanged
existing behavior). `retry_recordings_pending_stt`/`retry_recordings_pending_llm`: seed
a tempdir with a mix of statuses, write the readiness marker, call the retry-scan,
assert only the matching recording(s) got re-processed (poll/await the spawned tasks
the way existing tests await notes `JoinHandle`s, or assert on resulting `meta.json`
state after a short `tokio::time` yield/join). Extend `commands.rs`'s delete/rename
guard tests with a `PendingModels` fixture asserting delete/rename succeed (mirroring
the existing `Failed`-fixture cases), and a `notes_status == PendingModel` fixture
asserting delete/rename succeed despite `Done` status (mirroring the existing
non-`Pending` `Done` cases).

**Regression note**: `transcribe.rs`'s existing
`finalize_marks_failed_but_keeps_audio_on_stt_error` test stubs a transcribe failure
against a tempdir with no manifest — after this change it would flip from `Failed` to
`PendingModels` unless updated. Fix it by writing a real STT-ready manifest in its
setup (`model_manager::write_manifest(tmp.path(), &model_manager::stt_model_version())`
right after the existing `ARISO_ROOT` env-var line), which correctly narrows its intent
to "STT is ready but the sidecar itself failed" — matching what the test's name and
assertions (`meta.status == Failed`, `meta.error.is_some()`) already claim to test. No
other existing test needs this: `append_stt_failure_leaves_target_untouched_and_saves_failed_clip`
exercises `save_failed_clip`, a different, unchanged code path (see Non-goals); the
three existing notes-failure tests (`failed_notes_also_clear_notes_in_progress`,
and the two `retry_notes_core` failure tests) only assert `notes_error.is_some()`, which
still holds under the new `MODELS_NOT_READY_NOTES_ERROR` classification.

**Rust (`model_manager.rs`)**: existing readiness/marker tests are unaffected structurally;
no new coverage needed there beyond confirming `download_local_stt`/`download_local_llm`
still compile with the added spawn (a smoke-level check, not new marker-combination
tests — the retry-scan itself is tested in `transcribe.rs`).

**Vitest (`useLocalRecordingProgress.test.ts`)**: extend `deriveStage`'s exhaustive
table with `status: 'pending-models'` → `'pending-models'`, and `status: 'done',
notesStatus: 'pending-model'` → `'notes-pending-model'`; assert both are in
`IN_FLIGHT_STAGES` (polling continues).

**Vitest (`MeetingDetailView.test.ts`)**: chip renders "Waiting for on-device models to
finish downloading…" with a spinner and no Retry button for `pending-models`; renders
"Waiting for the notes model to finish downloading…" for `notes-pending-model`; neither
counts as a `TERMINAL_STAGES` transition (no spurious `contentReady` emit while pending).

**Vitest (`LibraryView.test.ts`)**: local row shows "Waiting for on-device models…" for
`status: 'pending-models'` and for `status: 'done'` with `hasTranscript && !hasNote &&
notesStatus === 'pending-model'`; this label is not affected by `finishedRecently`
timing (assert it still shows well past the existing one-hour window a plain `pending`
row would drop at).

**Vitest (`SettingsView.download.test.ts`)**: no functional change to
`shouldPromptDownload`/`pendingInstalls`/`modelBannerVisible` — existing tests should
keep passing unmodified; add a template-level assertion (or a small `SettingsView.test.ts`
snapshot/text check if one exists) that the banner no longer says a model download
"must finish... before you can record."

**Vitest (`recordingStartError.test.ts`)**: update the existing `/sign-in required/i`
case's expected copy to the reworded, Ariso-only message.

**Vitest (`LibraryView.test.ts` / `MeetingPickerView.test.ts`)**: assert
`start_recording_window` is still invoked (and resolves) for the Local backend when
`local_model_status` mocks report models missing — i.e. the frontend no longer expects
or handles a rejection for this case on the local path.

**Manual** (called out per the autopilot contract as unverifiable in CI — no running
app, no real model download): with Local selected and both on-device models deleted/
never installed, record a short meeting from the tray, from Library, and by letting
auto-record trigger — in each case confirm the pill shows its normal success checkmark
(not an error), the Library row shows "Waiting for on-device models…", and Settings
(opened separately) shows both models downloading. Let a download finish and confirm
the row and detail chip pick up "Generating Transcript" → eventually "Ready" without
any user action.

## Acceptance criteria (from the issue + `shawnzhu`'s trusted comment)

- [ ] Local recording is never blocked by missing/incomplete on-device model
      downloads, at every start entry point: tray/menu "Record", Library's Record
      button and "Continue this meeting," and mic-monitor auto-record.
- [ ] Audio is saved immediately regardless of model state (already true today;
      preserved).
- [ ] A recording whose transcript and/or notes can't be generated yet because a
      model is missing shows as pending in the Library (row label) and in the meeting
      detail view (status chip) — visually distinct from "generating" and from
      "failed."
- [ ] Missing model downloads start (or continue) in the background without blocking
      or re-blocking recording.
- [ ] Once the required model(s) finish downloading, transcription and notes
      generation resume and complete automatically, with no manual retry needed.
- [ ] Settings copy no longer claims recording is blocked on model downloads.
