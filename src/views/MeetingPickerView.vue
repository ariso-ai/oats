<template>
  <div class="picker">
    <h2 class="title">{{ state === 'empty' ? 'New meeting' : 'Select a meeting' }}</h2>

    <div v-if="isChoosing" class="start-status" role="status" aria-live="polite">
      <span class="spinner" />
      <span>Starting recording…</span>
    </div>

    <div v-if="state === 'loading'" class="state-row">
      <span class="spinner" />
      <span>Loading meetings…</span>
    </div>

    <div v-else-if="state === 'error'" class="state-row">
      <span class="error-icon">!</span>
      <span>Could not load meetings.</span>
    </div>

    <template v-else-if="state === 'list'">
      <!-- Collapsed default: the forced "Continue" meeting (from the Library),
           else the heuristic pick. -->
      <template v-if="!showAll">
        <template v-if="forcedDefault">
          <p class="section-label">Continue meeting</p>
          <MeetingPickerRow
            :title="forcedDefault.title"
            :start-at="forcedDefault.start_at"
            featured
            :disabled="isChoosing"
            @choose="choose(forcedDefault.id)"
          />
        </template>
        <template v-else>
          <p v-if="defaultMeeting.kind !== 'none'" class="section-label">
            {{ featuredLabel }}
          </p>
          <MeetingPickerRow
            v-if="defaultMeeting.featured"
            :title="defaultMeeting.featured.title || ''"
            :start-at="defaultMeeting.featured.start_at"
            featured
            :live="isHappeningNow"
            :disabled="isChoosing"
            @choose="choose(defaultMeeting.featured.id)"
          />
          <p v-else class="section-label">No meeting happening now</p>
        </template>
      </template>

      <!-- Expanded: full flat list of today's meetings (shared by both cases).
           The recommended meeting keeps its primary treatment here so expanding
           adds options instead of flattening the pick into anonymous rows. -->
      <ul
        v-else
        ref="listEl"
        class="meeting-list"
        :class="{ 'is-faded-top': fadeTop, 'is-faded-bottom': fadeBottom }"
        @scroll="updateFades"
      >
        <li v-for="row in decoratedMeetings" :key="row.meeting.id">
          <MeetingPickerRow
            :title="row.meeting.title || ''"
            :start-at="row.meeting.start_at"
            :badge="row.badge"
            :live="row.live"
            :featured="row.featured"
            :disabled="isChoosing"
            @choose="choose(row.meeting.id)"
          />
        </li>
      </ul>

      <button class="link-btn" type="button" @click="toggleShowAll">
        {{ showAll ? 'View less ▴' : 'View all ▾' }}
      </button>
    </template>

    <div class="new-meeting" :class="{ 'new-meeting--divided': state === 'list' }">
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
          <button v-if="state !== 'empty'" class="btn btn-secondary" type="button" :disabled="isChoosing" @click="cancelNewMeeting">
            Cancel
          </button>
          <button class="btn btn-primary" :disabled="isChoosing" @click="startNewMeeting">
            Start recording
          </button>
        </div>
        <p v-if="createError" class="create-error">{{ createError }}</p>
      </template>
    </div>

    <AriJoinConfirmDialog
      :open="ariConfirm.open.value"
      @confirm="ariConfirm.confirm"
      @cancel="ariConfirm.cancel"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useMeetingApi, type ScheduledMeeting } from '../composables/useMeetingApi';
import { pickDefaultMeeting } from '../composables/pickDefaultMeeting';
import { parseDefaultMeetingId } from '../composables/parseDefaultMeetingId';
import { arisoTruthy, shouldConfirmAriJoin } from '../composables/autoJoin';
import { useAriJoinConfirm } from '../composables/useAriJoinConfirm';
import AriJoinConfirmDialog from './AriJoinConfirmDialog.vue';
import MeetingPickerRow from './MeetingPickerRow.vue';

type PickerState = 'loading' | 'list' | 'empty' | 'error';

const meetingApi = useMeetingApi();
const state = ref<PickerState>('loading');
const meetings = ref<ScheduledMeeting[]>([]);
const isChoosing = ref(false);
const ariConfirm = useAriJoinConfirm();
const showAll = ref(false);
const showTitlePrompt = ref(false);
const titleDraft = ref('');
const createError = ref<string | null>(null);
const titleInput = ref<HTMLInputElement | null>(null);
const listEl = ref<HTMLElement | null>(null);
const fadeTop = ref(false);
const fadeBottom = ref(false);
const now = new Date();

// A meeting the Library asked us to feature as the default choice (the meeting
// open in its detail pane). May be a PAST meeting not in today's list, so its
// title/time are resolved from getMeetingNotes when the list doesn't carry it.
const forcedDefault = ref<{ id: number; title: string; start_at: string } | null>(null);

const defaultMeeting = computed(() => pickDefaultMeeting(meetings.value, now));

function todayBoundsLocal(): { startDate: Date; endDate: Date } {
  const start = new Date();
  start.setHours(0, 0, 0, 0);
  const end = new Date();
  end.setHours(23, 59, 59, 999);
  return { startDate: start, endDate: end };
}

// The primary treatment belongs to exactly one meeting: the forced "Continue"
// meeting when the Library sent us one, else the heuristic pick. Everything
// else stays a quiet card.
const featuredId = computed(
  () => forcedDefault.value?.id ?? defaultMeeting.value.featured?.id ?? null
);

// Only the heuristic pick can be live — a forced Continue meeting is whatever
// the Library had open, which is often already over.
const isHappeningNow = computed(
  () => !forcedDefault.value && defaultMeeting.value.kind === 'current'
);

