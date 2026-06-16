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
      <button class="add-btn" aria-label="Start recording" title="Start recording" @click="startRecording">
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
        </svg>
        <span class="add-btn-label">Start recording</span>
      </button>
    </div>

    <aside v-if="leftPanelVisible" class="sidebar">

      <button
        v-if="activeBackend?.supportsSearch"
        class="search-trigger"
        type="button"
        aria-label="Search notes"
        @click="openSearchPalette"
      >
        <svg class="search-trigger-icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">
          <circle cx="11" cy="11" r="7" stroke="currentColor" stroke-width="2" />
          <path d="m16.5 16.5 4 4" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
        </svg>
        <span>Search</span>
        <kbd>{{ searchShortcutLabel }}</kbd>
      </button>

      <PendingUploads ref="pendingUploads" @uploaded="onPendingUploaded" />

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
            <span v-if="recordingActive && recordingMeetingId === m.id" class="mi-rec-dot" aria-hidden="true" />
            <span class="mi-head">
              <span class="mi-title">{{ m.title }}</span>
              <span v-if="relLabel(m)" class="mi-rel" :class="{ 'mi-rel--now': isNextNow(m) }">{{ relLabel(m) }}</span>
            </span>
            <span class="mi-sub" :class="{ 'mi-sub--now': isNextNow(m) }">{{ subFor(m) }}</span>
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
          <button class="nav-tab" type="button" title="Todo" disabled>
            <svg viewBox="0 0 24 24" class="nav-ic"><path d="M9 6h11M9 12h11M9 18h11" /><path d="m3 6 1.5 1.5L7 5M3 12l1.5 1.5L7 11M3 18l1.5 1.5L7 17" /></svg>
            <span>Todo</span>
          </button>
        </div>
      </nav>
    </aside>

    <!-- Floating detail card on the backdrop, with the recorder strip
         (mirroring an on-going recording) docked underneath. -->
    <section class="detail-wrap">
      <div class="detail-card">
        <MeetingDetailView
          v-if="selectedItem"
          ref="detailView"
          :item="selectedItem"
          @close="clearSelection"
          @title-updated="onTitleUpdated"
        />
        <UpNextCard
          v-else
          :meetings="displayMeetings"
          :now="now"
          @select="selectMeeting"
          @start="startRecordingFor"
          @record="startRecording"
        />
      </div>
      <RecorderStrip
        :meeting-id="selectedItem?.id ?? null"
        @recording-change="recordingMeetingId = $event"
        @recording-active="recordingActive = $event"
      />
    </section>

    <!-- Key by backend id so switching backends remounts the palette, discarding
         any query/results typed against the previous backend's corpus. -->
    <LibrarySearchPalette
      :key="activeBackend?.id"
      :open="searchPaletteOpen"
      :search-meetings="searchMeetings"
      @close="searchPaletteOpen = false"
      @go-to-notes="goHomeFromSearch"
      @select="onSearchResultSelected"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getAllWebviewWindows } from '@tauri-apps/api/webviewWindow';
import { getActiveBackend, timestampTitle, type Backend, type MeetingListItem } from '../composables/useBackend';
import { timestampFromLocalRecordingId } from '../composables/localRecordingId';
import {
  groupMeetingsByDate,
  groupTodaysMeetings,
  upcomingRelLabel,
  isMeetingInProgress,
  type MeetingSection,
} from '../composables/groupMeetingsByDate';
import MeetingDetailView from './MeetingDetailView.vue';
import UpNextCard from './UpNextCard.vue';
import LibrarySearchPalette from './LibrarySearchPalette.vue';
import RecorderStrip from './RecorderStrip.vue';
import PendingUploads from './PendingUploads.vue';
import { emitNotificationsSync } from '../composables/useMeetingNotifications';
import { decideRecordingAction } from '../composables/decideRecordingAction';

