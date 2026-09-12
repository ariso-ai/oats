<template>
  <!-- Headless recorder host. The floating pill is drawn natively (Swift on
       macOS, Win32 on Windows; see src-tauri/src/recorder_pill) from the
       recorder://state this view broadcasts. The window paints nothing and
       ignores the cursor. -->
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from 'vue';
import { useRoute } from 'vue-router';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { useRecorder } from '../composables/useRecorder';
import { recordingStartErrorMessage } from '../composables/recordingStartError';
import {
  getActiveBackend,
  timestampTitle,
  type Backend,
  type FinalizeResult,
  type RecordingMeta,
} from '../composables/useBackend';
import { pending, local } from '../tauri';
import { loadRecordingEnabled } from '../composables/useRecordingPermissions';
import { isSilenceDetectionEnabled } from '../composables/useSilenceDetection';
import { isMeetingEndReminderEnabled } from '../composables/useMeetingEndReminder';
import { deriveRecordingMode } from './recordingSettings';
import { centerWeightedBars } from './waveformBars';
import { shouldPromptSilence, shouldAutoStopAfterPrompt } from '../composables/silenceWatch';
import {
  shouldPromptMeetingEnd,
  findMeetingEndAt,
  findNextMeetingStart,
  MEETING_END_GRACE_MS,
  MEETING_END_PROMPT_TIMEOUT_MS,
} from '../composables/meetingEndWatch';
import { localRecordingIdFromStart } from '../composables/localRecordingId';
import { resolveAssociation } from '../composables/useAutoTrigger';
import { useMeetingApi } from '../composables/useMeetingApi';

const SUCCESS_CLOSE_MS = 1500;

const recorder = useRecorder();
const backend = ref<Backend | null>(null);
const isUploading = ref(false);
const uploadResult = ref<'success' | 'failed' | null>(null);

// Held after stop so a failed upload can be retried without re-recording.
// Cleared on success/dismiss; the meta also keys the on-disk pending buffer.
const stoppedBlob = ref<Blob | null>(null);
const stoppedMeta = ref<RecordingMeta | null>(null);

const route = useRoute();
const meetingIdQuery = route.query.meetingId;
const effectiveMeetingId = ref<number | null>(
  typeof meetingIdQuery === 'string' && /^\d+$/.test(meetingIdQuery)
    ? Number(meetingIdQuery)
    : null,
);
const localAppendIdRaw = route.query.localAppendId;
// When the user chose "Continue this meeting", the Library passes the target
// local recording id here so finalize appends to it regardless of elapsed time.
const localAppendId =
  typeof localAppendIdRaw === 'string' && localAppendIdRaw.length > 0 ? localAppendIdRaw : null;
// When the detail pane was empty at Start, the Library passes this so finalize
// forces a brand-new recording and skips the 5-minute auto-append.
const forceNew = route.query.forceNew === '1';
const isAuto = route.query.auto === '1';
const isStopping = ref(false);
// Auto recordings shorter than this are discarded, not uploaded (guards against
// late mic-on / quick-off races). Manual recordings are never length-gated.
const MIN_AUTO_DURATION_S = 15;

// Mirror the recording to the library window's embedded recorder strip and the
// native floating pill. The bars ride on frameLevels, sampled in the audio
// callback, so the cadence survives this window being hidden.
type RecorderPhase = 'starting' | 'recording' | 'uploading' | 'success' | 'failed' | 'closed';

function currentPhase(): RecorderPhase {
  if (uploadResult.value) return uploadResult.value;
  if (isUploading.value) return 'uploading';
  return recorder.isRecording.value ? 'recording' : 'starting';
}

// Once `closed` is sent, stay silent: a heartbeat firing between the closed
// broadcast and the window's destruction would otherwise revive the strip.
let closedSent = false;

// The local recording id the current session will finalize into — the append
// target when resuming the recent recording, or this session's own new id. Rust
// owns the append decision (5-min window), so we resolve it once when recording
// starts (the startedAt watcher) rather than deriving a new id from the start
// time here. Null until resolved.
const effectiveLocalRecordingId = ref<string | null>(null);

