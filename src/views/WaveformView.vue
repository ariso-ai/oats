<template>
  <!-- The window is a fixed size (room for the expanded pill + its shadow); the
       pill is anchored to the bottom and grows UPWARD via a CSS transition. -->
  <div class="stage">
    <div
      class="pill"
      :class="{ expanded: isExpanded, paused: recorder.isPaused.value }"
      @mouseenter="expand"
      @mouseleave="collapse"
      @click="showMeetings"
    >
      <img class="logo" src="../assets/oats-dark.svg" alt="" />

      <template v-if="uploadResult === 'failed'">
        <span class="status-icon err">✗</span>
        <button
          class="ctrl-btn retry-btn"
          aria-label="Retry upload"
          @click.stop.prevent="runFinalize"
        >↻</button>
        <button
          class="ctrl-btn dismiss-btn"
          aria-label="Dismiss recording"
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
            :aria-label="recorder.isPaused.value ? 'Resume recording' : 'Pause recording'"
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
            aria-label="Stop recording"
            @click.stop.prevent="handleStop"
          >
            <svg width="14" height="14" viewBox="0 0 14 14">
              <rect x="2" y="2" width="10" height="10" rx="2" fill="currentColor" />
            </svg>
          </button>
        </div>

        <div v-if="confirmVisible" class="confirm">
          <button
            class="ctrl-btn keep-btn"
            aria-label="Keep recording"
            @click.stop.prevent="keepRecording"
          >✓</button>
          <button
            class="ctrl-btn discard-btn"
            aria-label="Discard recording"
            @click.stop.prevent="discardRecording"
          >✕</button>
        </div>

        <!-- Tauri only drags when the mousedown target itself carries the
             attribute, so every leaf in the handle needs it. -->
        <div class="drag-handle" data-tauri-drag-region @click.stop>
          <div class="divider" data-tauri-drag-region />
          <div class="drag-dots" data-tauri-drag-region>
            <span v-for="n in 6" :key="n" class="dot" data-tauri-drag-region />
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
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { useRecorder } from '../composables/useRecorder';
import { useWaveform } from '../composables/useWaveform';
import { getActiveBackend, type Backend, type RecordingMeta } from '../composables/useBackend';
import { pending } from '../tauri';
import { loadRecordingEnabled } from '../composables/useRecordingPermissions';
import { deriveRecordingMode } from './recordingSettings';
import { centerWeightedBars } from './waveformBars';
import { shouldAutoStop } from '../composables/silenceWatch';
import { localRecordingIdFromStart } from '../composables/localRecordingId';
import { resolveAssociation } from '../composables/useAutoTrigger';
import { useMeetingApi } from '../composables/useMeetingApi';

const SUCCESS_CLOSE_MS = 1500;

const recorder = useRecorder();
const waveform = useWaveform();
const backend = ref<Backend | null>(null);
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
const isAuto = route.query.auto === '1';
const confirmVisible = ref(false);
const isStopping = ref(false);
let confirmTimer: ReturnType<typeof setTimeout> | null = null;
const CONFIRM_TIMEOUT_MS = 60_000;
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
type RecorderPhase = 'recording' | 'uploading' | 'success' | 'failed' | 'closed';

function currentPhase(): RecorderPhase {
  if (uploadResult.value) return uploadResult.value;
  if (isUploading.value) return 'uploading';
  return 'recording';
}

// Once `closed` is sent, stay silent: a heartbeat firing between the closed
// broadcast and the window's destruction would otherwise revive the strip.
let closedSent = false;

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
    // Local recordings have no meeting id, but their finalized recording id is
    // deterministic from the start time — broadcast it so the library can pin
    // the strip / red dot to the row the recording will land on.
    localRecordingId:
      backend.value?.id === 'local' && recorder.startedAt.value
        ? localRecordingIdFromStart(recorder.startedAt.value)
        : null,
    phase,
  }).catch(() => { /* no listeners / shutting down */ });
}

watch(() => recorder.frameLevels.value, () => broadcastState());
watch(
  [() => recorder.durationSeconds.value, () => recorder.isPaused.value, isUploading, uploadResult],
  () => broadcastState(),
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

// Clicking the pill body (not the controls or the drag handle) brings up the
// meetings window.
async function showMeetings() {
  try {
    await invoke('create_library_window');
  } catch (e) {
    console.error('Failed to open meetings window', e);
  }
}

let unlistenPause: UnlistenFn | null = null;
let unlistenResume: UnlistenFn | null = null;
let unlistenStop: UnlistenFn | null = null;
let unlistenAutoStop: UnlistenFn | null = null;
let closeTimer: ReturnType<typeof setTimeout> | null = null;
let silenceTimer: ReturnType<typeof setInterval> | null = null;

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
  let mode: ReturnType<typeof deriveRecordingMode>;
  try {
    mode = deriveRecordingMode(await loadRecordingEnabled());
  } catch {
    await rollbackAndClose();
    return;
  }
  if (mode === null) {
    await rollbackAndClose();
    return;
  }

  try {
    await recorder.startRecording(mode);
  } catch {
    await rollbackAndClose();
    return;
  }
  const analyser = recorder.getAnalyser();
  if (analyser) {
    waveform.start(analyser);
  }
  await invoke('set_tray_recording', { isRecording: true, isPaused: false });
}

