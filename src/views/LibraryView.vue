<template>
  <div class="library">
    <h1 class="title">Meetings</h1>
    <p v-if="loading" class="hint">Loading…</p>
    <p v-else-if="recordings.length === 0" class="hint">No recordings yet.</p>
    <ul v-else class="list">
      <li v-for="r in recordings" :key="r.id" class="recording-row">
        <div class="row-main">
          <span class="row-title">{{ r.title }}</span>
          <span class="row-status" :class="`status-${r.status}`">{{ r.status }}</span>
        </div>
        <div class="row-sub">
          <span>{{ formatDate(r.createdAt) }}</span>
          <span>{{ formatDuration(r.durationSeconds) }}</span>
        </div>
        <div class="row-controls">
          <RecordingAudioPlayer :id="r.id" :has-audio="r.hasAudio" />
          <button class="btn-note" :disabled="!r.hasNote" @click="openNote(r.id)">Note</button>
          <button class="btn-transcript" :disabled="!r.hasTranscript" @click="openTranscript(r.id)">Transcript</button>
        </div>
      </li>
    </ul>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { local, type RecordingSummary } from '../tauri';
import RecordingAudioPlayer from './RecordingAudioPlayer.vue';

const recordings = ref<RecordingSummary[]>([]);
const loading = ref(true);

function formatDuration(secs: number): string {
  const m = Math.floor(secs / 60).toString().padStart(2, '0');
  const s = Math.floor(secs % 60).toString().padStart(2, '0');
  return `${m}:${s}`;
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
}

async function openNote(id: string) {
  try {
    await local.openRecordingFile(id, 'note');
  } catch (e) {
    console.error('Failed to open note', e);
  }
}

async function openTranscript(id: string) {
  try {
    await local.openRecordingFile(id, 'transcript');
  } catch (e) {
    console.error('Failed to open transcript', e);
  }
}

onMounted(async () => {
  try {
    recordings.value = await local.listRecordings();
  } catch (e) {
    console.error('Failed to list recordings', e);
  } finally {
    loading.value = false;
  }
});
</script>

<style scoped>
.library {
  padding: 24px;
  font-family: -apple-system, system-ui, sans-serif;
  background: #f5f5f7;
  height: 100vh;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
}
.title { font-size: 20px; font-weight: 700; margin-bottom: 16px; color: #1d1d1f; flex-shrink: 0; }
.hint { font-size: 14px; color: #86868b; }
/* Scrolls vertically when the recordings overflow the window. min-height: 0 lets
   this flex child shrink below its content height so overflow-y can engage. */
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
/* nowrap keeps the player + both buttons on one line; the player flexes to fill
   the remaining width (see RecordingAudioPlayer .audio-el) while the buttons
   keep their size. */
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
.btn-note:disabled, .btn-transcript:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
