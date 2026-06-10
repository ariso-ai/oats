<template>
  <div class="library">
    <!-- Transparent drag region across the top, holding the panel toggle next
         to the native traffic lights. -->
    <div class="titlebar" data-tauri-drag-region>
      <button
        class="panel-toggle"
        :aria-pressed="leftPanelVisible"
        :title="leftPanelVisible ? 'Hide meetings list' : 'Show meetings list'"
        aria-label="Toggle meetings list"
        @click="toggleLeftPanel"
      >
        <svg v-if="leftPanelVisible" width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden="true">
          <rect x="1.75" y="2.75" width="14.5" height="12.5" rx="2.25" stroke="currentColor" stroke-width="1.5" />
          <rect x="2.5" y="3.5" width="4" height="11" rx="1" fill="currentColor" />
        </svg>
        <svg v-else width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden="true">
          <rect x="1.75" y="2.75" width="14.5" height="12.5" rx="2.25" stroke="currentColor" stroke-width="1.5" />
          <line x1="6.75" y1="3" x2="6.75" y2="15" stroke="currentColor" stroke-width="1.5" />
        </svg>
      </button>
    </div>

    <aside v-if="leftPanelVisible" class="sidebar">
      <!-- Date header + new-recording button -->
      <header class="sidebar-head">
        <div class="date">
          <span class="date-day">{{ dayNum }}</span>
          <span class="date-month">{{ monthName }}</span>
        </div>
        <button class="add-btn" aria-label="Start recording" title="Start recording" @click="startRecording">
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
            <path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
          </svg>
        </button>
      </header>

      <p v-if="loading" class="hint">Loading…</p>
      <p v-else-if="error" class="hint">{{ error }}</p>
      <p v-else-if="meetings.length === 0" class="hint">No meetings yet.</p>

      <!-- Scrollable list with top/bottom fade mask -->
      <div v-else class="meeting-list">
        <template v-for="section in displayedSections" :key="section.key">
          <div v-if="section.label" class="group-label">{{ section.label }}</div>
          <button
            v-for="m in section.items"
            :key="m.id"
            class="meeting-item"
            :class="{ selected: selectedItem?.id === m.id }"
            :aria-pressed="selectedItem?.id === m.id"
            @click="selectMeeting(m)"
          >
            <span class="mi-title">{{ m.title }}</span>
            <span class="mi-sub">{{ itemSub(m) }}</span>
          </button>
        </template>
        <p v-if="displayedSections.length === 0" class="hint">No meetings today.</p>
      </div>

      <!-- Floating bottom navigation -->
      <nav class="bottom-nav">
        <div class="nav-pill">
          <button class="nav-tab" :class="{ 'nav-tab--active': activeView === 'today' }" type="button" title="Today" @click="activeView = 'today'">
            <svg viewBox="0 0 24 24" class="nav-ic"><path d="M3 10.5 12 4l9 6.5V20a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z" /></svg>
            <span>Today</span>
          </button>
          <button class="nav-tab" :class="{ 'nav-tab--active': activeView === 'meetings' }" type="button" title="Meetings" @click="activeView = 'meetings'">
            <svg viewBox="0 0 24 24" class="nav-ic"><path d="M4 6h10a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z" /><path d="m16 10 5-3v10l-5-3" /></svg>
            <span>Meetings</span>
          </button>
          <button class="nav-tab" type="button" title="Todo">
            <svg viewBox="0 0 24 24" class="nav-ic"><path d="M9 6h11M9 12h11M9 18h11" /><path d="m3 6 1.5 1.5L7 5M3 12l1.5 1.5L7 11M3 18l1.5 1.5L7 17" /></svg>
            <span>Todo</span>
          </button>
        </div>
      </nav>
    </aside>

    <!-- Floating detail card on the backdrop -->
    <section class="detail-wrap">
      <MeetingDetailView v-if="selectedItem" :item="selectedItem" @close="selectedItem = null" />
      <div v-else class="empty-card">
        <p>Select a meeting to view its notes.</p>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { getAllWebviewWindows } from '@tauri-apps/api/webviewWindow';
import { getActiveBackend, type MeetingListItem } from '../composables/useBackend';
import { groupMeetingsByDate, todaysMeetings, type MeetingSection } from '../composables/groupMeetingsByDate';
import MeetingDetailView from './MeetingDetailView.vue';

const meetings = ref<MeetingListItem[]>([]);
const loading = ref(true);
const error = ref<string | null>(null);
const recording = ref(false);
const leftPanelVisible = ref(true);
const selectedItem = ref<MeetingListItem | null>(null);

const now = new Date();
const dayNum = now.getDate();
const monthName = now.toLocaleString(undefined, { month: 'long' }).toUpperCase();

const activeView = ref<'today' | 'meetings'>('meetings');

const displayedSections = computed<MeetingSection[]>(() => {
  if (activeView.value === 'today') {
    const items = todaysMeetings(meetings.value, now);
    return items.length ? [{ key: 'today', label: '', items }] : [];
  }
  return groupMeetingsByDate(meetings.value, now);
});

function itemSub(m: MeetingListItem): string {
  const d = new Date(m.timestamp);
  const time = Number.isNaN(d.getTime())
    ? m.timestamp
    : d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
  if (m.durationSeconds != null) return `${time} • ${Math.max(1, Math.round(m.durationSeconds / 60))}min`;
  return time;
}