function broadcastState(phase: RecorderPhase = currentPhase()): void {
  if (closedSent) return;
  // Don't announce "recording" while startRecording() is still awaiting
  // getUserMedia/setup — the strip would render a phantom active recorder.
  if (phase === 'recording' && !recorder.isRecording.value) return;
  if (phase === 'closed') closedSent = true;
  emit('recorder://state', {
    bars: centerWeightedBars(recorder.frameLevels.value.slice(0, 20), 3),
    durationSeconds: recorder.durationSeconds.value,
    isPaused: recorder.isPaused.value,
    meetingId: effectiveMeetingId.value,
    // Local recordings have no meeting id. Broadcast the id the recording will
    // finalize INTO — the append target when resuming the recent recording, else
    // the new recording's own id — so the library pins the strip to the current
    // meeting on a resume instead of spinning up a phantom new note. Null until
    // resolved (see the startedAt watcher), which keeps the strip home-less for a
    // beat rather than briefly selecting the wrong (new) row.
    localRecordingId:
      backend.value?.id === 'local' ? effectiveLocalRecordingId.value : null,
    phase,
  }).catch(() => { /* no listeners / shutting down */ });
}

watch(() => recorder.frameLevels.value, () => broadcastState());
watch(
  [() => recorder.durationSeconds.value, () => recorder.isPaused.value, isUploading, uploadResult],
  () => broadcastState(),
);
// Re-resolve the scheduled end whenever the attached meeting changes.
watch(effectiveMeetingId, () => void resolveMeetingEnd());
// Tell the native side what is being recorded, so the tray's recording menu can
// name it and open it (#355). Fires whenever an identity becomes known: an
// Ariso meeting (calendar-matched, picker-selected, or the ad-hoc one created
// for an unmatched auto-trigger) or a local recording's resolved id. Local
// passes no meeting id — it has no server-side meeting — and the tray routes
// its click through the same `recording://reveal` broadcast either way.
let trayIdentityToken = 0;
async function registerTrayIdentity(): Promise<void> {
  const token = ++trayIdentityToken;
  const meetingId = effectiveMeetingId.value;
  const localId = effectiveLocalRecordingId.value;
  if (meetingId === null && localId === null) return;
  // Wait for the backend to resolve rather than registering an unnamed row we
  // would immediately have to correct; the watcher re-runs once it is known.
  const backendId = backend.value?.id;
  if (!backendId) return;

  let title: string | null = null;
  if (meetingId !== null) {
    // Ariso only. A local recording must never reach out to the network, and
    // that guarantee shouldn't rest on "a local recording can't carry a meeting
    // id anyway" — resolveSilenceSubtitle and resolveMeetingEnd guard the same
    // way. Without a title the row still exists and is still clickable.
    if (backendId === 'ariso') {
      try {
        const { meeting } = await useMeetingApi().getMeeting(meetingId);
        title = meeting.title ?? null;
      } catch (e) {
        // The row still needs to exist and be clickable; only its label suffers.
        console.error('Failed to resolve the recording title for the tray', e);
      }
    }
  } else if (recorder.startedAt.value) {
    title = timestampTitle(recorder.startedAt.value);
  }
  if (token !== trayIdentityToken) return;
  try {
    await invoke('set_recording_meeting', { meetingId, title });
  } catch (e) {
    console.error('Failed to register the recording with the tray', e);
  }
}
watch(
  // The backend is a source too: it resolves asynchronously, and an Ariso
  // meeting id can already be known (route query) before it does.
  [effectiveMeetingId, effectiveLocalRecordingId, () => backend.value?.id],
  () => void registerTrayIdentity(),
  { immediate: true },
);
// The resolved local recording id whose stub `meta.json` has not been written
// yet. Null when there is nothing to write: an explicit append target, a failed
// resolve, a non-local backend, or a stub already written.
let pendingStubId: string | null = null;

// Write the stub `meta.json` that makes a capturing local recording a real,
// renamable row. Pass the exact title finalize will use so the label never
// changes under the user. At most one write per session, success or not.
async function writeLocalRecordingStub(): Promise<void> {
  const id = pendingStubId;
  const startAt = recorder.startedAt.value;
  if (!id || !startAt) return;
  pendingStubId = null;
  try {
    await local.beginRecording(id, startAt, timestampTitle(startAt));
  } catch (e) {
    // Cosmetic: without it the rename affordance stays broken until Stop
    // (today's behavior). Never worth aborting a live recording over.
    console.error('Failed to create the local recording on disk', e);
  }
}