const meetings = ref<MeetingListItem[]>([]);
const loading = ref(true);
const error = ref<string | null>(null);
const recording = ref(false);
const leftPanelVisible = ref(true);
const selectedItem = ref<MeetingListItem | null>(null);
const activeBackend = ref<Backend | null>(null);
const searchPaletteOpen = ref(false);
type MeetingDetailViewExposed = InstanceType<typeof MeetingDetailView> & {
  saveNotesNow?: () => Promise<void>;
};
const detailView = ref<MeetingDetailViewExposed | null>(null);
const pendingUploads = ref<{ refresh: () => Promise<void> } | null>(null);
// Meeting the recording session belongs to (reported by the strip). Persists
// through the upload/failed phases so the row stays selected/pinned.
const recordingMeetingId = ref<string | null>(null);
// True only while audio is actively being captured — gates the red dot so it
// stops pulsing the moment recording ends (e.g. a lingering failed-upload pill).
const recordingActive = ref(false);
// Ad-hoc meetings we recorded this session that the backend list doesn't surface
// yet (e.g. "Record a new meeting" — created via /meeting-notes/audio, so it
// isn't a calendar-scheduled meeting and never appears in listMeetings()). We
// fetch their metadata and keep them in the sidebar during AND after recording,
// until a reload naturally includes them. Keyed by id.
const pinnedMeetings = ref<Map<string, MeetingListItem>>(new Map());

// A ticking "now" so relative labels ("in 20min" → "Now") and the upcoming/past
// split stay fresh while the window sits open.
const now = ref(new Date());
const dayNum = computed(() => now.value.getDate());
const monthName = computed(() => now.value.toLocaleString(undefined, { month: 'long' }).toUpperCase());

const activeView = ref<'today' | 'meetings'>('meetings');
const isMac = computed(() =>
  typeof navigator !== 'undefined' && navigator.platform.toUpperCase().includes('MAC')
);
const searchShortcutLabel = computed(() => (isMac.value ? '⌘K' : 'Ctrl K'));

// An in-progress local recording has no list row yet (the entry is created on
// finalize). Synthesize one under the id the finalized recording will use, so
// the red dot, selection, and the recorder strip have a home that survives the
// post-recording reload.
const displayMeetings = computed<MeetingListItem[]>(() => {
  // Layer pinned ad-hoc meetings the backend list hasn't caught up to on top of
  // the loaded list (deduped by id), so they survive the post-recording reload.
  const pinned = [...pinnedMeetings.value.values()].filter(
    (p) => !meetings.value.some((m) => m.id === p.id)
  );
  const base = pinned.length ? [...pinned, ...meetings.value] : meetings.value;

  const id = recordingMeetingId.value;
  if (!id || base.some((m) => m.id === id)) return base;
  const timestamp = timestampFromLocalRecordingId(id);
  if (!timestamp) return base; // an Ariso meeting outside the loaded window
  return [
    {
      id,
      title: timestampTitle(timestamp),
      timestamp,
      files: { hasAudio: false, hasNote: false, hasTranscript: false },
    },
    ...base,
  ];
});

const displayedSections = computed<MeetingSection[]>(() => {
  if (activeView.value === 'today') {
    return groupTodaysMeetings(displayMeetings.value, now.value);
  }
  return groupMeetingsByDate(displayMeetings.value, now.value);
});

// Only the next upcoming meeting (soonest, or the one in progress) carries a
// relative-time chip; it's the first item of the Today view's UPCOMING section.
// The Meetings view groups purely by date and has no UPCOMING section, so no
// chip shows there.
const nextUpcomingId = computed<string | null>(() => {
  const up = displayedSections.value.find((s) => s.key === 'upcoming');
  return up?.items[0]?.id ?? null;
});

function relLabel(m: MeetingListItem): string {
  return m.id === nextUpcomingId.value ? upcomingRelLabel(m, now.value) : '';
}

function isNextNow(m: MeetingListItem): boolean {
  return m.id === nextUpcomingId.value && isMeetingInProgress(m, now.value);
}

function fmtClock(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime())
    ? iso
    : d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
}

