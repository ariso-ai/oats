<template>
  <!-- The window is a fixed size (room for the expanded pill + its shadow); the
       pill is anchored to the bottom and grows UPWARD via a CSS transition. -->
  <div class="stage">
    <!-- While the meetings window owns the recorder UI (its embedded strip), the
         window stays born-visible for getUserMedia but paints nothing — no flash.
         pillHidden flips to false if the meetings window is minimized/closed. -->
    <div
      v-if="!pillHidden"
      class="pill"
      :class="{ expanded: isExpanded, paused: recorder.isPaused.value }"
      @mouseenter="expand"
      @mouseleave="collapse"
      @click="showMeetings"
    >
      <img class="logo" src="../assets/oats-tray-white.svg" alt="" />

      <template v-if="uploadResult === 'failed'">
        <span class="status-icon err">✗</span>
        <button
          class="ctrl-btn retry-btn"
          :aria-label="retryButtonLabel"
          :title="retryButtonLabel"
          @click.stop.prevent="runFinalize"
        >↻</button>
        <button
          class="ctrl-btn resume-btn"
          :aria-label="resumeButtonLabel"
          :title="resumeButtonLabel"
          @click.stop.prevent="resumeFailed"
        >●</button>
        <button
          class="ctrl-btn dismiss-btn"
          :aria-label="discardButtonLabel"
          :title="discardButtonLabel"
          @click.stop.prevent="dismissFailed"
        >✕</button>
      </template>
      <template v-else-if="uploadResult === 'success'">
        <span class="status-icon ok">✓</span>
      </template>
      <template v-else-if="isUploading">
        <span class="spinner" />
      </template>
      <template v-else>
        <div class="bars">
          <div
            v-for="(level, i) in bars"
            :key="i"
            class="bar"
            :class="{ paused: recorder.isPaused.value }"
            :style="{ height: `${Math.max(12, Math.min(100, Math.sqrt(level) * 150))}%` }"
          />
        </div>

        <!-- Always in the DOM so its reveal can animate; clipped + faded when
             collapsed. Pause sits above Stop. -->
        <div class="expanded-area" :class="{ open: isExpanded }">
          <span class="timer">{{ formattedDuration }}</span>
          <button
            class="ctrl-btn pause-btn"
            :aria-label="pauseResumeLabel"
            :title="pauseResumeLabel"
            @click.stop.prevent="recorder.isPaused.value ? handleResume() : handlePause()"
          >
            <svg v-if="!recorder.isPaused.value" width="14" height="14" viewBox="0 0 14 14">
              <rect x="2" y="1" width="3.5" height="12" rx="1" fill="currentColor" />
              <rect x="8.5" y="1" width="3.5" height="12" rx="1" fill="currentColor" />
            </svg>
            <svg v-else width="14" height="14" viewBox="0 0 14 14">
              <circle cx="7" cy="7" r="5.5" fill="currentColor" />
            </svg>
          </button>
          <button
            class="ctrl-btn stop-btn"
            :aria-label="stopButtonLabel"
            :title="stopButtonLabel"
            @click.stop.prevent="handleStop"
          >
            <svg width="14" height="14" viewBox="0 0 14 14">
              <rect x="2" y="2" width="10" height="10" rx="2" fill="currentColor" />
            </svg>
          </button>
        </div>

        <!-- Windows tracks the pointer itself because WebView2 can deliver the
             first move before Tauri's asynchronous native drag loop is ready. -->
        <div
          class="drag-handle"
          @pointerdown.left.stop.prevent="startWindowDrag"
          @click.stop
        >
          <div class="divider" />
          <div class="drag-dots">
            <span v-for="n in 6" :key="n" class="dot" />
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch, onMounted, onUnmounted } from 'vue';
import { useRoute } from 'vue-router';
import { invoke } from '@tauri-apps/api/core';
import { LogicalPosition } from '@tauri-apps/api/dpi';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { useRecorder } from '../composables/useRecorder';
import { recordingStartErrorMessage } from '../composables/recordingStartError';
import { useWaveform } from '../composables/useWaveform';
import {
  getActiveBackend,
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
const waveform = useWaveform();
const backend = ref<Backend | null>(null);
// These labels drive both native hover tooltips and accessibility names so the
// icon-only controls stay understandable in local and cloud recording flows.
const pauseResumeLabel = computed(() => (recorder.isPaused.value ? 'Resume recording' : 'Pause recording'));
const stopButtonLabel = 'Stop and save recording';
const retryButtonLabel = 'Retry upload';
const resumeButtonLabel = 'Continue recording';
const discardButtonLabel = 'Discard recording';
const isUploading = ref(false);
const uploadResult = ref<'success' | 'failed' | null>(null);
const isExpanded = ref(false);

// Held after stop so a failed upload can be retried without re-recording.
// Cleared on success/dismiss; the meta also keys the on-disk pending buffer.
const stoppedBlob = ref<Blob | null>(null);
const stoppedMeta = ref<RecordingMeta | null>(null);

// Voice energy lives in the low FFT bins; the upper bins are near-silent and
// would leave the higher bars dead. Bucket only the low part of the spectrum,
// center-weighted so the middle bar carries the hottest (lowest) bucket.
const bars = computed(() => centerWeightedBars(waveform.levels.value.slice(0, 20), 3));

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
// Born hidden when the meetings window is the visible UI: the window must still
// exist (and stay visible to WebKit so getUserMedia resolves), but the pill
// paints nothing so it never flashes over the meetings window. The Rust
// visibility watcher pushes recorder://pill-visible to flip this when the
// meetings window is minimized or closed mid-recording.
const pillHidden = ref(route.query.pillHidden === '1');
const isStopping = ref(false);
// Auto recordings shorter than this are discarded, not uploaded (guards against
// late mic-on / quick-off races). Manual recordings are never length-gated.
const MIN_AUTO_DURATION_S = 15;

const formattedDuration = computed(() => {
  const s = recorder.durationSeconds.value;
  const mins = Math.floor(s / 60).toString().padStart(2, '0');
  const secs = (s % 60).toString().padStart(2, '0');
  return `${mins}:${secs}`;
});

// Mirror the recording to the library window's embedded recorder strip. The
// bars ride on frameLevels (sampled in the audio callback, so the cadence
// survives this window being hidden, unlike rAF-driven waveform.levels).
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
      if (token === idResolveToken) effectiveLocalRecordingId.value = id;
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

function expand() {
  // Don't expand during upload/result states.
  if (isUploading.value || uploadResult.value) return;
  isExpanded.value = true;
}

function collapse() {
  isExpanded.value = false;
}

type WindowsDragState = {
  pointerId: number;
  startPointerX: number;
  startPointerY: number;
  startWindowX: number;
  startWindowY: number;
  handle: HTMLElement;
};

let windowsDrag: WindowsDragState | null = null;

function finishWindowsDrag(event?: PointerEvent) {
  if (!windowsDrag || (event && event.pointerId !== windowsDrag.pointerId)) return;

  const { handle, pointerId } = windowsDrag;
  windowsDrag = null;
  window.removeEventListener('pointermove', moveWindowsWindow, true);
  window.removeEventListener('pointerup', finishWindowsDrag, true);
  window.removeEventListener('pointercancel', finishWindowsDrag, true);
  handle.removeEventListener('lostpointercapture', finishWindowsDrag);
  if (handle.hasPointerCapture?.(pointerId)) handle.releasePointerCapture(pointerId);
}

function moveWindowsWindow(event: PointerEvent) {
  if (
    !windowsDrag
    || event.pointerId !== windowsDrag.pointerId
    || event.isPrimary === false
    || (event.buttons & 1) === 0
  ) return;

  event.preventDefault();
  const x = windowsDrag.startWindowX + event.screenX - windowsDrag.startPointerX;
  const y = windowsDrag.startWindowY + event.screenY - windowsDrag.startPointerY;
  // CSSOM screen coordinates and Tauri LogicalPosition use logical pixels, so
  // this remains correct on Windows displays with non-100% scaling.
  void getCurrentWebviewWindow().setPosition(new LogicalPosition(x, y)).catch((e) => {
    console.error('Failed to move recorder window', e);
    finishWindowsDrag(event);
  });
}

async function startWindowDrag(event: PointerEvent) {
  if (/Windows/i.test(navigator.userAgent)) {
    if (event.isPrimary === false || event.button !== 0) return;
    finishWindowsDrag();

    const handle = event.currentTarget as HTMLElement;
    windowsDrag = {
      pointerId: event.pointerId,
      startPointerX: event.screenX,
      startPointerY: event.screenY,
      startWindowX: window.screenX,
      startWindowY: window.screenY,
      handle,
    };
    window.addEventListener('pointermove', moveWindowsWindow, true);
    window.addEventListener('pointerup', finishWindowsDrag, true);
    window.addEventListener('pointercancel', finishWindowsDrag, true);
    handle.addEventListener('lostpointercapture', finishWindowsDrag);
    handle.setPointerCapture?.(event.pointerId);
    return;
  }

  try {
    await getCurrentWebviewWindow().startDragging();
  } catch (e) {
    console.error('Failed to start recorder window drag', e);
  }
}

onUnmounted(() => finishWindowsDrag());

// Clicking the pill body (not the controls or the drag handle) brings up the
// meetings window and surfaces the meeting being recorded. A window that was
// only minimized is already running, so it needs the explicit `reveal` nudge;
// a freshly-created one instead lands on the recording via its own
// recordingMeetingId watch once the recorder strip's first heartbeat arrives.
async function showMeetings() {
  try {
    await invoke('create_library_window');
    await emit('recording://reveal');
  } catch (e) {
    console.error('Failed to open meetings window', e);
  }
}

// Keep the empty transparent window from intercepting clicks meant for the
// window underneath it while the pill isn't painted.
async function applyPillVisibility(hidden: boolean) {
  pillHidden.value = hidden;
  try {
    await getCurrentWebviewWindow().setIgnoreCursorEvents(hidden);
  } catch { /* permission denied / shutting down */ }
}

let unlistenPillVisible: UnlistenFn | null = null;
let unlistenPendingUploaded: UnlistenFn | null = null;
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

  // WebKit never resolves getUserMedia for a window that isn't actually on
  // screen. The pill is hidden whenever the library's embedded strip is the
  // visible recording UI — including across a failed-upload → Resume, where it
  // stays hidden from the prior stop. Show it before capture so getUserMedia
  // can resolve; the native pill watcher re-hides it once capture is active.
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
  const analyser = recorder.getAnalyser();
  if (analyser) {
    waveform.start(analyser);
  }
  await invoke('set_tray_recording', { isRecording: true, isPaused: false });
}