// When a local recording starts, ask Rust which recording it will finalize into
// (append target vs. new) and broadcast that id. Resolving once here — rather
// than deriving a fresh id from the start time on every heartbeat — is what
// keeps a resume docked to the current meeting instead of a phantom new note.
// Depends on the backend too: `startedAt` is set once per session, but the
// backend resolves asynchronously, so re-run once we know it's local.
let idResolveToken = 0;
watch(
  [() => recorder.startedAt.value, () => backend.value?.id],
  async ([startAt, backendId]) => {
    const token = ++idResolveToken;
    if (!startAt || backendId !== 'local') {
      effectiveLocalRecordingId.value = null;
      pendingStubId = null;
      return;
    }
    if (localAppendId) {
      // Explicit continue: skip the time-window resolve and dock to the target.
      effectiveLocalRecordingId.value = localAppendId;
      return;
    }
    try {
      // forceNew makes the backend return this session's own new id (never an
      // append target), so the recorder docks to a fresh row.
      const id = await local.recordingIdForStart(startAt, forceNew);
      if (token !== idResolveToken) return;
      effectiveLocalRecordingId.value = id;
      // Give the recording an on-disk identity so its row is real and renamable
      // while it captures (#355), not only after Stop. A manual recording gets
      // it right away; an auto one waits until it outlives the discard window
      // (see the duration watcher), because handleStop discards a sub-15s blip
      // without ever finalizing and there is no delete for local recordings —
      // the stub would be a permanent phantom row.
      pendingStubId = id;
      if (!isAuto) await writeLocalRecordingStub();
    } catch (e) {
      // Fall back to this session's own id so recording still works if the
      // resolve fails (worst case: today's behavior, a new-recording row).
      console.error('Failed to resolve local recording id; using new-recording id', e);
      if (token === idResolveToken) effectiveLocalRecordingId.value = localRecordingIdFromStart(startAt);
    }
  },
  { immediate: true },
);
// Heartbeat so the strip can detect a dead recorder (no events ≈ crashed):
// frame/duration watchers go quiet during upload and after stop.
const stateHeartbeat = setInterval(() => broadcastState(), 1_000);
onUnmounted(() => clearInterval(stateHeartbeat));

let unlistenPendingUploaded: UnlistenFn | null = null;
let unlistenYield: UnlistenFn | null = null;
let unlistenRetryUpload: UnlistenFn | null = null;
let unlistenContinueRecording: UnlistenFn | null = null;
let unlistenDiscardRecording: UnlistenFn | null = null;
let unlistenPause: UnlistenFn | null = null;
let unlistenResume: UnlistenFn | null = null;
let unlistenStop: UnlistenFn | null = null;
let unlistenAutoStop: UnlistenFn | null = null;
let unlistenSilenceKeep: UnlistenFn | null = null;
let unlistenSilenceStop: UnlistenFn | null = null;
// Wall-clock ms when the silence prompt was shown, or null when idle.
let promptShownAt: number | null = null;
let closeTimer: ReturnType<typeof setTimeout> | null = null;
let silenceTimer: ReturnType<typeof setInterval> | null = null;

let unlistenMeetingEndKeep: UnlistenFn | null = null;
let unlistenMeetingEndStop: UnlistenFn | null = null;
let meetingEndTimer: ReturnType<typeof setInterval> | null = null;
// Scheduled end of the attached meeting (epoch ms), or null when the watch is
// disabled (local / unattached / non-Ariso / no end_at). Meeting title for the
// card subtitle.
const meetingEndAt = ref<number | null>(null);
const meetingEndSubtitle = ref<string | undefined>(undefined);
// Scheduled start of the NEXT calendar meeting (epoch ms), or null when there
// is none. The next meeting's start is the transition point: it triggers the
// prompt immediately, so back-to-back (or slightly overlapping) meetings don't
// bleed through the end+grace wait.
const meetingNextStartAt = ref<number | null>(null);
let meetingEndPromptShownAt: number | null = null;
let meetingEndPromptsShown = 0;
let meetingEndLastPromptAt: number | null = null;
// Meeting-stop reminder gate (default on). Read once on mount like silence
// detection; toggling mid-recording only affects the next recording. When off,
// we skip both the timer and the scheduled-meetings lookup it depends on.
let meetingEndReminderEnabled = true;

// Reset the tray to idle and close the recording window. Best-effort: a
// failure of either step must not throw out of the abort/rollback path.
async function rollbackAndClose() {
  broadcastState('closed');
  try {
    await invoke('set_tray_recording', { isRecording: false, isPaused: false });
  } catch { /* ignore */ }
  try {
    await getCurrentWebviewWindow().close();
  } catch { /* ignore */ }
}

