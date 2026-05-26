<template>
  <div class="waveform-container" data-tauri-drag-region>
    <template v-if="uploadResult">
      <div class="upload-info" data-tauri-drag-region>
        <span :class="uploadResult === 'success' ? 'upload-check' : 'upload-error-icon'">
          {{ uploadResult === 'success' ? '✓' : '✗' }}
        </span>
        <span class="upload-label">{{ uploadResult === 'success' ? 'Upload successful' : 'Upload failed' }}</span>
        <button class="close-btn" @click.stop.prevent="closeWindow">Close</button>
      </div>
    </template>
    <template v-else-if="isUploading">
      <div class="upload-info" data-tauri-drag-region>
        <span class="upload-spinner" />
        <span class="upload-label">Uploading…</span>
      </div>
    </template>
    <template v-else>
      <div class="bars" data-tauri-drag-region>
        <div
          v-for="(level, i) in waveform.levels.value"
          :key="i"
          class="bar"
          :class="{ paused: recorder.isPaused.value }"
          :style="{ height: `${Math.max(8, level * 100)}%` }"
        />
      </div>
      <div class="info" data-tauri-drag-region>
        <template v-if="!recorder.isPaused.value">
          <span class="rec-dot" />
          <span class="rec-label">REC</span>
        </template>
        <span v-else class="paused-label">PAUSED</span>
        <span class="timer">{{ formattedDuration }}</span>
      </div>
      <div class="controls">
        <button
          class="ctrl-btn pause-resume-btn"
          :aria-label="recorder.isPaused.value ? 'Resume recording' : 'Pause recording'"
          @click.stop.prevent="recorder.isPaused.value ? handleResume() : handlePause()"
        >
          <svg v-if="!recorder.isPaused.value" width="14" height="14" viewBox="0 0 14 14">
            <rect x="2" y="1" width="3.5" height="12" rx="1" fill="currentColor" />
            <rect x="8.5" y="1" width="3.5" height="12" rx="1" fill="currentColor" />
          </svg>
          <svg v-else width="14" height="14" viewBox="0 0 14 14">
            <polygon points="3,1 13,7 3,13" fill="currentColor" />
          </svg>
        </button>
        <button
          class="ctrl-btn stop-btn"
          aria-label="Stop recording"
          @click.stop.prevent="handleStop"
        >
          <svg width="12" height="12" viewBox="0 0 12 12">
            <rect x="1" y="1" width="10" height="10" rx="2" fill="currentColor" />
          </svg>
        </button>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { load } from '@tauri-apps/plugin-store';
import { useRecorder, type RecordingMode } from '../composables/useRecorder';
import { useWaveform } from '../composables/useWaveform';
import { useMeetingApi } from '../composables/useMeetingApi';

const recorder = useRecorder();
const waveform = useWaveform();
const meetingApi = useMeetingApi();
const isUploading = ref(false);
const uploadResult = ref<'success' | 'failed' | null>(null);

const formattedDuration = computed(() => {
  const s = recorder.durationSeconds.value;
  const mins = Math.floor(s / 60).toString().padStart(2, '0');
  const secs = (s % 60).toString().padStart(2, '0');
  return `${mins}:${secs}`;
});

let unlistenPause: UnlistenFn | null = null;
let unlistenResume: UnlistenFn | null = null;
let unlistenStop: UnlistenFn | null = null;

async function startRecording() {
  const store = await load('settings.json', { autoSave: true });
  const savedMode = await store.get<string>('recordingMode');
  const mode: RecordingMode = savedMode === 'mic' ? 'mic' : 'mic_and_system';

  try {
    await recorder.startRecording(mode);
  } catch {
    // Recording failed (permission denied, device error, etc.)
    // Roll back tray to idle state and close the window.
    await invoke('set_tray_recording', { isRecording: false, isPaused: false });
    try { await getCurrentWebviewWindow().close(); } catch { /* ignore */ }
    return;
  }
  const analyser = recorder.getAnalyser();
  if (analyser) {
    waveform.start(analyser);
  }
  await invoke('set_tray_recording', { isRecording: true, isPaused: false });
}

async function handleStop() {
  waveform.stop();
  const endAt = new Date().toISOString();
  isUploading.value = true;
  const mp3Blob = await recorder.stopRecording();
  await invoke('set_tray_recording', { isRecording: false, isPaused: false });

  if (mp3Blob.size > 0) {
    const timeout = new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error('Upload timed out')), 30_000)
    );
    try {
      await Promise.race([
        meetingApi.uploadAudio(mp3Blob, { endAt }),
        timeout,
      ]);
      uploadResult.value = 'success';
    } catch (err) {
      console.error('Upload failed:', err);
      uploadResult.value = 'failed';
    }
  } else {
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

  unlistenPause = await listen('tray://pause-recording', handlePause);
  unlistenResume = await listen('tray://resume-recording', handleResume);
  unlistenStop = await listen('tray://stop-recording', handleStop);

  await startRecording();
});

onUnmounted(() => {
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
  overflow: hidden;
}
</style>

<style scoped>
.waveform-container {
  width: 320px;
  height: 56px;
  background: #0f0f1a;
  border-radius: 12px;
  display: flex;
  align-items: center;
  padding: 0 12px;
  overflow: hidden;
  cursor: grab;
}

.bars {
  display: flex;
  align-items: center;
  gap: 2px;
  height: 36px;
  flex: 1;
  min-width: 0;
}

.bar {
  width: 2px;
  border-radius: 2px;
  background: #ffffff;
  transition: height 75ms, width 150ms, background 150ms;
}

.bar.paused {
  width: 3px;
  background: #4b5563;
}

.info {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-left: 10px;
  flex-shrink: 0;
}

.rec-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #f87171;
  animation: pulse 1s infinite;
}

.rec-label {
  font-size: 10px;
  font-weight: 600;
  color: #f87171;
  letter-spacing: 0.5px;
}

.paused-label {
  font-size: 10px;
  font-weight: 600;
  color: #9ca3af;
  letter-spacing: 0.5px;
}

.timer {
  font-size: 11px;
  color: #999;
  font-family: monospace;
}

.controls {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-left: 10px;
  flex-shrink: 0;
}

.ctrl-btn {
  width: 28px;
  height: 28px;
  border-radius: 6px;
  border: none;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: background 0.15s;
}

.pause-resume-btn {
  background: #1e1e2e;
  color: #d1d5db;
}

.pause-resume-btn:hover {
  background: #2a2a3e;
}

.stop-btn {
  background: #1e1e2e;
  color: #f87171;
}

.stop-btn:hover {
  background: #2a2a3e;
}

.upload-info {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  justify-content: center;
}

.upload-spinner {
  width: 14px;
  height: 14px;
  border: 2px solid #4b5563;
  border-top-color: #818cf8;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

.upload-label {
  font-size: 12px;
  font-weight: 600;
  color: #9ca3af;
  letter-spacing: 0.5px;
}

.upload-check {
  font-size: 14px;
  font-weight: 700;
  color: #34d399;
}

.upload-error-icon {
  font-size: 14px;
  font-weight: 700;
  color: #f87171;
}

.close-btn {
  margin-left: 8px;
  padding: 2px 10px;
  border-radius: 6px;
  border: none;
  background: #1e1e2e;
  color: #d1d5db;
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s;
}

.close-btn:hover {
  background: #2a2a3e;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