const featuredLabel = computed(() => {
  if (forcedDefault.value) return 'Continue';
  return isHappeningNow.value ? 'Happening now' : 'Up next';
});

// Expanded rows carry the label inline, since a section heading can't attach to
// one row in the middle of a list the way it does in the collapsed view.
const decoratedMeetings = computed(() =>
  meetings.value.map((m) => {
    const featured = m.id === featuredId.value;
    return {
      meeting: m,
      featured,
      badge: featured ? featuredLabel.value : null,
      live: featured && isHappeningNow.value,
    };
  })
);

// Only fade an edge the list actually scrolls past — an unconditional top fade
// dissolves the first meeting's title into the backdrop before you scroll.
function updateFades(): void {
  const el = listEl.value;
  if (!el) {
    fadeTop.value = false;
    fadeBottom.value = false;
    return;
  }
  const max = el.scrollHeight - el.clientHeight;
  fadeTop.value = el.scrollTop > 1;
  fadeBottom.value = max > 1 && el.scrollTop < max - 1;
}

async function toggleShowAll(): Promise<void> {
  showAll.value = !showAll.value;
  await nextTick();
  updateFades();
}

async function choose(meetingId: number | null): Promise<void> {
  if (isChoosing.value) return;
  const chosen = meetings.value.find((m) => m.id === meetingId);
  if (
    shouldConfirmAriJoin('ariso', arisoTruthy(chosen?.auto_join_scheduled)) &&
    !(await ariConfirm.requestConfirm())
  ) {
    return; // user chose Cancel — leave the picker open
  }
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
  // In the empty state the prompt IS the whole UI — there is nothing to cancel
  // back to, so Escape/Cancel must not collapse it into a dead-end.
  if (state.value === 'empty') return;
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
  } finally {
    isChoosing.value = false;
  }
}

onMounted(async () => {
  const forcedId = parseDefaultMeetingId(window.location.hash);
  try {
    const { startDate, endDate } = todayBoundsLocal();
    const result = await meetingApi.listScheduledMeetings(startDate, endDate);
    meetings.value = result;

    if (forcedId != null) {
      // Prefer the already-fetched today entry; else fetch the (possibly past)
      // meeting's title + start so we can feature it.
      const inList = result.find((m) => m.id === forcedId);
      if (inList) {
        forcedDefault.value = { id: inList.id, title: inList.title ?? '', start_at: inList.start_at };
      } else {
        try {
          const notes = await meetingApi.getMeetingNotes(forcedId);
          forcedDefault.value = { id: forcedId, title: notes.title ?? '', start_at: notes.start_at };
        } catch (e) {
          console.error('Failed to resolve default meeting for picker', e);
        }
      }
      // With a forced default we always show the list surface (never the
      // empty→prompt dead-end), so Continue + "Record a new meeting" both show.
      state.value = 'list';
      return;
    }

    if (result.length === 0) {
      // No meetings to choose from — go straight to creating one. The optional
      // title prompt becomes the whole UI instead of a dead-end message.
      state.value = 'empty';
      showTitlePrompt.value = true;
      await nextTick();
      titleInput.value?.focus();
    } else {
      state.value = 'list';
    }
  } catch (err) {
    console.error('Failed to load scheduled meetings:', err);
    if (forcedId != null) {
      // Still let the user continue the intended meeting even if the today-list
      // fetch failed.
      try {
        const notes = await meetingApi.getMeetingNotes(forcedId);
        forcedDefault.value = { id: forcedId, title: notes.title ?? '', start_at: notes.start_at };
        state.value = 'list';
        return;
      } catch (e) {
        console.error('Failed to resolve default meeting for picker', e);
      }
    }
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

.start-status {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 20px;
  margin: -8px 0 10px;
  color: #6f6f6f;
  font-size: 13px;
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

/* Scrollable list, with the Meetings-window fade applied only to an edge that
   is actually scrolled past (see updateFades). Horizontal padding leaves room
   for the rows' hover shadow and focus ring, which overflow-x would clip. */
.meeting-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  list-style: none;
  margin: 0;
  padding: 4px 6px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  --fade-top: 0px;
  --fade-bottom: 0px;
  -webkit-mask-image: linear-gradient(to bottom, transparent 0, #000 var(--fade-top), #000 calc(100% - var(--fade-bottom)), transparent 100%);
  mask-image: linear-gradient(to bottom, transparent 0, #000 var(--fade-top), #000 calc(100% - var(--fade-bottom)), transparent 100%);
}
.meeting-list.is-faded-top { --fade-top: 18px; }
.meeting-list.is-faded-bottom { --fade-bottom: 18px; }
.meeting-list::-webkit-scrollbar { width: 6px; }
.meeting-list::-webkit-scrollbar-thumb { background: #d6d6d6; border-radius: 3px; }

.skip-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.new-meeting {
  margin-top: 14px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

/* A hairline keeps "record something else" legible as the fallback path below
   the list, rather than competing with it. */
.new-meeting--divided {
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid #e7e5e2;
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

/* Secondary by design: the meetings above are the primary choice, so this keeps
   a target's shape but drops the fill and the raised shadow. */
.skip-btn {
  padding: 10px 14px;
  border-radius: 12px;
  border: 1px solid #e0dedb;
  background: transparent;
  color: #4a4a4a;
  font-family: inherit;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.12s, border-color 0.12s, color 0.12s;
}

.skip-btn:not(:disabled):hover {
  background: rgba(0, 0, 0, 0.03);
  border-color: #d6d6d6;
  color: #1c1c1c;
}

.skip-btn:focus-visible {
  outline: 2px solid #1c1c1c;
  outline-offset: 2px;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