async function startRecording() {
  // Surface initialization before any settings, permission, or device work so
  // every launcher has an immediate and honest state to render.
  broadcastState('starting');
  let mode: ReturnType<typeof deriveRecordingMode>;
  try {
    mode = deriveRecordingMode(await loadRecordingEnabled());
  } catch (error) {
    await emit('recording://start-failed', {
      message: recordingStartErrorMessage(error),
    }).catch(() => {});
    await rollbackAndClose();
    return;
  }
  if (mode === null) {
    await emit('recording://start-failed', {
      message: 'No recording source is enabled. Enable Microphone or System Audio in Settings, then try again.',
    }).catch(() => {});
    await rollbackAndClose();
    return;
  }

  // Webviews don't reliably resolve getUserMedia for a window that isn't on
  // screen, and this window is hidden once capture starts — including across a
  // failed-upload → Resume, where it stays hidden from the prior stop. Show it
  // (empty and click-through) before capture so getUserMedia can resolve; the
  // native pill watcher re-hides it once capture is active.
  try {
    await getCurrentWebviewWindow().show();
  } catch {
    /* best-effort: a closed/denied window just proceeds */
  }

  try {
    await recorder.startRecording(mode);
  } catch (error) {
    await emit('recording://start-failed', {
      message: recordingStartErrorMessage(error),
    }).catch(() => {});
    await rollbackAndClose();
    return;
  }
  await invoke('set_tray_recording', { isRecording: true, isPaused: false });
}

// True once an Ariso auto-trigger has run and found no calendar meeting to
// attach to. Such a session has no identity at all today, so nothing can
// surface it while it runs; `createAdHocMeeting` gives it one.
const needsAdHocMeeting = ref(false);
let adHocMeetingRequested = false;

// Auto-trigger: attach to a matching calendar meeting when one is found. The
// user has already opted in via the pre-recording notification prompt (or
// auto-record is on), so there's no in-pill confirmation — a no-match recording
// gets its own ad-hoc meeting instead (see the duration watcher below).
async function resolveAuto() {
  try {
    if (backend.value?.id === 'ariso') {
      const now = new Date();
      const start = new Date(now.getTime() - 2 * 60 * 60 * 1000);
      const end = new Date(now.getTime() + 2 * 60 * 60 * 1000);
      const meetings = await useMeetingApi().listScheduledMeetings(start, end);
      const assoc = resolveAssociation('ariso', meetings, now);
      if (assoc.kind === 'matched') {
        effectiveMeetingId.value = assoc.meetingId ?? null;
      } else {
        // No calendar match. Don't create the meeting yet: this could still be
        // a two-second mic blip that handleStop discards.
        needsAdHocMeeting.value = true;
      }
    }
  } catch (e) {
    console.error('Auto-trigger calendar match failed; recording unattached', e);
  }
}

// Once an unmatched auto recording outlives the discard threshold it is a real
// meeting, so give it a real one — the same ad-hoc endpoint "Record a new
// meeting" uses. From here it is indistinguishable, to every list/pill/tray
// surface, from a manually-started ad-hoc recording.
//
// Deferring to this point sidesteps a hard problem for free: there is no
// delete-meeting endpoint, so a meeting created for a session that then gets
// discarded would be permanent garbage.
async function createAdHocMeeting(): Promise<void> {
  if (adHocMeetingRequested) return;
  adHocMeetingRequested = true; // one attempt per session, never a mid-session retry
  try {
    const { meetingId } = await useMeetingApi().createAudioMeeting();
    effectiveMeetingId.value = meetingId;
  } catch (e) {
    // Leave the session unattached for its remaining duration — identical to
    // today's behavior, not a regression. Finalize still creates a meeting
    // server-side when the upload arrives with no id attached.
    console.error('Failed to create an ad-hoc meeting for this recording', e);
  }
}

// An auto recording only earns a persistent identity once it outlives the
// discard threshold — before that, handleStop throws the capture away and
// neither backend can clean up after itself (no delete-meeting endpoint on
// Ariso, no delete at all for local recordings). Both deferrals land here.
watch(
  () => recorder.durationSeconds.value,
  (seconds) => {
    if (isStopping.value) return;
    if (seconds < MIN_AUTO_DURATION_S) return;
    if (needsAdHocMeeting.value) {
      needsAdHocMeeting.value = false;
      void createAdHocMeeting();
    }
    if (isAuto && pendingStubId) void writeLocalRecordingStub();
  },
);

// Discard the in-progress capture without uploading, then close.
async function discardRecording() {
  if (isStopping.value) return;
  isStopping.value = true;
  if (closeTimer) {
    clearTimeout(closeTimer);
    closeTimer = null;
  }
  if (silenceTimer) {
    clearInterval(silenceTimer);
    silenceTimer = null;
  }
  if (promptShownAt !== null) {
    promptShownAt = null;
    void invoke('dismiss_silence_prompt');
  }
  if (meetingEndTimer) {
    clearInterval(meetingEndTimer);
    meetingEndTimer = null;
  }
  if (meetingEndPromptShownAt !== null) {
    meetingEndPromptShownAt = null;
    void invoke('dismiss_meeting_end_prompt');
  }
  try {
    await recorder.stopRecording();
  } catch {
    /* best-effort */
  }
  await invoke('set_tray_recording', { isRecording: false, isPaused: false });
  await closeWindow();
}