// The in-progress next meeting shows its start–end range (rendered green);
// every other row keeps the normal start-time subtitle.
function subFor(m: MeetingListItem): string {
  if (isNextNow(m) && m.endTimestamp) {
    return `${fmtClock(m.timestamp)} – ${fmtClock(m.endTimestamp)}`;
  }
  return itemSub(m);
}

function itemSub(m: MeetingListItem): string {
  const d = new Date(m.timestamp);
  const time = Number.isNaN(d.getTime())
    ? m.timestamp
    : d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
  if (m.durationSeconds != null) return `${time} • ${Math.max(1, Math.round(m.durationSeconds / 60))}min`;
  return time;
}

// Selection changes ask the detail pane to flush editable notes first, so a
// slow autosave from the previous meeting cannot land after the row changed.
let selectionReqId = 0;

async function selectMeeting(m: MeetingListItem): Promise<void> {
  if (selectedItem.value?.id === m.id) return;
  const my = ++selectionReqId;
  await detailView.value?.saveNotesNow?.();
  if (my !== selectionReqId) return;
  selectedItem.value = m;
}

async function clearSelection(): Promise<void> {
  const my = ++selectionReqId;
  await detailView.value?.saveNotesNow?.();
  if (my !== selectionReqId) return;
  selectedItem.value = null;
}

function openSearchPalette(): void {
  if (!activeBackend.value?.supportsSearch) return;
  searchPaletteOpen.value = true;
}

// The palette asks the active backend to search, but still returns normal
// Library rows so selection and detail loading stay on the existing path.
async function searchMeetings(query: string): Promise<MeetingListItem[]> {
  const backend = activeBackend.value ?? (await getActiveBackend());
  activeBackend.value = backend;
  if (!backend.supportsSearch) return [];
  return backend.searchMeetings(query);
}

async function onSearchResultSelected(meeting: MeetingListItem): Promise<void> {
  searchPaletteOpen.value = false;
  await selectMeeting(meeting);
}

// The palette's Home command returns the Library to its neutral detail state:
// close search, then clear the selected meeting after saving any edits.
async function goHomeFromSearch(): Promise<void> {
  searchPaletteOpen.value = false;
  await clearSelection();
}

