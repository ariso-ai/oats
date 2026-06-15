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

    <div class="new-meeting">
      <button
        v-if="!showTitlePrompt"
        class="skip-btn"
        :disabled="isChoosing"
        @click="openNewMeetingPrompt"
      >
        Record a new meeting
      </button>

      <template v-else>
        <input
          ref="titleInput"
          v-model="titleDraft"
          class="title-input"
          type="text"
          placeholder="Meeting title (optional)"
          :disabled="isChoosing"
          aria-label="Meeting title"
          @keydown.enter.prevent="startNewMeeting"
          @keydown.esc.prevent="cancelNewMeeting"
        />
        <div class="new-meeting-actions">
          <button class="btn btn-secondary" type="button" :disabled="isChoosing" @click="cancelNewMeeting">
            Cancel
          </button>
          <button class="btn btn-primary" :disabled="isChoosing" @click="startNewMeeting">
            Start recording
          </button>
        </div>
        <p v-if="createError" class="create-error">{{ createError }}</p>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useMeetingApi, type ScheduledMeeting } from '../composables/useMeetingApi';
import { pickDefaultMeeting } from '../composables/pickDefaultMeeting';

type PickerState = 'loading' | 'list' | 'empty' | 'error';

const meetingApi = useMeetingApi();
const state = ref<PickerState>('loading');
const meetings = ref<ScheduledMeeting[]>([]);
const isChoosing = ref(false);
const showAll = ref(false);
const showTitlePrompt = ref(false);
const titleDraft = ref('');
const createError = ref<string | null>(null);
const titleInput = ref<HTMLInputElement | null>(null);
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

async function openNewMeetingPrompt(): Promise<void> {
  createError.value = null;
  showTitlePrompt.value = true;
  await nextTick();
  titleInput.value?.focus();
}

function cancelNewMeeting(): void {
  showTitlePrompt.value = false;
  titleDraft.value = '';
  createError.value = null;
}

// Create a fresh meeting (current user as the only participant, set server-side)
// and open the recorder attached to it. Title is optional — an empty draft just
// leaves the meeting untitled.
async function startNewMeeting(): Promise<void> {
  if (isChoosing.value) return;
  isChoosing.value = true;
  createError.value = null;
  try {
    const { meetingId } = await meetingApi.createAudioMeeting(titleDraft.value);
    await invoke('start_recording_window', { meetingId });
  } catch (err) {
    console.error('Failed to start a new meeting:', err);
    createError.value =
      err instanceof Error ? err.message : 'Could not start the meeting.';
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
  background: #f7f6f4; /* Backdrop/Primary — matches the Meetings window */
  color: #1c1c1c;
  font-family: 'Polymath', -apple-system, system-ui, sans-serif;
  padding: 20px;
  box-sizing: border-box;
}

.title {
  font-size: 16px;
  font-weight: 600;
  color: #1c1c1c;
  margin: 0 0 16px;
}

.section-label {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 1.5px;
  color: #9a9a96;
  margin: 0 0 6px;
  padding: 0 2px;
}

.link-btn {
  align-self: flex-start;
  margin-top: 8px;
  padding: 0;
  border: none;
  background: none;
  color: #6f6f6f;
  font-family: inherit;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
}

.link-btn:hover {
  color: #1c1c1c;
  text-decoration: underline;
}

.state-row {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  color: #6f6f6f;
  font-size: 14px;
}

.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid #d6d6d6;
  border-top-color: #1c1c1c;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

.error-icon {
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: #d96a5a;
  color: #f7f6f4;
  font-weight: 700;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 12px;
}

/* Scrollable list with the same top/bottom fade as the Meetings window. */
.meeting-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  list-style: none;
  margin: 0;
  padding: 6px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  -webkit-mask-image: linear-gradient(to bottom, transparent 0, #000 24px, #000 calc(100% - 24px), transparent 100%);
  mask-image: linear-gradient(to bottom, transparent 0, #000 24px, #000 calc(100% - 24px), transparent 100%);
}
.meeting-list::-webkit-scrollbar { width: 6px; }
.meeting-list::-webkit-scrollbar-thumb { background: #d6d6d6; border-radius: 3px; }

.meeting-row {
  display: flex;
  flex-direction: column;
  gap: 3px;
  width: 100%;
  padding: 10px 12px;
  border: 1px solid transparent;
  border-radius: 12px;
  background: transparent;
  color: #1c1c1c;
  font-family: inherit;
  text-align: left;
  cursor: pointer;
  transition: background 0.12s;
}

.meeting-row:hover {
  background: rgba(0, 0, 0, 0.03);
}

.meeting-row:disabled,
.skip-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.meeting-title {
  font-size: 15px;
  font-weight: 500;
  color: #1c1c1c;
  line-height: 1.25;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.meeting-time {
  font-size: 12px;
  color: #6f6f6f;
}

.new-meeting {
  margin-top: 14px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.title-input {
  width: 100%;
  box-sizing: border-box;
  padding: 10px 12px;
  border-radius: 12px;
  border: 1px solid #d6d6d6;
  background: #ffffff;
  color: #1c1c1c;
  font-family: inherit;
  font-size: 14px;
}

.title-input:focus {
  outline: none;
  border-color: #9a9a96;
}

.new-meeting-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
}

.btn {
  padding: 9px 16px;
  border-radius: 12px;
  font-family: inherit;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: transform 0.1s, box-shadow 0.1s, background 0.12s;
}

.btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.btn-secondary {
  border: 1px solid #d6d6d6;
  background: #ffffff;
  color: #1c1c1c;
}

.btn-secondary:not(:disabled):hover {
  background: rgba(0, 0, 0, 0.03);
}

.btn-primary {
  border: 1px solid #1c1c1c;
  background: #1c1c1c;
  color: #f7f6f4;
  box-shadow: 2px 2px 0 #e7e5e2;
}

.btn-primary:not(:disabled):hover {
  box-shadow: 1px 1px 0 #e7e5e2;
  transform: translate(1px, 1px);
}

.create-error {
  margin: 0;
  font-size: 12px;
  color: #d96a5a;
}

.skip-btn {
  padding: 10px 14px;
  border-radius: 12px;
  border: 1px solid #d6d6d6;
  background: #ffffff;
  box-shadow: 2px 2px 0 #e7e5e2;
  color: #1c1c1c;
  font-family: inherit;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: transform 0.1s, box-shadow 0.1s;
}

.skip-btn:hover {
  box-shadow: 1px 1px 0 #e7e5e2;
  transform: translate(1px, 1px);
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