async function handleStop() {
  if (isStopping.value) return;
  // Auto recordings that stop almost immediately (late mic-on / quick-off
  // races) are discarded rather than uploaded as a stub. Manual recordings are
  // never length-gated.
  if (isAuto && recorder.durationSeconds.value < MIN_AUTO_DURATION_S) {
    await discardRecording();
    return;
  }
  isStopping.value = true;
  // Tear down the backstop timer so it can't fire post-stop.
  if (silenceTimer) {
    clearInterval(silenceTimer);
    silenceTimer = null;
  }
  if (promptShownAt !== null) {
    promptShownAt = null;
    void invoke('dismiss_silence_prompt');
  }
  if (meetingEndTimer) {
    clearInterval(meetingEndTimer);
    meetingEndTimer = null;
  }
  if (meetingEndPromptShownAt !== null) {
    meetingEndPromptShownAt = null;
    void invoke('dismiss_meeting_end_prompt');
  }
  isUploading.value = true;
  const endAt = new Date().toISOString();
  const startAt = recorder.startedAt.value;
  const newBlob = await recorder.stopRecording();
  await invoke('set_tray_recording', { isRecording: false, isPaused: false });

  // A held blob means we're resuming a failed recording: concatenate the new
  // segment onto it and upload the whole thing as one recording. Keep the
  // ORIGINAL startAt so finalize re-keys the same on-disk buffer / Library row.
  const prevBlob = stoppedBlob.value;
  const prevMeta = stoppedMeta.value;
  const combinedBlob = prevBlob
    ? new Blob([prevBlob, newBlob], { type: 'audio/mpeg' })
    : newBlob;

  if (combinedBlob.size > 0 && backend.value) {
    stoppedBlob.value = combinedBlob;
    stoppedMeta.value = {
      startAt: prevMeta?.startAt ?? startAt,
      endAt,
      durationSeconds:
        (prevMeta?.durationSeconds ?? 0) + recorder.durationSeconds.value,
      meetingId: prevMeta?.meetingId ?? effectiveMeetingId.value ?? undefined,
      localAppendId: prevMeta?.localAppendId ?? localAppendId ?? undefined,
      forceNew: prevMeta?.forceNew ?? forceNew ?? undefined,
    };
    await runFinalize();
  } else {
    if (combinedBlob.size > 0 && !backend.value) {
      console.error('handleStop: backend not initialized; discarding recording');
    }
    await closeWindow();
  }
}

// Best-effort meeting title for the silence prompt's subtitle. Ariso recordings
// attached to a meeting show its title; local/unattached recordings show none
// (the prompt window hides the subtitle line when it's absent).
async function resolveSilenceSubtitle(): Promise<string | undefined> {
  if (backend.value?.id !== 'ariso' || effectiveMeetingId.value === null) return undefined;
  try {
    const { meeting } = await useMeetingApi().getMeeting(effectiveMeetingId.value);
    return meeting.title ?? undefined;
  } catch {
    return undefined;
  }
}

// Resolve the attached meeting's scheduled end and the next meeting's start
// (Ariso only). end_at lives on the scheduled-meetings list, NOT
// /desktop/meetings/{id}, so fetch the ±2h window and match by id. Any failure
// leaves both null → the watch stays off.
async function resolveMeetingEnd() {
  if (!meetingEndReminderEnabled || backend.value?.id !== 'ariso' || effectiveMeetingId.value === null) {
    meetingEndAt.value = null;
    meetingEndSubtitle.value = undefined;
    meetingNextStartAt.value = null;
    return;
  }
  try {
    const now = new Date();
    const start = new Date(now.getTime() - 2 * 60 * 60 * 1000);
    const end = new Date(now.getTime() + 24 * 60 * 60 * 1000);
    const meetings = await useMeetingApi().listScheduledMeetings(start, end);
    const info = findMeetingEndAt(meetings, effectiveMeetingId.value);
    meetingEndAt.value = info.endAt;
    meetingEndSubtitle.value = info.title ?? undefined;
    meetingNextStartAt.value = findNextMeetingStart(meetings, effectiveMeetingId.value).startAt;
  } catch (e) {
    console.error('Failed to resolve meeting end; meeting-end watch disabled', e);
    meetingEndAt.value = null;
    meetingNextStartAt.value = null;
  }
}

// User chose "Keep recording": reset the silence clock (same mechanism as
// resume) so the prompt naturally re-fires after another 10 min of silence.
// The tap auto-dismisses the notification, so no explicit dismiss is needed.
function handleSilenceKeep() {
  recorder.lastSoundAt.value = Date.now();
  promptShownAt = null;
}

