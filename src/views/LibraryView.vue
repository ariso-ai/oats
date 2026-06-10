<template>
  <div class="library">
    <aside v-if="leftPanelVisible" class="left-panel">
      <h1 class="title">Meetings</h1>
      <p v-if="loading" class="hint">Loading…</p>
      <p v-else-if="error" class="hint">{{ error }}</p>
      <p v-else-if="meetings.length === 0" class="hint">No meetings yet.</p>
      <ul v-else class="list">
          <!-- key is backend-scoped: the list always comes from a single backend at a time -->
        <li
          v-for="m in meetings"
          :key="m.id"
          class="recording-row"
          :class="{ selected: selectedItem?.id === m.id }"
          tabindex="0"
          role="button"
          :aria-pressed="selectedItem?.id === m.id"
          @click="selectMeeting(m)"
          @keydown.enter.prevent="selectMeeting(m)"
          @keydown.space.prevent="selectMeeting(m)"
        >
          <div class="row-main">
            <span class="row-title">{{ m.title }}</span>
            <span v-if="m.status" class="row-status" :class="`status-${m.status}`">{{ m.status }}</span>
          </div>
          <div class="row-sub">
            <span>{{ formatDate(m.timestamp) }}</span>
            <span v-if="m.durationSeconds != null">{{ formatDuration(m.durationSeconds) }}</span>
          </div>
          <!-- Note/transcript now open in the right-hand detail panel on row
               click; only the audio control remains here. Stop its clicks from
               also selecting the row. -->
          <div v-if="m.files && m.files.hasAudio" class="row-controls" @click.stop>
            <RecordingAudioPlayer :id="m.id" :has-audio="m.files.hasAudio" />
          </div>
        </li>
      </ul>
    </aside>

    <section class="right-panel">
      <div class="right-body">
        <MeetingDetailView v-if="selectedItem" :item="selectedItem" />
        <div v-else class="empty-detail">Select a meeting to view its notes.</div>
      </div>
    </section>

    <!-- Transparent drag region across the top. Holds the panel toggle (left,
         beside the traffic lights) and the record button (right). No background
         or border, so each panel's own color shows through the title bar. -->
    <div class="titlebar" data-tauri-drag-region>
      <button
        class="panel-toggle"
        :aria-pressed="leftPanelVisible"
        :title="leftPanelVisible ? 'Hide meetings list' : 'Show meetings list'"
        aria-label="Toggle meetings list"
        @click="toggleLeftPanel"
      >
        <!-- Panel visible: left column filled (click to hide). -->
        <svg v-if="leftPanelVisible" width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden="true">
          <rect x="1.75" y="2.75" width="14.5" height="12.5" rx="2.25" stroke="currentColor" stroke-width="1.5" />
          <rect x="2.5" y="3.5" width="4" height="11" rx="1" fill="currentColor" />
        </svg>
        <!-- Panel hidden: empty outline (click to show). -->
        <svg v-else width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden="true">
          <rect x="1.75" y="2.75" width="14.5" height="12.5" rx="2.25" stroke="currentColor" stroke-width="1.5" />
          <line x1="6.75" y1="3" x2="6.75" y2="15" stroke="currentColor" stroke-width="1.5" />
        </svg>
      </button>

      <button
        v-if="!recording"
        class="record-btn"
        aria-label="Start recording"
        @click="startRecording"
      >
        <span class="record-dot" />
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { getAllWebviewWindows } from '@tauri-apps/api/webviewWindow';
import { getActiveBackend, type MeetingListItem } from '../composables/useBackend';
import RecordingAudioPlayer from './RecordingAudioPlayer.vue';
import MeetingDetailView from './MeetingDetailView.vue';

const meetings = ref<MeetingListItem[]>([]);
const loading = ref(true);
const error = ref<string | null>(null);
const recording = ref(false);
const leftPanelVisible = ref(true);
const selectedItem = ref<MeetingListItem | null>(null);

function selectMeeting(m: MeetingListItem): void {
  selectedItem.value = m;
}

function toggleLeftPanel(): void {
  leftPanelVisible.value = !leftPanelVisible.value;
}

