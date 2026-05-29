<template>
  <div class="picker">
    <h2 class="title">Select a meeting</h2>

    <div v-if="state === 'loading'" class="state-row">
      <span class="spinner" />
      <span>Loading meetings…</span>
    </div>

    <div v-else-if="state === 'error'" class="state-row">
      <span class="error-icon">!</span>
      <span>Could not load meetings.</span>
    </div>

    <div v-else-if="state === 'empty'" class="state-row">
      <span>No meetings today.</span>
    </div>

    <template v-else>
      <!-- Collapsed default: a single featured meeting (or a prompt) -->
      <template v-if="!showAll">
        <p v-if="defaultMeeting.kind !== 'none'" class="section-label">
          {{ defaultMeeting.kind === 'current' ? 'Happening now' : 'Up next' }}
        </p>
        <button
          v-if="defaultMeeting.featured"
          class="meeting-row"
          :disabled="isChoosing"
          @click="choose(defaultMeeting.featured.id)"
        >
          <span class="meeting-title">{{ defaultMeeting.featured.title || 'Untitled meeting' }}</span>
          <span class="meeting-time">{{ formatTime(defaultMeeting.featured.start_at) }}</span>
        </button>
        <p v-else class="section-label">No meeting happening now</p>
      </template>

      <!-- Expanded: full flat list of today's meetings -->
      <ul v-else class="meeting-list">
        <li v-for="m in meetings" :key="m.id">
          <button class="meeting-row" :disabled="isChoosing" @click="choose(m.id)">
            <span class="meeting-title">{{ m.title || 'Untitled meeting' }}</span>
            <span class="meeting-time">{{ formatTime(m.start_at) }}</span>
          </button>
        </li>
      </ul>

      <button class="link-btn" type="button" @click="showAll = !showAll">
        {{ showAll ? 'View less ▴' : 'View all ▾' }}
      </button>
    </template>

    <button class="skip-btn" :disabled="isChoosing" @click="choose(null)">
      Record without meeting
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useMeetingApi, type ScheduledMeeting } from '../composables/useMeetingApi';
import { pickDefaultMeeting } from '../composables/pickDefaultMeeting';

type PickerState = 'loading' | 'list' | 'empty' | 'error';

const meetingApi = useMeetingApi();
const state = ref<PickerState>('loading');
const meetings = ref<ScheduledMeeting[]>([]);
const isChoosing = ref(false);
const showAll = ref(false);
const now = new Date();

const defaultMeeting = computed(() => pickDefaultMeeting(meetings.value, now));

function todayBoundsLocal(): { startDate: Date; endDate: Date } {
  const start = new Date();
  start.setHours(0, 0, 0, 0);
  const end = new Date();
  end.setHours(23, 59, 59, 999);
  return { startDate: start, endDate: end };
}

function formatTime(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleTimeString(undefined, {
    hour: 'numeric',
    minute: '2-digit',
  });
}

async function choose(meetingId: number | null): Promise<void> {
  if (isChoosing.value) return;
  isChoosing.value = true;
  try {
    await invoke('start_recording_window', { meetingId });
  } catch (err) {
    console.error('Failed to start recording window:', err);
    isChoosing.value = false;
  }
}

onMounted(async () => {
  try {
    const { startDate, endDate } = todayBoundsLocal();
    const result = await meetingApi.listScheduledMeetings(startDate, endDate);
    if (result.length === 0) {
      state.value = 'empty';
    } else {
      meetings.value = result;
      state.value = 'list';
    }
  } catch (err) {
    console.error('Failed to load scheduled meetings:', err);
    state.value = 'error';
  }
});
</script>

<style scoped>
.picker {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: #0f0f1a;
  color: #e5e7eb;
  padding: 20px;
  box-sizing: border-box;
}

.title {
  font-size: 16px;
  font-weight: 600;
  margin: 0 0 16px;
}

.section-label {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: #9ca3af;
  margin: 0 0 8px;
}

.link-btn {
  align-self: flex-start;
  margin-top: 8px;
  padding: 0;
  border: none;
  background: none;
  color: #818cf8;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
}

.link-btn:hover {
  text-decoration: underline;
}

.state-row {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  color: #9ca3af;
  font-size: 13px;
}

.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid #4b5563;
  border-top-color: #818cf8;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

.error-icon {
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: #f87171;
  color: #0f0f1a;
  font-weight: 700;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 12px;
}

.meeting-list {
  flex: 1;
  overflow-y: auto;
  list-style: none;
  margin: 0;
  padding: 0;
}

.meeting-row {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 14px;
  border: none;
  background: #1e1e2e;
  color: #e5e7eb;
  border-radius: 8px;
  margin-bottom: 6px;
  cursor: pointer;
  font-size: 13px;
  text-align: left;
  transition: background 0.15s;
}

.meeting-row:hover {
  background: #2a2a3e;
}

.meeting-row:disabled,
.skip-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.meeting-title {
  font-weight: 500;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-right: 10px;
}

.meeting-time {
  color: #9ca3af;
  font-family: monospace;
  flex-shrink: 0;
}

.skip-btn {
  margin-top: 14px;
  padding: 10px 14px;
  border-radius: 8px;
  border: none;
  background: #1e1e2e;
  color: #d1d5db;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s;
}

.skip-btn:hover {
  background: #2a2a3e;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