// Auto-trigger: resolve calendar association, falling back to a confirm prompt.
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
        return;
      }
    }
  } catch (e) {
    console.error('Auto-trigger match failed; asking for confirmation', e);
  }
  showConfirm();
}

function showConfirm() {
  confirmVisible.value = true;
  // No response within the window → discard (per spec).
  confirmTimer = setTimeout(() => {
    confirmTimer = null;
    void discardRecording();
  }, CONFIRM_TIMEOUT_MS);
}

function keepRecording() {
  if (confirmTimer) {
    clearTimeout(confirmTimer);
    confirmTimer = null;
  }
  confirmVisible.value = false;
}

// Discard the in-progress capture without uploading, then close.
async function discardRecording() {
  if (isStopping.value) return;
  isStopping.value = true;
  if (confirmTimer) {
    clearTimeout(confirmTimer);
    confirmTimer = null;
  }
  if (closeTimer) {
    clearTimeout(closeTimer);
    closeTimer = null;
  }
  confirmVisible.value = false;
  if (silenceTimer) {
    clearInterval(silenceTimer);
    silenceTimer = null;
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

async function handleStop() {
  if (isStopping.value) return;
  // An unanswered confirm overlay means the user never opted in — a stop of any
  // kind (native mic-off, silence backstop, tray) must discard, not upload.
  if (confirmVisible.value) {
    await discardRecording();
    return;
  }
  if (isAuto && recorder.durationSeconds.value < MIN_AUTO_DURATION_S) {
    await discardRecording();
    return;
  }
  isStopping.value = true;
  // Tear down the auto-trigger/backstop timers so they can't fire post-stop.
  if (confirmTimer) {
    clearTimeout(confirmTimer);
    confirmTimer = null;
  }
  if (silenceTimer) {
    clearInterval(silenceTimer);
    silenceTimer = null;
  }
  collapse();
  isUploading.value = true;
  waveform.stop();
  const endAt = new Date().toISOString();
  const startAt = recorder.startedAt.value;
  const mp3Blob = await recorder.stopRecording();
  await invoke('set_tray_recording', { isRecording: false, isPaused: false });

  if (mp3Blob.size > 0 && backend.value) {
    stoppedBlob.value = mp3Blob;
    stoppedMeta.value = {
      startAt,
      endAt,
      durationSeconds: recorder.durationSeconds.value,
      meetingId: effectiveMeetingId.value ?? undefined,
    };
    await runFinalize();
  } else {
    if (mp3Blob.size > 0 && !backend.value) {
      console.error('handleStop: backend not initialized; discarding recording');
    }
    await closeWindow();
  }
}

// Upload the stopped recording. Shared by the stop flow and the failed pill's
// Retry button — blob and meta stay in refs so retry needs no re-record.
// Tracks the underlying finalize promise (not the UI-timeout race) so that a
// timed-out attempt whose work is still running won't be re-launched by Retry.
let inFlightFinalize: Promise<unknown> | null = null;
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
    await Promise.race([work, timeout]);
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

  unlistenPause = await listen('tray://pause-recording', handlePause);
  unlistenResume = await listen('tray://resume-recording', handleResume);
  unlistenStop = await listen('tray://stop-recording', handleStop);

  await startRecording();

  // Universal silence backstop: end any recording after 15 min of no sound.
  silenceTimer = setInterval(() => {
    if (isUploading.value || uploadResult.value || !recorder.isRecording.value) return;
    if (
      shouldAutoStop(
        recorder.lastSoundAt.value,
        Date.now(),
        recorder.isPaused.value,
      )
    ) {
      void handleStop();
    }
  }, 1_000);

  unlistenAutoStop = await listen('auto-record://stop', handleStop);
  if (isAuto) {
    void resolveAuto();
  }
});

onUnmounted(() => {
  if (silenceTimer) clearInterval(silenceTimer);
  if (closeTimer) clearTimeout(closeTimer);
  unlistenPause?.();
  unlistenResume?.();
  unlistenStop?.();
  unlistenAutoStop?.();
  if (confirmTimer) clearTimeout(confirmTimer);
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
/* Children carry the drag-region attribute individually, so give them the
   grab cursor too; switch to grabbing while actively dragging. */
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

.confirm {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  margin-top: 7px;
  flex-shrink: 0;
}
.keep-btn { color: #34d399; font-size: 16px; font-weight: 700; }
.discard-btn { color: #f87171; font-size: 14px; font-weight: 700; }

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