// User chose "Stop now": stop immediately.
async function handleSilenceStop() {
  promptShownAt = null;
  await handleStop();
}

// User chose "Keep recording" (or ignored/timed-out): return to idle. The watch
// re-prompts once more after MEETING_END_REPROMPT_MS, then stops asking.
function handleMeetingEndKeep() {
  meetingEndPromptShownAt = null;
}

// User chose "Stop": stop this recording, then re-arm the mic monitor so a
// back-to-back next call records as a fresh, separately-attached session.
async function handleMeetingEndStop() {
  meetingEndPromptShownAt = null;
  // Start stop (which includes upload/finalize) but re-arm the mic monitor
  // immediately — before waiting for the upload — so a back-to-back call can
  // be detected without delay. request_mic_monitor_rearm is a simple atomic
  // flag store in Rust and is safe to call before finalize completes.
  const stopTask = handleStop();
  try {
    await invoke('request_mic_monitor_rearm');
  } catch (e) {
    console.error('Failed to re-arm mic monitor after meeting-end stop', e);
  }
  await stopTask;
}

// Upload the stopped recording. Shared by the stop flow and the failed pill's
// Retry button — blob and meta stay in refs so retry needs no re-record.
// Tracks the underlying finalize promise (not the UI-timeout race) so that a
// timed-out attempt whose work is still running won't be re-launched by Retry.
let inFlightFinalize: Promise<FinalizeResult> | null = null;
async function runFinalize() {
  if (!stoppedBlob.value || !stoppedMeta.value || !backend.value) return;
  if (inFlightFinalize) return;
  isUploading.value = true;
  uploadResult.value = null;
  // This only bounds the UI wait. A timed-out local transcription keeps
  // running natively and still writes its final status to meta.json, so the
  // Library (source of truth) may show 'done'/'failed' even if the window
  // showed a timeout. Audio is persisted before transcription/upload, so
  // nothing is lost.
  const work = backend.value.finalizeRecording(stoppedBlob.value, stoppedMeta.value);
  inFlightFinalize = work;
  // Clear the in-flight guard only when the underlying promise truly settles;
  // the UI timeout below races independently and must not release the guard.
  void work
    .catch(() => undefined)
    .finally(() => {
      if (inFlightFinalize === work) inFlightFinalize = null;
    });
  let timeoutId: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timeoutId = setTimeout(() => reject(new Error('Operation timed out')), 120_000);
  });
  try {
    const result = await Promise.race([work, timeout]);
    // An unattached Ariso upload creates its meeting during finalize. Carry
    // that server-assigned id into the success event so Library can pin and
    // reload the real meeting instead of waiting for a later focus refresh.
    if (
      result.backend === 'ariso' &&
      typeof result.meetingId === 'number' &&
      Number.isSafeInteger(result.meetingId)
    ) {
      effectiveMeetingId.value = result.meetingId;
    }
    uploadResult.value = 'success';
    stoppedBlob.value = null;
    stoppedMeta.value = null;
    // Brief confirmation, then auto-close.
    closeTimer = setTimeout(() => { closeTimer = null; void closeWindow(); }, SUCCESS_CLOSE_MS);
  } catch (err) {
    console.error('Finalize failed:', err);
    // Stay open on failure so the user can retry or dismiss.
    uploadResult.value = 'failed';
  } finally {
    clearTimeout(timeoutId);
    isUploading.value = false;
  }
}

// Explicit discard of a failed upload: delete the on-disk buffer and close.
async function dismissFailed() {
  const meta = stoppedMeta.value;
  stoppedBlob.value = null;
  stoppedMeta.value = null;
  if (meta) {
    try {
      await pending.discardAudio(meta.startAt ?? meta.endAt);
    } catch (e) {
      console.error('Failed to discard buffered audio', e);
    }
  }
  await closeWindow();
}

// The sidebar "Pending uploads" retry uploaded (and discarded) this recording's
// on-disk buffer out from under us. Our failed pill — and the library strip it
// heartbeats — is now stale, and our held blob would double-upload on Retry.
// Drop the in-memory copy without re-discarding the (already-gone) buffer, then
// close so both pills clear. Ignored unless we're actually showing the failed
// pill: a live recording or an in-flight resume must not be torn down.
async function handlePendingUploadSucceeded() {
  if (uploadResult.value !== 'failed') return;
  inFlightFinalize = null;
  stoppedBlob.value = null;
  stoppedMeta.value = null;
  await closeWindow();
}