function selectMeeting(m: MeetingListItem): void {
  selectedItem.value = m;
}

function toggleLeftPanel(): void {
  leftPanelVisible.value = !leftPanelVisible.value;
}

async function openSettings(): Promise<void> {
  try {
    await invoke('create_settings_window', {});
  } catch (e) {
    console.error('Failed to open settings', e);
  }
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

// Recording runs in the separate "waveform" window; its presence is our signal.
async function refreshRecordingState(): Promise<void> {
  try {
    const wins = await getAllWebviewWindows();
    setRecording(wins.some((w) => w.label === 'waveform'));
  } catch (e) {
    console.error('Failed to read window state', e);
  }
}

// Open the floating recorder pill (its own always-on-top window).
async function startRecording(): Promise<void> {
  try {
    await invoke('start_recording_window', {});
    setRecording(true);
  } catch (e) {
    console.error('Failed to start recording window', e);
  }
}

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
  background: #f7f6f4; /* Backdrop/Primary */
  font-family: 'Polymath', -apple-system, system-ui, sans-serif;
  color: #1c1c1c;
  box-sizing: border-box;
}

/* Transparent title-bar overlay (panel toggle by the traffic lights). */
.titlebar {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 28px;
  z-index: 5;
  box-sizing: border-box;
  display: flex;
  align-items: center;
  padding: 3px 12px 0 78px;
  background: transparent;
}
.panel-toggle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 22px;
  padding: 0;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: #8a8a86;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}
.panel-toggle:hover { background: #ecebe8; color: #1c1c1c; }
.panel-toggle[aria-pressed='true'] { color: #1c1c1c; }

/* Sidebar */
.sidebar {
  width: 276px;
  flex-shrink: 0;
  padding: 40px 18px 16px;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.sidebar-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 6px 16px;
  flex-shrink: 0;
}
.date { display: flex; align-items: baseline; gap: 8px; }
.date-day { font-size: 20px; font-weight: 700; color: #1c1c1c; }
.date-month { font-size: 13px; font-weight: 500; letter-spacing: 2px; color: #1c1c1c; }
.add-btn {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  background: #ffffff;
  border: 1px solid #d6d6d6;
  box-shadow: 2px 2px 0 #e7e5e2;
  color: #1a1a1a;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: transform 0.1s, box-shadow 0.1s;
}
.add-btn:hover { box-shadow: 1px 1px 0 #e7e5e2; transform: translate(1px, 1px); }

.hint { font-size: 14px; color: #6f6f6f; padding: 0 6px; }

/* Meeting list with top/bottom fade so the first/last rows dissolve into the
   backdrop on scroll, matching the design. */
.meeting-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 6px;
  -webkit-mask-image: linear-gradient(to bottom, transparent 0, #000 24px, #000 calc(100% - 24px), transparent 100%);
  mask-image: linear-gradient(to bottom, transparent 0, #000 24px, #000 calc(100% - 24px), transparent 100%);
}
.meeting-list::-webkit-scrollbar { width: 6px; }
.meeting-list::-webkit-scrollbar-thumb { background: #d6d6d6; border-radius: 3px; }

.group-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 1.5px;
  color: #9a9a96;
  padding: 14px 10px 4px;
}

.meeting-item {
  display: flex;
  flex-direction: column;
  gap: 3px;
  text-align: left;
  width: 100%;
  padding: 10px 12px;
  border: 1px solid transparent;
  border-radius: 12px;
  background: transparent;
  cursor: pointer;
  transition: background 0.12s;
}
.meeting-item:hover { background: rgba(0, 0, 0, 0.03); }
.meeting-item.selected {
  background: #ffffff;
  border-color: #1c1c1c;
  box-shadow: 3px 3px 0 #e7e5e2;
}
.mi-title { font-size: 15px; font-weight: 500; color: #1c1c1c; line-height: 1.25; }
.mi-sub { font-size: 12px; color: #6f6f6f; }

/* Bottom nav */
.bottom-nav {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  padding-top: 12px;
}
.nav-pill,
.nav-circle {
  display: flex;
  align-items: center;
  gap: 4px;
  background: #ffffff;
  border: 1px solid #e5e6e3;
  border-radius: 999px;
  box-shadow: 2px 2px 0 #e7e5e2;
  padding: 5px;
}
.nav-tab {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 10px;
  border: none;
  border-radius: 999px;
  background: transparent;
  color: #6f6f6f;
  font-family: inherit;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
}
.nav-tab:hover { color: #1c1c1c; }
.nav-tab--active { background: #1c1c1c; color: #ffffff; }
.nav-tab--active:hover { color: #ffffff; }
.nav-icon-btn {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  border: none;
  background: transparent;
  color: #6f6f6f;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}
.nav-icon-btn:hover { color: #1c1c1c; }
.nav-ic { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; flex-shrink: 0; }

/* Detail card area */
.detail-wrap {
  flex: 1;
  min-width: 0;
  padding: 28px 24px 24px 8px;
  box-sizing: border-box;
  display: flex;
  min-height: 0;
}
.empty-card {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #ffffff;
  border: 1px solid #e5e6e3;
  border-radius: 16px;
  color: #6f6f6f;
  font-size: 14px;
}
</style>
