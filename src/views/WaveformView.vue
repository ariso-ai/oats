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
      <img class="logo" src="../assets/icon-r-b.png" alt="" />

      <template v-if="uploadResult">
        <span class="status-icon" :class="uploadResult === 'success' ? 'ok' : 'err'">
          {{ uploadResult === 'success' ? '✓' : '✗' }}
        </span>
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
import { computed, ref, onMounted, onUnmounted } from 'vue';
import { useRoute } from 'vue-router';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { useRecorder } from '../composables/useRecorder';
import { useWaveform } from '../composables/useWaveform';
import { getActiveBackend, type Backend } from '../composables/useBackend';
import { loadRecordingEnabled } from '../composables/useRecordingPermissions';
import { deriveRecordingMode } from './recordingSettings';
import { bucketLevels } from './waveformBars';

const SUCCESS_CLOSE_MS = 1500;

const recorder = useRecorder();
const waveform = useWaveform();
const backend = ref<Backend | null>(null);
const isUploading = ref(false);
const uploadResult = ref<'success' | 'failed' | null>(null);
const isExpanded = ref(false);

// Voice energy lives in the low FFT bins; the upper bins are near-silent and
// would leave bars 2-3 dead. Bucket only the low part of the spectrum so all
// three bars react to speech.
const bars = computed(() => bucketLevels(waveform.levels.value.slice(0, 12), 3));

const route = useRoute();
const meetingIdQuery = route.query.meetingId;
const meetingId: number | null =
  typeof meetingIdQuery === 'string' && /^\d+$/.test(meetingIdQuery)
    ? Number(meetingIdQuery)
    : null;

const formattedDuration = computed(() => {
  const s = recorder.durationSeconds.value;
  const mins = Math.floor(s / 60).toString().padStart(2, '0');
  const secs = (s % 60).toString().padStart(2, '0');
  return `${mins}:${secs}`;
});

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
let closeTimer: ReturnType<typeof setTimeout> | null = null;

// Reset the tray to idle and close the recording window. Best-effort: a
// failure of either step must not throw out of the abort/rollback path.
async function rollbackAndClose() {
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

async function handleStop() {
  collapse();
  isUploading.value = true;
  waveform.stop();
  const endAt = new Date().toISOString();
  const startAt = recorder.startedAt.value;
  const mp3Blob = await recorder.stopRecording();
  await invoke('set_tray_recording', { isRecording: false, isPaused: false });

  if (mp3Blob.size > 0 && backend.value) {
    // This only bounds the UI wait. A timed-out local transcription keeps
    // running natively and still writes its final status to meta.json, so the
    // Library (source of truth) may show 'done'/'failed' even if the window
    // showed a timeout. Audio is persisted before transcription, so nothing is lost.
    const timeout = new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error('Operation timed out')), 120_000)
    );
    try {
      await Promise.race([
        backend.value.finalizeRecording(mp3Blob, {
          startAt,
          endAt,
          durationSeconds: recorder.durationSeconds.value,
          meetingId: meetingId ?? undefined,
        }),
        timeout,
      ]);
      uploadResult.value = 'success';
      // Brief confirmation, then auto-close.
      closeTimer = setTimeout(() => { closeTimer = null; void closeWindow(); }, SUCCESS_CLOSE_MS);
    } catch (err) {
      console.error('Finalize failed:', err);
      // Stay open on failure so the user can drag away / dismiss via the tray.
      uploadResult.value = 'failed';
    }
  } else {
    if (mp3Blob.size > 0 && !backend.value) {
      console.error('handleStop: backend not initialized; discarding recording');
    }
    await closeWindow();
  }

  isUploading.value = false;
}

async function closeWindow() {
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
});

onUnmounted(() => {
  if (closeTimer) clearTimeout(closeTimer);
  unlistenPause?.();
  unlistenResume?.();
  unlistenStop?.();
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
  cursor: grab;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5);
}

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
  height: 28px;
  margin-top: 7px;
  flex-shrink: 0;
}

.bar {
  width: 3px;
  border-radius: 2px;
  background: #ffffff;
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
}

.divider {
  width: 22px;
  height: 1px;
  background: rgba(255, 255, 255, 0.08);
}

.drag-dots {
  display: grid;
  grid-template-columns: repeat(3, 4px);
  gap: 3px 4px;
  justify-content: center;
  margin-top: 6px;
}

.dot {
  width: 4px;
  height: 4px;
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