// A new recording was requested while this window still holds the one recorder
// slot (see open_waveform_window). Stand down so it can take over — the native
// side re-opens a fresh pill once this one is destroyed.
//
// Only from a settled post-upload state. The stopped audio is already buffered
// on disk (finalizeRecording persists it before attempting the upload), so the
// Pending uploads sidebar owns the retry from here; drop the in-memory copy
// WITHOUT discarding the buffer, exactly like handlePendingUploadSucceeded.
//
// Refusing while capture or a finalize is still in flight is the point. Tearing
// the window down mid-upload can strand a buffer whose upload actually
// succeeded — discardAudio runs here, after the request resolves — and the next
// retry would upload it a second time. `inFlightFinalize` covers the finalize
// that outlived its 120s UI timeout and left a 'failed' pill up while still
// running; it clears when that work truly settles, so the block is temporary.
// A refusal leaves the pre-existing behavior: the request expires natively and
// this pill just took focus.
async function handleYield() {
  if (uploadResult.value === null || isUploading.value || inFlightFinalize) return;
  if (closeTimer) {
    clearTimeout(closeTimer);
    closeTimer = null;
  }
  stoppedBlob.value = null;
  stoppedMeta.value = null;
  await closeWindow();
}

// Keep the failed recording's audio and resume capturing into a fresh buffer.
// The next stop concatenates the held blob with the new segment (see handleStop).
async function resumeFailed() {
  if (!stoppedBlob.value) return;
  if (closeTimer) {
    clearTimeout(closeTimer);
    closeTimer = null;
  }
  // Abandon any timed-out-but-still-flying finalize so the next stop's upload
  // isn't dropped by runFinalize's in-flight guard. The held blob survives.
  inFlightFinalize = null;
  uploadResult.value = null;
  isStopping.value = false;
  await startRecording();
  broadcastState();
}

// The native pill's failed-upload controls (Retry / Continue / Discard) arrive
// as events and only act while an upload has actually failed.
function whenFailed(action: () => Promise<void>) {
  return () => {
    if (uploadResult.value !== 'failed') return;
    void action();
  };
}

async function closeWindow() {
  broadcastState('closed');
  try {
    await getCurrentWebviewWindow().close();
  } catch {
    // fallback if close permission is denied
  }
}

async function handlePause() {
  recorder.pauseRecording();
  await invoke('set_tray_recording', { isRecording: true, isPaused: true });
}

async function handleResume() {
  recorder.resumeRecording();
  await invoke('set_tray_recording', { isRecording: true, isPaused: false });
}

