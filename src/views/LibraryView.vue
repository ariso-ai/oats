<template>
  <div class="library">
    <aside class="left-panel">
      <h1 class="title">Meetings</h1>
      <p v-if="loading" class="hint">Loading…</p>
      <p v-else-if="error" class="hint">{{ error }}</p>
      <p v-else-if="meetings.length === 0" class="hint">No meetings yet.</p>
      <ul v-else class="list">
          <!-- key is backend-scoped: the list always comes from a single backend at a time -->
        <li v-for="m in meetings" :key="m.id" class="recording-row">
          <div class="row-main">
            <span class="row-title">{{ m.title }}</span>
            <span v-if="m.status" class="row-status" :class="`status-${m.status}`">{{ m.status }}</span>
          </div>
          <div class="row-sub">
            <span>{{ formatDate(m.timestamp) }}</span>
            <span v-if="m.durationSeconds != null">{{ formatDuration(m.durationSeconds) }}</span>
          </div>
          <div v-if="m.files" class="row-controls">
            <RecordingAudioPlayer :id="m.id" :has-audio="m.files.hasAudio" />
            <button class="btn-note" :disabled="!m.files.hasNote" @click="openNote(m.id)">Note</button>
            <button class="btn-transcript" :disabled="!m.files.hasTranscript" @click="openTranscript(m.id)">Transcript</button>
          </div>
        </li>
      </ul>
    </aside>

    <section class="right-panel">
      <header class="right-header">
        <button
          v-if="!recording"
          class="record-btn"
          aria-label="Start recording"
          @click="startRecording"
        >
          <span class="record-dot" />
        </button>
      </header>
      <div class="right-body" />
    </section>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { getAllWebviewWindows } from '@tauri-apps/api/webviewWindow';
import { local } from '../tauri';
import { getActiveBackend, type MeetingListItem } from '../composables/useBackend';
import RecordingAudioPlayer from './RecordingAudioPlayer.vue';

const meetings = ref<MeetingListItem[]>([]);
const loading = ref(true);
const error = ref<string | null>(null);
const recording = ref(false);

function formatDuration(secs: number): string {
  const m = Math.floor(secs / 60).toString().padStart(2, '0');
  const s = Math.floor(secs % 60).toString().padStart(2, '0');
  return `${m}:${s}`;
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
}

async function loadMeetings(): Promise<void> {
  loading.value = true;
  error.value = null;
  try {
    meetings.value = await (await getActiveBackend()).listMeetings();
  } catch (e) {
    console.error('Failed to list meetings', e);
    error.value = 'Could not load meetings.';
  } finally {
    loading.value = false;
  }
}

async function openNote(id: string): Promise<void> {
  try {
    await local.openRecordingFile(id, 'note');
  } catch (e) {
    console.error('Failed to open note', e);
  }
}

async function openTranscript(id: string): Promise<void> {
  try {
    await local.openRecordingFile(id, 'transcript');
  } catch (e) {
    console.error('Failed to open transcript', e);
  }
}

// Recording runs in the separate "waveform" window; its presence is our
// recording signal. Used to hide the Record button while a recording is active.
async function refreshRecordingState(): Promise<void> {
  try {
    const wins = await getAllWebviewWindows();
    recording.value = wins.some((w) => w.label === 'waveform');
  } catch (e) {
    console.error('Failed to read window state', e);
  }
}

// Open the floating recorder pill (its own always-on-top window) instead of an
// in-window dock. The window dedups itself if one is already open.
async function startRecording(): Promise<void> {
  try {
    await invoke('start_recording_window', {});
    recording.value = true; // hide the button immediately; refreshed on focus
  } catch (e) {
    console.error('Failed to start recording window', e);
  }
}

// The floating recorder lives in a separate window, so the list can't react to a
// "done" callback. On focus (e.g. after the recorder finishes/closes) reload the
// meetings and re-check whether a recording is still in progress.
function onWindowFocus(): void {
  void loadMeetings();
  void refreshRecordingState();
}

onMounted(() => {
  void loadMeetings();
  void refreshRecordingState();
  window.addEventListener('focus', onWindowFocus);
});

onUnmounted(() => {
  window.removeEventListener('focus', onWindowFocus);
});
</script>

<style scoped>
.library {
  display: flex;
  height: 100vh;
  font-family: -apple-system, system-ui, sans-serif;
  box-sizing: border-box;
}

/* Left panel: meetings list */
.left-panel {
  width: 300px;
  flex-shrink: 0;
  background: #f5f5f7;
  border-right: 1px solid #e5e5ea;
  padding: 24px;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
}
.title { font-size: 20px; font-weight: 700; margin-bottom: 16px; color: #1d1d1f; flex-shrink: 0; }
.hint { font-size: 14px; color: #86868b; }
.list { list-style: none; margin: 0; padding: 0 4px 0 0; display: flex; flex-direction: column; gap: 8px; flex: 1; min-height: 0; overflow-y: auto; }
.recording-row { background: #fff; border-radius: 10px; padding: 12px 14px; box-shadow: 0 1px 3px rgba(0,0,0,0.06); }
.row-main { display: flex; justify-content: space-between; align-items: center; }
.row-title { font-size: 14px; font-weight: 500; color: #1d1d1f; }
.row-status { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; }
.status-done { color: #16a34a; }
.status-failed { color: #dc2626; }
.status-transcribing { color: #4f46e5; }
.status-recording { color: #86868b; }
.row-sub { display: flex; justify-content: space-between; margin-top: 4px; font-size: 12px; color: #86868b; }
.row-controls { display: flex; align-items: center; gap: 8px; margin-top: 10px; flex-wrap: nowrap; }
.btn-note, .btn-transcript {
  font-size: 13px;
  padding: 5px 14px;
  border-radius: 6px;
  border: 1px solid #d1d5db;
  background: white;
  color: #1d1d1f;
  cursor: pointer;
  flex-shrink: 0;
  white-space: nowrap;
}
.btn-note:disabled, .btn-transcript:disabled { opacity: 0.5; cursor: not-allowed; }

/* Right panel: (empty) detail + record control */
.right-panel {
  flex: 1;
  min-width: 0;
  background: #ffffff;
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
}
.right-header {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  padding: 16px 20px;
  flex-shrink: 0;
}
.record-btn {
  width: 40px;
  height: 40px;
  border-radius: 50%;
  border: 1px solid #e5e5ea;
  background: #fff;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: background 0.15s, box-shadow 0.15s;
}
.record-btn:hover:not(:disabled) { background: #f5f5f7; box-shadow: 0 1px 4px rgba(0,0,0,0.08); }
.record-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.record-dot { width: 14px; height: 14px; border-radius: 50%; background: #f43f5e; }
.right-body { flex: 1; min-height: 0; }
</style>