// Drive recording state through here so we only react to transitions: hide the
// meetings list when a recording begins and restore it when one ends, while
// leaving the toggle free to override the panel in between.
function setRecording(next: boolean): void {
  if (next && !recording.value) {
    leftPanelVisible.value = false;
  } else if (!next && recording.value) {
    leftPanelVisible.value = true;
  }
  recording.value = next;
}

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

// Recording runs in the separate "waveform" window; its presence is our
// recording signal. Used to hide the Record button while a recording is active.
async function refreshRecordingState(): Promise<void> {
  try {
    const wins = await getAllWebviewWindows();
    setRecording(wins.some((w) => w.label === 'waveform'));
  } catch (e) {
    console.error('Failed to read window state', e);
  }
}

// Open the floating recorder pill (its own always-on-top window) instead of an
// in-window dock. The window dedups itself if one is already open.
async function startRecording(): Promise<void> {
  try {
    await invoke('start_recording_window', {});
    setRecording(true); // hide the button + list immediately; refreshed on focus
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
  position: relative;
  font-family: -apple-system, system-ui, sans-serif;
  box-sizing: border-box;
}

/* Transparent title bar overlaid on the top of the panels, so it carries no
   color or border of its own — each panel's background shows through. It holds
   the panel toggle (left) and record button (right). padding-left clears the
   native traffic lights; padding-top nudges both buttons down ~2px so they
   share the traffic lights' line. */
.titlebar {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 28px;
  box-sizing: border-box;
  display: flex;
  align-items: center;
  padding: 3px 12px 0 78px;
  background: transparent;
}
.panel-toggle,
.record-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 22px;
  padding: 0;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: #86868b;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}
.panel-toggle:hover,
.record-btn:hover { background: #e5e5ea; color: #1d1d1f; }
.panel-toggle[aria-pressed='true'] { color: #1d1d1f; }
.record-btn { margin-left: auto; } /* push to the right end of the title bar */
.record-dot { width: 11px; height: 11px; border-radius: 50%; background: #f43f5e; }

/* Left panel: meetings list */
.left-panel {
  width: 300px;
  flex-shrink: 0;
  background: #f5f5f7;
  border-right: 1px solid #e5e5ea;
  padding: 40px 24px 24px; /* extra top room clears the title-bar overlay */
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
}
.title { font-size: 20px; font-weight: 700; margin-bottom: 16px; color: #1d1d1f; flex-shrink: 0; }
.hint { font-size: 14px; color: #86868b; }
.list { list-style: none; margin: 0; padding: 0 4px 0 0; display: flex; flex-direction: column; gap: 8px; flex: 1; min-height: 0; overflow-y: auto; }
.recording-row { background: #fff; border-radius: 10px; padding: 12px 14px; box-shadow: 0 1px 3px rgba(0,0,0,0.06); cursor: pointer; border: 1px solid transparent; transition: border-color 0.12s, box-shadow 0.12s; }
.recording-row:hover { box-shadow: 0 1px 6px rgba(0,0,0,0.10); }
.recording-row.selected { border-color: #6c63c0; box-shadow: 0 0 0 1px #6c63c0; }
.row-main { display: flex; justify-content: space-between; align-items: center; }
.row-title { font-size: 14px; font-weight: 500; color: #1d1d1f; }
.row-status { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; }
.status-done { color: #16a34a; }
.status-failed { color: #dc2626; }
.status-transcribing { color: #4f46e5; }
.status-recording { color: #86868b; }
.row-sub { display: flex; justify-content: space-between; margin-top: 4px; font-size: 12px; color: #86868b; }
.row-controls { display: flex; align-items: center; gap: 8px; margin-top: 10px; flex-wrap: nowrap; }

/* Right panel: meeting detail area */
.right-panel {
  flex: 1;
  min-width: 0;
  background: #ffffff;
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
}
/* padding-top clears the transparent title-bar overlay so the detail's stripe
   header (and the empty state) sit below the toggle/record row. */
.right-body { flex: 1; min-height: 0; padding-top: 28px; box-sizing: border-box; display: flex; flex-direction: column; }
.empty-detail { flex: 1; display: flex; align-items: center; justify-content: center; color: #86868b; font-size: 14px; }
</style>