onMounted(async () => {
  document.documentElement.style.background = 'transparent';
  document.body.style.background = 'transparent';

  backend.value = await getActiveBackend();

  // The window stays on screen until capture starts (see startRecording) but
  // is empty: let clicks fall through to whatever is underneath.
  try {
    await getCurrentWebviewWindow().setIgnoreCursorEvents(true);
  } catch { /* permission denied / shutting down */ }

  unlistenPendingUploaded = await listen('pending-upload://succeeded', handlePendingUploadSucceeded);
  unlistenYield = await listen('recorder://yield', handleYield);
  unlistenRetryUpload = await listen('recorder://retry-upload', whenFailed(runFinalize));
  unlistenContinueRecording = await listen('recorder://continue-recording', whenFailed(resumeFailed));
  unlistenDiscardRecording = await listen('recorder://discard-recording', whenFailed(dismissFailed));

  unlistenPause = await listen('tray://pause-recording', handlePause);
  unlistenResume = await listen('tray://resume-recording', handleResume);
  unlistenStop = await listen('tray://stop-recording', handleStop);

  await startRecording();

  // Silence prompt: after 10 min of no captured sound, prompt the user (native
  // notification) to keep or stop; auto-stop after a 60s grace if ignored. Gated
  // on the user setting (default on), read once on mount — toggling it
  // mid-recording only affects the next recording. Defaults to on if the read
  // fails, so a quiet recording is still surfaced.
  let silenceDetectionEnabled = true;
  try {
    silenceDetectionEnabled = await isSilenceDetectionEnabled();
  } catch {
    /* keep the safe default (on) */
  }
  if (silenceDetectionEnabled) {
    silenceTimer = setInterval(() => {
      if (isUploading.value || uploadResult.value || !recorder.isRecording.value) return;
      const now = Date.now();
      if (promptShownAt === null) {
        if (shouldPromptSilence(recorder.lastSoundAt.value, now, recorder.isPaused.value)) {
          promptShownAt = now;
          void resolveSilenceSubtitle().then((subtitle) =>
            invoke('show_silence_prompt', subtitle ? { subtitle } : {}),
          );
        }
        return;
      }
      // Prompt is showing. Cancel it if paused or if audio resumed.
      if (recorder.isPaused.value || recorder.lastSoundAt.value > promptShownAt) {
        promptShownAt = null;
        void invoke('dismiss_silence_prompt');
        return;
      }
      if (
        shouldAutoStopAfterPrompt(
          promptShownAt,
          recorder.lastSoundAt.value,
          now,
          recorder.isPaused.value,
        )
      ) {
        promptShownAt = null;
        void invoke('dismiss_silence_prompt');
        void handleStop();
      }
    }, 1_000);
  }

  unlistenSilenceKeep = await listen('silence-prompt://keep', handleSilenceKeep);
  unlistenSilenceStop = await listen('silence-prompt://stop', handleSilenceStop);
  unlistenAutoStop = await listen('auto-record://stop', handleStop);

  // Read the meeting-stop-reminder setting once, before any lookup, so the
  // disabled path skips both the timer and the scheduled-meetings lookup it
  // depends on. Defaults to on if the read fails.
  try {
    meetingEndReminderEnabled = await isMeetingEndReminderEnabled();
  } catch {
    /* keep the safe default (on) */
  }

  // Resolve the attached meeting's end now (covers the manual param path; the
  // watcher covers the async auto path). No-op when the reminder is disabled.
  void resolveMeetingEnd();

  // Meeting-stop reminder: prompts when the attached meeting's scheduled end
  // has passed, or immediately when the next calendar meeting starts (the
  // transition point for back-to-back calls); ignoring it keeps recording.
  if (meetingEndReminderEnabled) {
    meetingEndTimer = setInterval(() => {
      if (isUploading.value || uploadResult.value || !recorder.isRecording.value) return;
      const now = Date.now();
      if (meetingEndPromptShownAt === null) {
        if (
          shouldPromptMeetingEnd(
            meetingEndAt.value,
            now,
            recorder.isPaused.value,
            meetingEndPromptsShown,
            meetingEndLastPromptAt,
            meetingNextStartAt.value,
          )
        ) {
          meetingEndPromptShownAt = now;
          meetingEndLastPromptAt = now;
          meetingEndPromptsShown += 1;
          // Title says why the card appeared; subtitle names the meeting being
          // ended. "Next meeting started" exactly when the end+grace rule alone
          // wouldn't have fired yet — i.e. the next meeting is the trigger.
          const nextStarted =
            meetingNextStartAt.value !== null &&
            now >= meetingNextStartAt.value &&
            (meetingEndAt.value === null || now < meetingEndAt.value + MEETING_END_GRACE_MS);
          void invoke('show_meeting_end_prompt', {
            ...(meetingEndSubtitle.value ? { subtitle: meetingEndSubtitle.value } : {}),
            ...(nextStarted ? { title: 'Next meeting started' } : {}),
          });
        }
        return;
      }
      // Prompt is showing: dismiss on pause or after the timeout (= keep recording).
      if (recorder.isPaused.value || now - meetingEndPromptShownAt >= MEETING_END_PROMPT_TIMEOUT_MS) {
        meetingEndPromptShownAt = null;
        void invoke('dismiss_meeting_end_prompt');
      }
    }, 1_000);
  }

  unlistenMeetingEndKeep = await listen('meeting-end-prompt://keep', handleMeetingEndKeep);
  unlistenMeetingEndStop = await listen('meeting-end-prompt://stop', handleMeetingEndStop);

  if (isAuto) {
    void resolveAuto();
  }
});

onUnmounted(() => {
  if (silenceTimer) clearInterval(silenceTimer);
  if (closeTimer) clearTimeout(closeTimer);
  unlistenPendingUploaded?.();
  unlistenYield?.();
  unlistenRetryUpload?.();
  unlistenContinueRecording?.();
  unlistenDiscardRecording?.();
  unlistenPause?.();
  unlistenResume?.();
  unlistenStop?.();
  unlistenAutoStop?.();
  unlistenSilenceKeep?.();
  unlistenSilenceStop?.();
  if (promptShownAt !== null) {
    promptShownAt = null;
    void invoke('dismiss_silence_prompt');
  }
  if (meetingEndTimer) clearInterval(meetingEndTimer);
  unlistenMeetingEndKeep?.();
  unlistenMeetingEndStop?.();
  if (meetingEndPromptShownAt !== null) {
    meetingEndPromptShownAt = null;
    void invoke('dismiss_meeting_end_prompt');
  }
});
</script>

<style>
/* Global styles for waveform window — must not be scoped */
html, body {
  background: transparent !important;
  margin: 0;
  padding: 0;
  height: 100%;
  overflow: hidden;
}
</style>
