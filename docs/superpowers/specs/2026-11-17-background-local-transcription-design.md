# Allow starting the next meeting while the previous transcript processes (issue #521)

## 1. Problem

Users who move quickly from one meeting to the next can be blocked from
starting the next recording while oats is still processing the previous
one's transcript.

Only one recorder-pill window (`"waveform"` label) exists at a time
(`src-tauri/src/recording_state.rs`, `src-tauri/src/commands.rs`
`open_waveform_window`). A new recording request while the pill is still up
asks it to "yield" (`recorder://yield`), and the pill only stands down once
it is in a settled post-upload state — `WaveformView.vue`'s `handleYield`
refuses while `isUploading.value || inFlightFinalize`. That part of the
design is sound and deliberate (issue #313/#320): it exists so a stale pill
can't silently swallow a new recording.

The actual blocker is how long that settled state takes to arrive for the
**local (offline) backend**. `local_finalize_recording` →
`finalize_core_with_target` → `fresh_recording_core` /
`append_recording_core` (`src-tauri/src/transcribe.rs`) run the full
on-device STT pass (`run_transcribe`, the `ariso-stt` sidecar) **inside**
the `async fn` the frontend awaits, before returning a result. Only AI-notes
generation is already detached (`tokio::spawn(process_notes(...))`, handle
dropped). `WaveformView.vue`'s `runFinalize` awaits that one call
(`backend.value.finalizeRecording(...)`) and only flips to the pill's
"success" state — which starts the auto-close timer — once it resolves. For
a non-trivial meeting, on-device transcription of the full recording can
take anywhere from several seconds to a few minutes, and for that entire
window the next recording is blocked.

The **cloud (Ariso) backend** does not have this problem today:
`ArisoBackend.finalizeRecording` (`src/composables/useBackend.ts`) only
buffers the audio locally and uploads it; transcript and notes generation
run server-side, fully decoupled from the upload call. The wait the pill
shows for cloud is upload time over the network — a real, already
network-bound, already-labeled "Uploading…" state, not transcript
processing. This is out of scope here (see Decisions).

## 2. Goal / non-goals

**Goal:** `local_finalize_recording` returns as soon as the recording's
audio and its initial `Transcribing` status are durably persisted, without
waiting for on-device transcription (or the already-detached notes) to
finish. The recorder pill then reaches its settled "success" state quickly
and closes, which lets `open_waveform_window`'s existing yield handshake let
the next recording in — with no changes needed to
`recording_state.rs`/`commands.rs`. Transcription (and notes after it)
continues in the background, writing its outcome to `meta.json` exactly as
notes generation already does; the existing poller
(`useLocalRecordingProgress.ts`, already has no fixed timeout and polls any
`IN_FLIGHT_STAGES` indefinitely) and the existing Library/detail status UI
(`docs/superpowers/specs/2026-08-13-post-meeting-processing-status-design.md`)
already render this `Transcribing` state correctly — today it's just rarely
observed because transcription used to finish before the window
re-rendered.

**Non-goals**
- The cloud/Ariso backend (see Decision 1).
- Changing `recording_state.rs`'s window-claim/yield mechanism — it already
  does the right thing once the finalize call resolves quickly.
- Making the 5-minute local auto-append window robust against a target
  that's still transcribing in the background (see Decision 3) — documented
  trade-off, not solved.
- A global cap on how many `ariso-stt` sidecar processes may run
  concurrently. Nothing in the current code enforces single-instance
  sidecar execution today (a checkpoint and a finalize for two different
  recordings can already overlap); this change increases how often two
  finalize-driven transcriptions overlap, but introduces no new correctness
  risk — only a possible, unmeasured, resource-contention slowdown. Flagged
  as a follow-up if it proves to matter, not solved here.
- `retry_transcription_core` / `retry_local_notes` — already async-surfaced
  via the detail panel's chip+Retry, unrelated to the recorder pill.

## 3. Decisions

(Clarify step — no trusted maintainer comments existed to override these;
all are defaults, recorded per the autopilot process.)

1. **Scope: local-only.** (default) The issue's "processing the previous
   transcript" describes the local on-device pipeline. Cloud's bottleneck is
   upload bandwidth, already async from transcript/notes, already shown as
   a distinct state. Out of scope.
2. **Local STT failure surfaces asynchronously, not via the pill.** (default)
   Once transcription is backgrounded, the recorder pill can no longer learn
   whether it will fail before closing. A failure is written to `meta.json`
   (`RecordingStatus::Failed`/`PendingModels`, `error`) exactly as it is
   today, and surfaces via the existing detail-panel "Transcript failed"
   chip + Retry (`retry_transcription_core`) — the same mechanism a notes
   failure already uses. The pill's inline Retry/Discard UI
   (`uploadResult === 'failed'` in `WaveformView.vue`) remains for failures
   that are still genuinely synchronous: vault/audio write errors, directory
   creation errors, lock issues — i.e., anything that risks losing the
   audio itself.
3. **Accept a wider (but still rare, still safe) auto-append miss window.**
   (default) `storage::most_recent_appendable` requires the newest recording
   to be `Done`. Today that's nearly always true by the time a next session
   starts, because transcription finished synchronously before the pill
   closed. After this change, a genuinely-intended "resume this meeting"
   within 5 minutes can land as a new, separate recording instead of an
   append if the prior clip is still transcribing in the background. The
   existing design already calls this "a rare race; safe and simple" — nothing
   is corrupted or lost, two recordings exist instead of one merged one.
   This change makes that race somewhat less rare; still not solved here.
4. **Apply the same treatment to `append_recording_core`.** (default) For a
   uniform contract — `FinalizeResult.status` is always `Transcribing`
   immediately after a successful local finalize, append or fresh — detach
   the append path's own (clip-sized, normally short) transcription too,
   rather than special-casing it as "small enough to stay synchronous."
5. **Test depth.** (default) Update every existing Rust test whose
   assertions assume synchronous completion, and add new tests (using the
   existing STT-stub gating pattern — `StubBehavior::...gating_transcribe()`,
   `SttGateGuard`, `wait_for_stt_start`, already used by
   `the_stamped_model_is_the_one_that_transcribed`) proving finalize returns
   before transcription completes, and that a second, unrelated recording's
   finalize is not blocked by a first one's in-flight background work. Run
   `cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1`
   (with the `DYLD_LIBRARY_PATH` workaround) for this change in addition to
   the three-command verification contract, given the depth of the refactor.

## 4. Design

### 4.1 Current shape (for reference)

```
fresh_recording_core / append_recording_core (one async fn, fully awaited):
  1. (fast) resolve id/dir, acquire per-recording lock, resolve title/audio name
  2. (fast) persist audio to the vault, write meta.json (status: Transcribing)
  3. (SLOW) run_transcribe(...)                      <-- blocks the caller
  4. (fast) write segments.json, transcript.md, meta.json (status: Done)
  5. spawn detached process_notes(...), return (FinalizeResult{Done}, notes_handle)
```

### 4.2 New shape

Split each function at the point `run_transcribe` is first needed. Steps 1–2
(now ending in "status: Transcribing") stay synchronous and are awaited by
the Tauri command as today. Steps 3–5 move into a function spawned via
`tokio::spawn`, which takes ownership of the already-held per-recording lock
guard (`tokio::sync::OwnedMutexGuard`) so it continues to serialize against
concurrent checkpoints/appends/retries on the *same* recording id exactly as
today — nothing about the locking *invariant* changes, only when the
synchronous caller stops waiting on it.

```
fresh_recording_core (fast, returns once audio + initial meta are durable):
  1. (fast) resolve id/dir, acquire per-recording lock, resolve title/audio name
  2. (fast) persist audio to the vault, write meta.json (status: Transcribing)
  3. spawn finish_fresh_recording(dir, id, title, meta, recording_guard)
  4. return (FinalizeResult{ status: Transcribing, id, title }, <handle to 3>)

finish_fresh_recording (background; holds the moved lock guard until it returns):
  a. run_transcribe(...)
  b. on success: write segments.json, transcript.md, meta.json (status: Done,
     notes_in_progress: true); await process_notes(...) directly (no inner
     spawn — this whole function is already detached from the caller)
  c. on failure: classify PendingModels vs Failed exactly as today, write
     meta.json, return (no caller left to propagate Err to — same pattern
     notes failures already use)
```

`append_recording_core` gets the same split:
- **Stays synchronous:** acquire the target's per-recording lock, re-validate
  the target under the lock (fallback to `fresh_recording_core` if it's no
  longer a valid append target — this call is itself now fast, so the
  fallback stays fast too, no special-casing needed), write the new clip's
  raw bytes to its temp file in the recording dir (`append-clip.mp3`) — this
  is the durability point: the clip can never be lost from here on.
- **Moves to the spawned `finish_append_recording`:** transcribing the clip,
  the "committed to merging" flip to `Transcribing` + content writes (same
  order as today — see the existing doc comment on crash safety, preserved
  verbatim, just relocated), the final `Done` write, and the notes chain. On
  a clip transcription failure: `save_failed_clip` (unchanged, target
  untouched) — logged, not propagated (no caller left to return `Err` to).

The returned `FinalizeResult.title` for an append is the **target's**
existing title (read under the lock before spawning), unchanged from today
— a resumed session never renames the meeting it's extending.

### 4.3 What does NOT change

- `src-tauri/src/recording_state.rs`, `src-tauri/src/commands.rs`
  (`open_waveform_window`, the yield handshake, `YIELD_TIMEOUT`): the
  window-claim mechanism already does the right thing as soon as
  `finalizeRecording` resolves quickly — no code there references
  transcription completion.
- `src/views/WaveformView.vue`, `src/composables/useBackend.ts`'s
  `FinalizeResult` *shape* (no new fields) — only how fast the promise
  resolves, and that `status` is `Transcribing` instead of `Done` on the
  success path.
- `src/composables/useLocalRecordingProgress.ts`,
  `MeetingDetailView.vue`'s status chip, `LibraryView.vue`'s
  `rowProcessingLabel` — all already read live `meta.json` state via
  `local.recordingStatus`/`listRecordings` and already handle
  `'transcribing'` as an in-flight, indefinitely-polled stage. This is
  exactly the existing machinery from the Aug 13 post-meeting-processing-status
  work; this change makes it get exercised far more often (a `Transcribing`
  window of real duration instead of one that almost always already closed),
  not a new code path.
- `checkpoint_core`/`checkpoint_locked`: already treats any non-`Recording`
  status as stale/terminal, unaffected either way by when the transition to
  `Transcribing`/`Done` happens.
- `local_finalize_recording`, `finalize_core`, `finalize_core_with_target`
  Tauri command/function **signatures** — same `Result<(FinalizeResult,
  JoinHandle<()>), String>` shape; the command already drops the handle.

## 5. Error handling / edge cases

| Situation | Behavior |
| --- | --- |
| Vault/audio write fails, or dir creation fails | Unchanged: synchronous `Err` from `local_finalize_recording`, pill shows failed/Retry — these are audio-loss risks, stay on the fast path. |
| On-device transcription fails (model not ready) | `meta.json` → `PendingModels` (unchanged content), written from the background task; no synchronous signal. Surfaces via the existing `useLocalRecordingProgress` poll → `pending-models` stage (resolves on its own once the model finishes downloading, exactly as today). |
| On-device transcription fails (genuine STT error) | `meta.json` → `Failed`, `error` set, audio retained (unchanged). Surfaces via the existing detail-panel "Transcript failed" chip + Retry. The *pill* no longer shows this — see Decision 2. |
| Append target no longer valid by the time the lock is acquired (a concurrent append landed first) | Unchanged fallback: release the guard, call `fresh_recording_core` — now itself fast, so the fallback is fast too. |
| Two finalize calls for the *same* append target race | Unchanged serialization: the second blocks on the per-recording lock (now held for the first's whole background duration, not just its old synchronous duration) and re-validates after acquiring it. |
| A resume lands inside the 5-minute window while the prior clip is still `Transcribing` in the background | `most_recent_appendable` requires `Done` → falls back to a new, separate recording. See Decision 3: accepted, pre-existing documented trade-off, now less rare. |
| Two *different* meetings' background transcriptions overlap (the actual fix in action) | Each runs under its own per-recording lock/dir; no shared state. Possible sidecar resource contention, no correctness issue. See Non-goals. |

## 6. Testing

**Rust (`src-tauri/src/transcribe.rs`)** — run with
`DYLD_LIBRARY_PATH=".../swift-5.5/macosx" cargo test --manifest-path
src-tauri/Cargo.toml -- --test-threads=1`:

- Update every existing test currently asserting `res.status ==
  RecordingStatus::Done` (or expecting a synchronous `Err`) immediately
  after `finalize_core`/`finalize_core_with_target`/append helpers return:
  assert `Transcribing` (or `Ok` with `Transcribing`) at that point, `await`
  the returned handle, then re-read `meta.json` from disk for the terminal
  assertion — matching the pattern most of these tests already use for the
  separately-tracked notes outcome.
- New: a fresh-recording finalize call returns (`Transcribing`) before a
  gated STT stub is released (using the existing `SttGateGuard`/
  `wait_for_stt_start` pattern).
- New: same, for the append path (a gated clip transcription).
- New: starting a finalize for a **second, unrelated** recording while a
  first one's background transcription is still gated — the second
  succeeds immediately and independently (the regression test for the
  actual bug: two different recordings' per-recording locks don't
  interfere).
- New: an STT failure (model not ready, and a genuine STT error) written to
  `meta.json` by the background task with no synchronous `Err` from the
  outer call.
- Existing append/concurrency tests
  (`second_recording_within_window_appends_to_first`, the append-race tests
  near the existing "concurrent append" coverage) keep passing once updated
  to await the handle before their file-system assertions — already their
  existing pattern.

**Frontend:** no behavior to add tests for — `useLocalRecordingProgress`,
`LibraryView`, `MeetingDetailView` are unchanged and already covered for the
`'transcribing'` stage.

**Manual / what cannot be verified in this run:** this needs the running
app with a real multi-minute local recording to observe the pill actually
closing within seconds of Stop while the Library row keeps showing
"Processing…" — not verifiable without a live device/mic session. Flagged
in the PR body as untested beyond the Rust test suite.

## 7. Acceptance criteria (from the issue, carried forward)

- [ ] A meeting's transcription and notes processing can continue after the
  user navigates away from it. (Notes already did; transcription now does
  too.)
- [ ] Users can quickly switch to or start the next meeting while earlier
  processing is in progress. (The fix: `local_finalize_recording` no longer
  blocks on transcription.)
- [ ] Starting the next meeting does not interrupt, overwrite, or
  misassociate the previous meeting's recording or processing results.
  (Per-recording lock, now held across the background task, preserves this.)
- [ ] Each meeting clearly shows its own recording and processing status.
  (Already true via the existing status chip/row machinery — now actually
  exercised for transcription, not just notes.)
- [ ] If there is a genuine limitation preventing a new recording, show a
  clear, actionable explanation rather than silently blocking
  initialization. (Unchanged: the existing `recording://start-failed`
  yield-expiry payload, `#320`, already covers genuinely-busy cases —
  still-capturing, still-uploading/persisting.)