// Keep the sidebar (and the selected reference) in sync after an inline rename
// in the detail panel, so the list label updates without a full reload.
function onTitleUpdated(payload: { id: string; title: string }): void {
  const m = meetings.value.find((x) => x.id === payload.id);
  if (m) m.title = payload.title;
  if (selectedItem.value?.id === payload.id) {
    selectedItem.value = { ...selectedItem.value, title: payload.title };
  }
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

// Bump per call so an older in-flight `listMeetings()` can't clobber a newer
// reload (e.g. the recording://started fallback firing while the initial
// onMounted load is still pending).
let loadMeetingsRequest = 0;

async function loadMeetings(): Promise<void> {
  const requestId = ++loadMeetingsRequest;
  loading.value = true;
  error.value = null;
  try {
    const backend = await getActiveBackend();
    activeBackend.value = backend;
    const next = await backend.listMeetings();
    if (requestId !== loadMeetingsRequest) return;
    meetings.value = next;
    // Drop any pinned ad-hoc meeting the backend list now surfaces on its own.
    if (pinnedMeetings.value.size) {
      const pruned = new Map(pinnedMeetings.value);
      for (const id of [...pruned.keys()]) {
        if (next.some((m) => m.id === id)) pruned.delete(id);
      }
      if (pruned.size !== pinnedMeetings.value.size) pinnedMeetings.value = pruned;
    }
    // The native tray owns its own meeting cache. When this visible list gets
    // fresh data, nudge Rust to re-fetch so the menu-bar title updates now.
    void emitNotificationsSync().catch((err) => {
      console.warn('Failed to sync tray after meeting list refresh', err);
    });
    if (!selectedItem.value && meetings.value.length > 0) {
      await selectMeeting(meetings.value[0]);
    } else if (selectedItem.value) {
      selectedItem.value =
        meetings.value.find((m) => m.id === selectedItem.value?.id) ?? selectedItem.value;
    }
  } catch (e) {
    if (requestId !== loadMeetingsRequest) return;
    console.error('Failed to list meetings', e);
    error.value = 'Could not load meetings.';
  } finally {
    if (requestId === loadMeetingsRequest) loading.value = false;
  }
}

async function onPendingUploaded(): Promise<void> {
  await loadMeetings();
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

// Ariso list rows carry ids as strings for shared rendering, while the recorder
// command accepts the backend's numeric meeting id.
function numericMeetingId(item: MeetingListItem | null): number | undefined {
  if (!item || !/^\d+$/.test(item.id)) return undefined;
  const id = Number(item.id);
  return Number.isSafeInteger(id) ? id : undefined;
}

// Open the floating recorder pill (its own always-on-top window) for a specific
// meeting — the featured meeting behind "Start Meeting Early" on the Up Next card.
async function startRecordingFor(item: MeetingListItem | null): Promise<void> {
  try {
    const backend = await getActiveBackend();
    // Ariso scheduled meetings use numeric backend ids; pass that id into the
    // recorder so the eventual upload attaches to the selected meeting.
    const meetingId = backend.id === 'ariso' ? numericMeetingId(item) : undefined;
    if (meetingId != null) {
      await invoke('start_recording_window', { meetingId });
      setRecording(true);
      return;
    }
    if (backend.usesMeetingPicker) {
      // Picker-using backends (Ariso) choose a meeting first; the picker then
      // starts the recorder itself.
      await invoke('open_meeting_picker', {});
      return;
    }
    await invoke('start_recording_window', {});
    setRecording(true);
  } catch (e) {
    console.error('Failed to start recording', e);
  }
}

// True when a meeting's start falls on the current local day.
function isTodayItem(m: MeetingListItem | null): boolean {
  if (!m) return false;
  const d = new Date(m.timestamp);
  if (Number.isNaN(d.getTime())) return false;
  const n = now.value;
  return (
    d.getFullYear() === n.getFullYear() &&
    d.getMonth() === n.getMonth() &&
    d.getDate() === n.getDate()
  );
}

// The meeting currently in progress (the one the Today list flags with the green
// "Now" chip): a today meeting with start <= now < end. Earliest start wins so it
// matches the chip.
function currentNowItem(): MeetingListItem | null {
  const live = displayMeetings.value.filter(
    (m) => isTodayItem(m) && isMeetingInProgress(m, now.value)
  );
  if (live.length === 0) return null;
  live.sort((a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime());
  return live[0];
}

// Open the floating recorder pill (its own always-on-top window). The button's
// behaviour follows the active nav view: Meetings always asks the picker; Today
// records the in-progress meeting (or a deliberately selected today meeting),
// falling back to the picker when neither applies. Non-picker (local) backends
// just open the recorder with no meeting attached.
async function startRecording(): Promise<void> {
  try {
    const backend = await getActiveBackend();
    const usesPicker = backend.usesMeetingPicker;
    const selectedTodayId =
      usesPicker && activeView.value === 'today' && isTodayItem(selectedItem.value)
        ? numericMeetingId(selectedItem.value)
        : undefined;
    const nowMeetingId = usesPicker ? numericMeetingId(currentNowItem()) : undefined;

    const action = decideRecordingAction({
      view: activeView.value,
      usesPicker,
      selectedTodayId: selectedTodayId ?? null,
      nowMeetingId: nowMeetingId ?? null,
    });

    if (action.kind === 'picker') {
      await invoke('open_meeting_picker', {});
      return;
    }
    if (action.kind === 'record') {
      await invoke('start_recording_window', { meetingId: action.meetingId });
      setRecording(true);
      return;
    }
    // record-adhoc: non-picker (local) backend with no meeting attached.
    await invoke('start_recording_window', {});
    setRecording(true);
  } catch (e) {
    console.error('Failed to start recording', e);
  }
}

// The Rust side announces every new recording (picker, tray, auto) with the
// meeting id it was started against. Collapse the sidebar right away and pull
// the picked meeting into the detail panel so the user sees what's recording.
async function onRecordingStarted(event: { payload: { meetingId: number | null } }): Promise<void> {
  setRecording(true);
  await selectRecordingMeeting(event.payload?.meetingId);
}

// Fetch an ad-hoc Ariso meeting's metadata and keep it in the sidebar until a
// reload includes it. Local recordings (timestamp-encoded ids) already get a
// synthetic row from displayMeetings, so only numeric Ariso ids are pinned.
async function pinRecordedMeeting(id: string): Promise<void> {
  if (!/^\d+$/.test(id)) return;
  if (meetings.value.some((m) => m.id === id) || pinnedMeetings.value.has(id)) return;
  try {
    const backend = await getActiveBackend();
    if (backend.id !== 'ariso') return;
    const detail = await backend.getMeetingDetail({ id, title: '', timestamp: new Date().toISOString() });
    pinnedMeetings.value = new Map(pinnedMeetings.value).set(id, {
      id,
      title: detail.title,
      timestamp: detail.startAt,
      endTimestamp: detail.endAt,
    });
  } catch (e) {
    console.error('Failed to pin recorded meeting', id, e);
  }
}

// Shared resolver for "the recording is attached to meeting X, surface it in
// the detail panel". Used by both the live `recording://started` event and the
// mount-time backend query that recovers state after the library was closed.
async function selectRecordingMeeting(id: number | null | undefined): Promise<void> {
  if (id == null) return;
  const idStr = String(id);
  let m = displayMeetings.value.find((x) => x.id === idStr);
  if (!m) {
    // The picker can start a meeting the library hasn't loaded yet — reload, and
    // pin it if it's an ad-hoc meeting the list still won't surface.
    await loadMeetings();
    await pinRecordedMeeting(idStr);
    m = displayMeetings.value.find((x) => x.id === idStr);
  }
  if (m) await selectMeeting(m);
}

// Keep the detail panel on the recorded meeting: surface its row when the
// strip reports a recording (the synthetic row for local, the scheduled row
// for Ariso), and reload when it ends so the finalized local recording
// replaces the synthetic row under the same id.
watch(recordingMeetingId, async (id, prevId) => {
  if (id) {
    // Ensure the recorded meeting has a sidebar row even when it's an ad-hoc
    // Ariso meeting the calendar list doesn't carry.
    await pinRecordedMeeting(id);
    const m = displayMeetings.value.find((x) => x.id === id);
    if (m && selectedItem.value?.id !== id) await selectMeeting(m);
    return;
  }
  await loadMeetings();
  await pendingUploads.value?.refresh();
  if (prevId && selectedItem.value?.id === prevId && !displayMeetings.value.some((m) => m.id === prevId)) {
    // Discarded/crashed recording — its row is gone (and nothing pinned it);
    // fall back to the first available meeting.
    selectedItem.value = displayMeetings.value[0] ?? null;
  }
});

function onWindowFocus(): void {
  now.value = new Date();
  void loadMeetings();
  void pendingUploads.value?.refresh();
  void refreshRecordingState();
}

// ⌘K mirrors the sidebar Search pill, gated on the active backend supporting
// search (both Ariso and local do — local filters its recordings by title).
function onGlobalKeydown(event: KeyboardEvent): void {
  const key = event.key.toLowerCase();
  const triggered = (isMac.value ? event.metaKey : event.ctrlKey) && key === 'k';
  if (!triggered || !activeBackend.value?.supportsSearch) return;
  event.preventDefault();
  searchPaletteOpen.value = true;
}

let clockTimer: number | undefined;
let unlistenRecordingStarted: UnlistenFn | null = null;

// Recover the attached meeting for a recording that started before this
// library window existed. The `recording://started` event is one-shot, so a
// window opened mid-recording would otherwise never see the selection.
async function recoverActiveRecording(): Promise<void> {
  try {
    const id = await invoke<number | null>('get_active_recording_meeting_id');
    if (id != null && selectedItem.value == null) {
      await selectRecordingMeeting(id);
    }
  } catch (e) {
    console.error('Failed to query active recording', e);
  }
}

onMounted(() => {
  void loadMeetings().then(() => recoverActiveRecording());
  void refreshRecordingState();
  void listen('recording://started', onRecordingStarted).then((un) => {
    unlistenRecordingStarted = un;
  });
  clockTimer = window.setInterval(() => {
    now.value = new Date();
  }, 30_000);
  window.addEventListener('focus', onWindowFocus);
  window.addEventListener('keydown', onGlobalKeydown);
});

onUnmounted(() => {
  if (clockTimer !== undefined) clearInterval(clockTimer);
  window.removeEventListener('focus', onWindowFocus);
  window.removeEventListener('keydown', onGlobalKeydown);
  unlistenRecordingStarted?.();
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
  padding: 3px 5px 0 78px;
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
  width: 300px;
  flex-shrink: 0;
  padding: 30px 18px 18px;
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
  margin-left: auto;
  height: 22px;
  padding: 0 9px 0 7px;
  gap: 5px;
  border-radius: 11px;
  background: #ffffff;
  border: 1px solid #d6d6d6;
  box-shadow: 1px 1px 0 #e7e5e2;
  color: #1a1a1a;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: transform 0.1s, box-shadow 0.1s;
}
.add-btn-label {
  font-size: 11px;
  font-weight: 600;
  line-height: 1;
  white-space: nowrap;
}
.add-btn:hover { box-shadow: 0 0 0 #e7e5e2; transform: translate(1px, 1px); }

.search-trigger {
  display: flex;
  align-items: center;
  gap: 9px;
  width: calc(100% - 12px);
  min-height: 42px;
  margin: 0 6px 10px;
  padding: 0 12px;
  border: 1px solid #d7d6d2;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.62);
  color: #76736e;
  font-family: inherit;
  font-size: 15px;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.12s, border-color 0.12s, color 0.12s;
}
.search-trigger:hover {
  border-color: #bdbbb6;
  background: #ffffff;
  color: #1c1c1c;
}
.search-trigger-icon {
  width: 18px;
  height: 18px;
  flex: 0 0 auto;
}
.search-trigger kbd {
  margin-left: auto;
  border: 0;
  background: transparent;
  color: #8f8c87;
  font-family: inherit;
  font-size: 14px;
  font-weight: 600;
}

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
  position: relative; /* anchors the recording dot to the row's corner */
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
/* Title hugs the left, rel-label pushed right. */
.mi-head { display: flex; align-items: baseline; gap: 8px; }
.mi-rel { margin-left: auto; }
/* Recording dot pinned to the row's top-right corner, out of the text flow so
   the title/rel-label layout is unaffected. */
.mi-rec-dot {
  position: absolute;
  top: 8px;
  right: 8px;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: #e0443e;
  animation: rec-pulse 1s infinite;
}
@keyframes rec-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}
.mi-title {
  font-size: 15px;
  font-weight: 500;
  color: #1c1c1c;
  line-height: 1.25;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.mi-rel { flex-shrink: 0; font-size: 11px; font-weight: 600; letter-spacing: 0.3px; color: #6f6f6f; }
.mi-rel--now { color: #2e8b4f; }
.mi-sub { font-size: 12px; color: #6f6f6f; }
.mi-sub--now { color: #2e8b4f; font-weight: 500; }

/* Bottom nav */
.bottom-nav {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  padding-top: 24px;
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
.nav-tab:disabled { opacity: 0.45; cursor: default; }
.nav-tab:disabled:hover { color: #6f6f6f; }
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

/* Detail card area: the recorder strip floats bottom-centered over the card
   while a recording is on-going (it positions against this wrapper). */
.detail-wrap {
  position: relative;
  flex: 1;
  min-width: 0;
  padding: 30px 18px 18px 8px;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.detail-card {
  flex: 1;
  min-height: 0;
  display: flex;
}
</style>