// Auto-trigger: attach to a matching calendar meeting when one is found. The
// user has already opted in via the pre-recording notification prompt (or
// auto-record is on), so there's no in-pill confirmation — a no-match recording
// simply proceeds unattached.
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
      }
    }
  } catch (e) {
    console.error('Auto-trigger calendar match failed; recording unattached', e);
  }
}

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
  waveform.stop();
  try {
    await recorder.stopRecording();
  } catch {
    /* best-effort */
  }
  await invoke('set_tray_recording', { isRecording: false, isPaused: false });
  await closeWindow();
}

async function localFinalizationId(startAt: string | null): Promise<string | undefined> {
  if (backend.value?.id !== 'local' || !startAt) return undefined;
  if (effectiveLocalRecordingId.value) return effectiveLocalRecordingId.value;
  if (localAppendId) return localAppendId;

  // A very short recording can stop before the startedAt watcher finishes its
  // lookup. Resolve the same target here so failure recovery never falls back
  // to a fresh timestamp id when Rust actually appended to an older recording.
  try {
    const id = await local.recordingIdForStart(startAt, forceNew);
    effectiveLocalRecordingId.value = id;
    return id;
  } catch (e) {
    console.error('Failed to resolve local finalization id at stop', e);
    return localRecordingIdFromStart(startAt);
  }
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
  collapse();
  isUploading.value = true;
  waveform.stop();
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
    const finalizedLocalId =
      prevMeta?.localRecordingId ?? await localFinalizationId(prevMeta?.startAt ?? startAt);
    stoppedBlob.value = combinedBlob;
    stoppedMeta.value = {
      startAt: prevMeta?.startAt ?? startAt,
      endAt,
      durationSeconds:
        (prevMeta?.durationSeconds ?? 0) + recorder.durationSeconds.value,
      meetingId: prevMeta?.meetingId ?? effectiveMeetingId.value ?? undefined,
      localAppendId: prevMeta?.localAppendId ?? localAppendId ?? undefined,
      localRecordingId: finalizedLocalId,
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
    // Local transcription failures are terminal but recoverable: Rust writes
    // the audio + Failed metadata before returning the error, and Meeting
    // detail owns retry from that persisted audio. Do not retain the hidden
    // waveform (and block a new recording) once persistence is confirmed.
    const meta = stoppedMeta.value;
    if (backend.value.id === 'local' && meta) {
      const failedId =
        meta.localRecordingId ?? localRecordingIdFromStart(meta.startAt ?? meta.endAt);
      try {
        const persisted = await local.recordingStatus(failedId);
        if (persisted.status === 'failed') {
          stoppedBlob.value = null;
          stoppedMeta.value = null;
          await closeWindow();
          return;
        }
      } catch {
        // If persistence cannot be confirmed, retain the in-memory blob and
        // recovery controls exactly as before so audio is never discarded.
      }
    }
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

  // Match the window's click behavior to its initial (param-driven) paint state.
  await applyPillVisibility(pillHidden.value);
  unlistenPillVisible = await listen<boolean>('recorder://pill-visible', (e) => {
    void applyPillVisibility(!e.payload);
  });

  unlistenPendingUploaded = await listen('pending-upload://succeeded', handlePendingUploadSucceeded);

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
  unlistenPillVisible?.();
  unlistenPendingUploaded?.();
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

<style scoped>
/* Fills the (fixed-size) window and bottom-centers the pill, leaving transparent
   room above and around it for the shadow and the upward growth. */
.stage {
  width: 100vw;
  height: 100vh;
  display: flex;
  align-items: flex-end;
  justify-content: center;
  padding-bottom: 22px;
  box-sizing: border-box;
}

.pill {
  width: 48px;
  background: #0d0d0d;
  border-radius: 24px;
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 7px 0;
  box-sizing: border-box;
  overflow: hidden;
  cursor: grab; /* open hand on hover */
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5);
}
.pill:active { cursor: grabbing; } /* closed/grabbing hand while pressed */

.logo {
  width: 28px;
  height: 28px;
  object-fit: contain;
  flex-shrink: 0;
}

.bars {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  height: 18px;
  margin-top: 7px;
  flex-shrink: 0;
}

.bar {
  width: 3px;
  border-radius: 2px;
  background: #f9d852;
  transition: height 75ms, background 150ms;
}

.bar.paused {
  background: #4b5563;
}

/* Revealed on hover: animates open/closed so the pill grows/shrinks smoothly. */
.expanded-area {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  max-height: 0;
  margin-top: 0;
  opacity: 0;
  overflow: hidden;
  pointer-events: none;
  flex-shrink: 0;
  transition: max-height 180ms ease, margin-top 180ms ease, opacity 150ms ease;
}

.expanded-area.open {
  max-height: 130px;
  margin-top: 7px;
  opacity: 1;
  pointer-events: auto;
}

.timer {
  font-size: 10px;
  font-family: monospace;
  color: #9ca3af;
}

.ctrl-btn {
  width: 34px;
  height: 34px;
  border-radius: 8px;
  border: none;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #1f1f1f;
  cursor: pointer;
  transition: background 0.15s;
}

.ctrl-btn:hover {
  background: #2a2a2a;
}

.stop-btn { color: #f87171; }
.pause-btn { color: #ffffff; }

/* Fixed margin above + the pill's bottom padding below give the handle the same
   surrounding space whether the pill is collapsed or expanded. */
.drag-handle {
  display: flex;
  flex-direction: column;
  align-items: center;
  margin-top: 7px;
  flex-shrink: 0;
  cursor: grab;
}
/* Give the nested handle graphics the same affordance as their hit target. */
.drag-handle * { cursor: grab; }
.drag-handle:active,
.drag-handle:active * { cursor: grabbing; }

.divider {
  width: 22px;
  height: 1px;
  background: rgba(255, 255, 255, 0.08);
}

.drag-dots {
  display: grid;
  grid-template-columns: repeat(3, 3.2px);
  gap: 2.4px 3.2px;
  justify-content: center;
  margin-top: 6px;
}

.dot {
  width: 3.2px;
  height: 3.2px;
  border-radius: 50%;
  background: #6b7280;
}

.status-icon {
  margin-top: 8px;
  font-size: 18px;
  font-weight: 700;
}

.status-icon.ok { color: #34d399; }
.status-icon.err { color: #f87171; }

.retry-btn {
  margin-top: 8px;
  color: #818cf8;
  font-size: 15px;
  font-weight: 700;
}
.resume-btn {
  margin-top: 6px;
  color: #34d399;
  font-size: 13px;
  font-weight: 700;
}
.dismiss-btn {
  margin-top: 6px;
  color: #f87171;
  font-size: 13px;
  font-weight: 700;
}

.spinner {
  margin-top: 8px;
  width: 16px;
  height: 16px;
  border: 2px solid #4b5563;
  border-top-color: #818cf8;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
