<template>
  <div class="recorder">
    <template v-if="uploadResult">
      <div class="upload-info">
        <span :class="uploadResult === 'success' ? 'upload-check' : 'upload-error-icon'">
          {{ uploadResult === 'success' ? '✓' : '✗' }}
        </span>
        <span class="upload-label">
          {{ uploadResult === 'success' ? successLabel : failLabel }}
        </span>
        <button class="close-btn" @click.stop.prevent="close">Close</button>
      </div>
    </template>
    <template v-else-if="isUploading">
      <div class="upload-info">
        <span class="upload-spinner" />
        <span class="upload-label">{{ progressLabel }}</span>
      </div>
    </template>
    <template v-else>
      <div class="bars">
        <div
          v-for="(level, i) in waveform.levels.value"
          :key="i"
          class="bar"
          :class="{ paused: recorder.isPaused.value }"
          :style="{ height: `${Math.max(8, level * 100)}%` }"
        />
      </div>
      <div class="info">
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
            <circle cx="7" cy="7" r="5.5" fill="currentColor" />
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
import { useRecorder } from '../composables/useRecorder';
import { useWaveform } from '../composables/useWaveform';
import { getActiveBackend, type Backend } from '../composables/useBackend';
import { loadRecordingEnabled } from '../composables/useRecordingPermissions';
import { deriveRecordingMode } from './recordingSettings';

const emit = defineEmits<{ done: [] }>();

const recorder = useRecorder();
const waveform = useWaveform();
const backend = ref<Backend | null>(null);
const isLocal = computed(() => backend.value?.id === 'local');
const successLabel = computed(() => (isLocal.value ? 'Transcription complete' : 'Upload successful'));
const failLabel = computed(() => (isLocal.value ? 'Transcription failed' : 'Upload failed'));
const progressLabel = computed(() => (isLocal.value ? 'Transcribing…' : 'Uploading…'));
const isUploading = ref(false);
const uploadResult = ref<'success' | 'failed' | null>(null);
const isStopping = ref(false);

const formattedDuration = computed(() => {
  const s = recorder.durationSeconds.value;
  const mins = Math.floor(s / 60).toString().padStart(2, '0');
  const secs = (s % 60).toString().padStart(2, '0');
  return `${mins}:${secs}`;
});

async function startRecording() {
  let mode: ReturnType<typeof deriveRecordingMode>;
  try {
    mode = deriveRecordingMode(await loadRecordingEnabled());
  } catch {
    emit('done');
    return;
  }
  if (mode === null) {
    // Both recording sources disabled — nothing to capture.
    emit('done');
    return;
  }
  try {
    await recorder.startRecording(mode);
  } catch {
    emit('done');
    return;
  }
  const analyser = recorder.getAnalyser();
  if (analyser) {
    waveform.start(analyser);
  }
}

async function handleStop() {
  if (isStopping.value) return;
  isStopping.value = true;
  waveform.stop();
  const endAt = new Date().toISOString();
  const startAt = recorder.startedAt.value;
  isUploading.value = true;
  let mp3Blob: Blob;
  try {
    mp3Blob = await recorder.stopRecording();
  } catch (err) {
    // A stop failure would otherwise leave the panel stuck on the uploading
    // template, which has no Close button. Reset state and bail to the parent.
    console.error('Stop failed:', err);
    isUploading.value = false;
    isStopping.value = false;
    emit('done');
    return;
  }

  if (mp3Blob.size > 0 && backend.value) {
    // Bound only the UI wait; a timed-out local transcription keeps running
    // natively and still records its final status (the list is the source of truth).
    const timeout = new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error('Operation timed out')), 120_000)
    );
    try {
      await Promise.race([
        backend.value.finalizeRecording(mp3Blob, {
          startAt,
          endAt,
          durationSeconds: recorder.durationSeconds.value,
        }),
        timeout,
      ]);
      uploadResult.value = 'success';
    } catch (err) {
      console.error('Finalize failed:', err);
      uploadResult.value = 'failed';
    }
  } else {
    if (mp3Blob.size > 0 && !backend.value) {
      console.warn('RecorderPanel: backend not initialized; discarding recording');
    }
    emit('done');
  }
  isUploading.value = false;
}

function handlePause() {
  recorder.pauseRecording();
}

function handleResume() {
  recorder.resumeRecording();
}

function close() {
  emit('done');
}

onMounted(async () => {
  backend.value = await getActiveBackend();
  await startRecording();
});

onUnmounted(() => {
  // useWaveform cleans itself up via its own onUnmounted hook. If the component
  // is torn down mid-recording, stop the recorder too so the AudioContext, mic
  // stream, timer, and system-audio listener don't leak (mic indicator staying lit).
  if (recorder.isRecording.value) {
    recorder.stopRecording().catch(() => {
      /* best-effort cleanup */
    });
  }
});
</script>

<style scoped>
.recorder {
  height: 56px;
  background: #0f0f1a;
  border-radius: 12px;
  display: flex;
  align-items: center;
  padding: 0 12px;
  overflow: hidden;
}
.bars { display: flex; align-items: center; gap: 2px; height: 36px; flex: 1; min-width: 0; }
.bar { width: 2px; border-radius: 2px; background: #ffffff; transition: height 75ms, width 150ms, background 150ms; }
.bar.paused { width: 3px; background: #4b5563; }
.info { display: flex; align-items: center; gap: 6px; margin-left: 10px; flex-shrink: 0; }
.rec-dot { width: 6px; height: 6px; border-radius: 50%; background: #f87171; animation: pulse 1s infinite; }
.rec-label { font-size: 10px; font-weight: 600; color: #f87171; letter-spacing: 0.5px; }
.paused-label { font-size: 10px; font-weight: 600; color: #9ca3af; letter-spacing: 0.5px; }
.timer { font-size: 11px; color: #999; font-family: monospace; }
.controls { display: flex; align-items: center; gap: 6px; margin-left: 10px; flex-shrink: 0; }
.ctrl-btn { width: 28px; height: 28px; border-radius: 6px; border: none; display: flex; align-items: center; justify-content: center; cursor: pointer; transition: background 0.15s; }
.pause-resume-btn { background: #1e1e2e; color: #d1d5db; }
.pause-resume-btn:hover { background: #2a2a3e; }
.stop-btn { background: #1e1e2e; color: #f87171; }
.stop-btn:hover { background: #2a2a3e; }
.upload-info { display: flex; align-items: center; gap: 8px; width: 100%; justify-content: center; }
.upload-spinner { width: 14px; height: 14px; border: 2px solid #4b5563; border-top-color: #818cf8; border-radius: 50%; animation: spin 0.8s linear infinite; }
.upload-label { font-size: 12px; font-weight: 600; color: #9ca3af; letter-spacing: 0.5px; }
.upload-check { font-size: 14px; font-weight: 700; color: #34d399; }
.upload-error-icon { font-size: 14px; font-weight: 700; color: #f87171; }
.close-btn { margin-left: 8px; padding: 2px 10px; border-radius: 6px; border: none; background: #1e1e2e; color: #d1d5db; font-size: 11px; font-weight: 600; cursor: pointer; transition: background 0.15s; }
.close-btn:hover { background: #2a2a3e; }
@keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.4; } }
@keyframes spin { to { transform: rotate(360deg); } }
</style>
