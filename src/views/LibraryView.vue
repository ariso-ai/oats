<template>
  <div class="library" :class="{ 'library--windows': isWindows }">
    <!-- macOS overlays this row beside the native traffic lights. Windows uses
         it as the complete titlebar, including native-equivalent controls. -->
    <div
      class="titlebar"
      :class="{ 'titlebar--windows': isWindows }"
      data-tauri-drag-region
      @dblclick.self="toggleLibraryWindowMaximize"
    >
      <div v-if="isWindows" class="titlebar-brand" data-tauri-drag-region>
        <img class="titlebar-logo" :src="oatsLogo" alt="" data-tauri-drag-region />
        <span class="titlebar-title" data-tauri-drag-region>Meetings</span>
      </div>
      <span v-if="isWindows" class="titlebar-divider" aria-hidden="true" />
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
      <!-- While a recording runs off-screen (its meeting isn't shown), the
           Start button becomes a "Recording" indicator that re-docks the strip
           when clicked. Otherwise it starts a recording, disabled while the
           strip is already on-screen so a second recording can't begin. -->
      <button
        v-if="recordingOffscreen"
        class="add-btn add-btn--recording"
        type="button"
        aria-label="Show current recording"
        title="Show current recording"
        @click="showRecordingMeeting"
      >
        <span class="rec-wave" aria-hidden="true">
          <span class="rec-wave-bar" />
          <span class="rec-wave-bar" />
          <span class="rec-wave-bar" />
          <span class="rec-wave-bar" />
        </span>
        <span class="add-btn-label">In recording</span>
      </button>
      <button
        v-else
        class="add-btn"
        :class="{ 'add-btn--starting': recordingStarting }"
        type="button"
        :disabled="startDisabled"
        :aria-label="recordingStarting ? 'Starting recording' : 'Start recording'"
        :title="recordingStarting ? 'Starting recording' : 'Start recording'"
        @click="startRecording"
      >
        <span v-if="recordingStarting" class="start-spinner" aria-hidden="true" />
        <svg v-else width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
        </svg>
        <span class="add-btn-label">
          {{ recordingStarting ? 'Starting recording…' : 'Start recording' }}
        </span>
      </button>
      <div v-if="isWindows" class="window-controls">
        <button
          class="window-control"
          type="button"
          aria-label="Minimize window"
          title="Minimize"
          @click="minimizeLibraryWindow"
        >
          <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M1.5 6.5h9" /></svg>
        </button>
        <button
          class="window-control"
          type="button"
          :aria-label="libraryWindowMaximized ? 'Restore window' : 'Maximize window'"
          :title="libraryWindowMaximized ? 'Restore' : 'Maximize'"
          @click="toggleLibraryWindowMaximize"
        >
          <svg v-if="libraryWindowMaximized" viewBox="0 0 12 12" aria-hidden="true">
            <path d="M3.5 3.5v-2h7v7h-2M1.5 3.5h7v7h-7z" />
          </svg>
          <svg v-else viewBox="0 0 12 12" aria-hidden="true">
            <rect x="1.5" y="1.5" width="9" height="9" />
          </svg>
        </button>
        <button
          class="window-control window-control--close"
          type="button"
          aria-label="Close window"
          title="Close"
          @click="closeLibraryWindow"
        >
          <svg viewBox="0 0 12 12" aria-hidden="true"><path d="m2 2 8 8M10 2 2 10" /></svg>
        </button>
      </div>
    </div>

    <div v-if="recordingStartError" class="recording-start-error" role="alert">
      <span>{{ recordingStartError }}</span>
      <button type="button" aria-label="Dismiss recording error" @click="recordingStartError = null">×</button>
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
            @click="selectMeeting(m, { userSelected: true })"
          >
            <span v-if="recordingActive && recordingMeetingId === m.id" class="mi-rec-dot" aria-hidden="true" />
            <span class="mi-head">
              <span class="mi-title" :class="{ 'mi-title--canceled': m.canceled }">{{ m.title }}</span>
              <span v-if="relLabel(m)" class="mi-rel" :class="{ 'mi-rel--now': isNextNow(m) }">{{ relLabel(m) }}</span>
            </span>
            <span class="mi-sub" :class="{ 'mi-sub--now': isNextNow(m) }">{{ subFor(m) }}</span>
          </button>
        </template>
        <p v-if="displayedSections.length === 0" class="hint">{{ emptyListHint }}</p>
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
          :now="now"
          @close="clearSelection"
          @title-updated="onTitleUpdated"
        />
        <UpNextCard
          v-else
          :meetings="displayMeetings"
          :now="now"
          @select="(m) => selectMeeting(m, { userSelected: true })"
          @start="startRecordingFor"
          @record="startRecording"
        />
      </div>
      <RecorderStrip
        :meeting-id="selectedItem?.id ?? null"
        @recording-change="recordingMeetingId = $event"
        @recording-active="recordingActive = $event"
        @recording-phase="onRecorderPhase"
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
    <AriJoinConfirmDialog
      :open="ariConfirm.open.value"
      @confirm="ariConfirm.confirm"
      @cancel="ariConfirm.cancel"
    />
    <RecordingStartChoiceDialog
      :open="recordingStartChoice.open.value"
      :meeting-title="recordingStartChoice.meetingTitle.value"
      @continue="recordingStartChoice.choose('continue')"
      @new="recordingStartChoice.choose('new')"
      @cancel="recordingStartChoice.cancel"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { getAllWebviewWindows } from '@tauri-apps/api/webviewWindow';
import { getActiveBackend, timestampTitle, BACKEND_CHANGED_EVENT, type Backend, type MeetingListItem } from '../composables/useBackend';
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
import { shouldConfirmAriJoin } from '../composables/autoJoin';
import { useAriJoinConfirm } from '../composables/useAriJoinConfirm';
import AriJoinConfirmDialog from './AriJoinConfirmDialog.vue';
import { decideStartRecording } from '../composables/decideStartRecording';
import { useRecordingStartChoice } from '../composables/useRecordingStartChoice';
import RecordingStartChoiceDialog from './RecordingStartChoiceDialog.vue';
import { recordingStartErrorMessage } from '../composables/recordingStartError';
import oatsLogo from '../assets/oats-dark.svg';

const meetings = ref<MeetingListItem[]>([]);
const loading = ref(true);
const error = ref<string | null>(null);
const recordingStartError = ref<string | null>(null);
const recording = ref(false);
type RecorderPhase = 'starting' | 'recording' | 'uploading' | 'success' | 'failed' | 'closed';
const recordingPhase = ref<RecorderPhase | null>(null);
// Once a structured waveform heartbeat arrives, that window becomes the
// lifecycle authority through upload/retry; native booleans only describe
// capture and must not unlock a second recorder underneath it.
const recorderOwnsLifecycle = ref(false);
const leftPanelVisible = ref(true);
const libraryWindowMaximized = ref(false);
const selectedItem = ref<MeetingListItem | null>(null);
// Tracks the row the user intentionally selected; auto-load and recorder-driven
// selections must not override Today's "record the live meeting" behavior.
const userSelectedMeetingId = ref<string | null>(null);
const ariConfirm = useAriJoinConfirm();
const recordingStartChoice = useRecordingStartChoice();
const activeBackend = ref<Backend | null>(null);
const searchPaletteOpen = ref(false);
type MeetingDetailViewExposed = InstanceType<typeof MeetingDetailView> & {
  saveNotesNow?: () => Promise<void>;
  openPrepTab?: () => void;
};
const detailView = ref<MeetingDetailViewExposed | null>(null);
const pendingUploads = ref<{ refresh: () => Promise<void> } | null>(null);
// Meeting the recording session belongs to (reported by the strip). Persists
// through the upload/failed phases so the row stays selected/pinned.
const recordingMeetingId = ref<string | null>(null);
// True only while audio is actively being captured — gates the red dot so it
// stops pulsing the moment recording ends (e.g. a lingering failed-upload pill).
const recordingActive = ref(false);
const recordingStarting = computed(
  () =>
    recording.value &&
    !recordingActive.value &&
    (recordingPhase.value === null || recordingPhase.value === 'starting'),
);

// The recorder strip is docked in the detail pane when an active recording's
// meeting is the one on-screen (a recording with no home row shows anywhere).
// Mirrors RecorderStrip's own visibility rule.
const recorderStripVisible = computed(
  () =>
    recordingActive.value &&
    (recordingMeetingId.value == null || recordingMeetingId.value === selectedItem.value?.id)
);
// A recording is running but its meeting isn't on-screen (the user closed the
// detail or navigated to another meeting). The titlebar then shows a
// "Recording" indicator instead of the Start button.
const recordingOffscreen = computed(() => recordingActive.value && !recorderStripVisible.value);
// A single waveform window owns capture, upload, and failed-upload recovery.
// Keep every launcher disabled for that window's full lifetime so a second
// click cannot race native creation or replace a recoverable failed session.
const startDisabled = computed(() => recording.value || recorderStripVisible.value);
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
const isWindows = computed(() =>
  typeof navigator !== 'undefined' && /Windows/i.test(navigator.userAgent)
);
const searchShortcutLabel = computed(() => (isMac.value ? '⌘K' : 'Ctrl K'));

const libraryWindow = getCurrentWindow();
async function syncLibraryWindowMaximized(): Promise<void> {
  libraryWindowMaximized.value = await libraryWindow.isMaximized();
}
async function minimizeLibraryWindow(): Promise<void> {
  await libraryWindow.minimize();
}
async function toggleLibraryWindowMaximize(): Promise<void> {
  if (!isWindows.value) return;
  await libraryWindow.toggleMaximize();
  await syncLibraryWindowMaximized();
}
async function closeLibraryWindow(): Promise<void> {
  await libraryWindow.close();
}

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

// The Meetings view is a history list — it stops at today, so it can come up
// empty even when the backend returned only future (scheduled) meetings.
const displayedSections = computed<MeetingSection[]>(() => {
  if (activeView.value === 'today') {
    return groupTodaysMeetings(displayMeetings.value, now.value);
  }
  return groupMeetingsByDate(displayMeetings.value, now.value);
});

const emptyListHint = computed(() =>
  activeView.value === 'today' ? 'No meetings today.' : 'No past meetings.'
);

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

async function selectMeeting(m: MeetingListItem, options: { userSelected?: boolean } = {}): Promise<void> {
  if (options.userSelected) userSelectedMeetingId.value = m.id;
  else if (userSelectedMeetingId.value !== m.id) userSelectedMeetingId.value = null;
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
  userSelectedMeetingId.value = null;
}

// Re-dock the recorder strip: pull the in-progress recording's meeting back
// into the detail pane (the titlebar "Recording" indicator's action).
async function showRecordingMeeting(): Promise<void> {
  const id = recordingMeetingId.value;
  if (!id) return;
  const m = displayMeetings.value.find((x) => x.id === id);
  if (m) await selectMeeting(m, { userSelected: false });
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
  await selectMeeting(meeting, { userSelected: true });
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

function markRecordingStarting(): void {
  recordingPhase.value = 'starting';
  setRecording(true);
}

function clearRecordingLaunch(): void {
  recorderOwnsLifecycle.value = false;
  recordingActive.value = false;
  recordingPhase.value = null;
  setRecording(false);
}

// Bump per call so an older in-flight `listMeetings()` can't clobber a newer
// reload (e.g. the recording://started fallback firing while the initial
// onMounted load is still pending).
let loadMeetingsRequest = 0;

// `autoSelectFirst` is only set for the initial mount load: opening the window
// lands on the first visible grouped row, not the backend's raw list order.
// Refresh-driven reloads (window focus/move, upload completion) pass false so
// they never yank the user off the Up Next greeting/card view back into detail.
async function loadMeetings(autoSelectFirst = false): Promise<void> {
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
    const firstVisible = displayedSections.value[0]?.items[0];
    if (autoSelectFirst && !selectedItem.value && firstVisible) {
      await selectMeeting(firstVisible, { userSelected: false });
    } else if (selectedItem.value) {
      const current = selectedItem.value;
      const fresh = meetings.value.find((m) => m.id === current.id);
      // A row can predate its prep (prep_id lands on a later list), so carry a
      // known prepId across the swap — losing it would drop the Prep tab from
      // the open meeting.
      selectedItem.value = !fresh
        ? current
        : current.prepId != null && fresh.prepId == null
          ? { ...fresh, prepId: current.prepId }
          : fresh;
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
  // A still-open waveform window may be sitting on a stale "Upload failed" pill
  // for the recording we just uploaded from the sidebar. Tell it to stand down
  // so its failed pill (mirrored into the recorder strip) clears and no Retry
  // can double-upload the buffer we already discarded.
  await emit('pending-upload://succeeded').catch((e) =>
    console.error('Failed to notify recorder of pending upload success', e),
  );
  await loadMeetings();
}

// Recording runs in the separate "waveform" window; its presence is our signal.
async function refreshRecordingState(): Promise<void> {
  try {
    const wins = await getAllWebviewWindows();
    const hasRecorder = wins.some((w) => w.label === 'waveform');
    if (hasRecorder) {
      if (!recordingActive.value && recordingPhase.value === null) {
        recordingPhase.value = 'starting';
      }
      setRecording(true);
    } else {
      clearRecordingLaunch();
    }
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
  recordingStartError.value = null;
  try {
    const backend = await getActiveBackend();
    if (
      shouldConfirmAriJoin(backend.id, item?.autoJoinScheduled) &&
      !(await ariConfirm.requestConfirm())
    ) {
      return; // user chose Cancel
    }
    // Ariso scheduled meetings use numeric backend ids; pass that id into the
    // recorder so the eventual upload attaches to the selected meeting.
    const meetingId = backend.id === 'ariso' ? numericMeetingId(item) : undefined;
    if (meetingId != null) {
      markRecordingStarting();
      await invoke('start_recording_window', { meetingId });
      return;
    }
    if (backend.usesMeetingPicker) {
      // Picker-using backends (Ariso) choose a meeting first; the picker then
      // starts the recorder itself.
      await invoke('open_meeting_picker', {});
      return;
    }
    markRecordingStarting();
    await invoke('start_recording_window', {});
  } catch (e) {
    clearRecordingLaunch();
    recordingStartError.value = recordingStartErrorMessage(e);
    console.error('Failed to start recording', e);
  }
}

// Open the floating recorder pill (its own always-on-top window). The button's
// behaviour follows the active nav view: Meetings always asks the picker; Today
// records the in-progress meeting (or a deliberately selected today meeting),
// falling back to the picker when neither applies. Non-picker (local) backends
// just open the recorder with no meeting attached.
async function startRecording(): Promise<void> {
  recordingStartError.value = null;
  try {
    const backend = await getActiveBackend();
    const usesPicker = backend.usesMeetingPicker;

    // Behavior keys on the detail pane, not on a deliberate selection: a meeting
    // shown (even auto-selected) keeps the "continue" affordance; an empty pane
    // starts a fresh recording that never attaches to a prior/now meeting.
    const shown = selectedItem.value;
    const plan = decideStartRecording({
      usesPicker,
      detailOpen: shown != null,
      shownMeeting: shown ? { numericId: numericMeetingId(shown) } : null,
    });

    if (plan.kind === 'ariso-picker') {
      // Ariso: always the picker; feature the shown meeting as default when
      // present. The command takes only the id — the picker resolves the title
      // from its fetched list (matches the pre-existing #206 wiring).
      const args = plan.defaultMeetingId != null ? { defaultMeetingId: plan.defaultMeetingId } : {};
      await invoke('open_meeting_picker', args);
      return;
    }

    if (plan.kind === 'local-new') {
      // Empty detail: force a brand-new recording, skipping the 5-minute auto-append.
      markRecordingStarting();
      await invoke('start_recording_window', { forceNew: true });
      return;
    }

    // plan.kind === 'local-continue': a meeting is shown — keep the 5-minute auto-append.
    markRecordingStarting();
    await invoke('start_recording_window', {});
  } catch (e) {
    clearRecordingLaunch();
    recordingStartError.value = recordingStartErrorMessage(e);
    console.error('Failed to start recording', e);
  }
}

// The Rust side announces every new recording (picker, tray, auto) with the
// meeting id it was started against. Collapse the sidebar right away and pull
// the picked meeting into the detail panel so the user sees what's recording.
async function onRecordingStarted(event: { payload: { meetingId: number | null } }): Promise<void> {
  markRecordingStarting();
  await selectRecordingMeeting(event.payload?.meetingId);
}

async function onRecorderPhase(phase: RecorderPhase | null): Promise<void> {
  recordingPhase.value = phase;
  if (phase === null) {
    // A missing heartbeat means the waveform died without its final `closed`
    // event; release the same lock that a clean shutdown would release.
    clearRecordingLaunch();
    return;
  }

  if (phase === 'closed') {
    const hadRecordedMeeting = recordingMeetingId.value !== null;
    clearRecordingLaunch();
    // RecorderStrip clears recordingMeetingId immediately after this callback;
    // its existing watcher owns the attached-meeting reload. Unattached Cloud
    // recordings have no id transition, so refresh them here instead.
    if (!hadRecordedMeeting) {
      await Promise.all([loadMeetings(), pendingUploads.value?.refresh()]);
    }
    return;
  }

  recorderOwnsLifecycle.value = true;

  const pendingWasMounted = leftPanelVisible.value;
  if (phase === 'uploading' || phase === 'failed' || phase === 'success') {
    // Capture is over, so bring the list back while the waveform window owns
    // save/retry state. Keep `recording` true to prevent a second recording;
    // remounting PendingUploads also performs a fresh initial read.
    leftPanelVisible.value = true;
    if (!pendingWasMounted) await nextTick();
  }

  if (phase === 'failed') {
    if (pendingWasMounted) await pendingUploads.value?.refresh();
  } else if (phase === 'success') {
    await Promise.all([
      loadMeetings(),
      pendingWasMounted ? pendingUploads.value?.refresh() : Promise.resolve(),
    ]);
  }
}

function onNativeRecordingState(event: { payload: unknown }): void {
  // Native creation emits a boolean before the waveform webview has mounted
  // and can send rich heartbeats. That early signal powers instant feedback for
  // tray and File-menu launches; object payloads are owned by RecorderStrip.
  if (typeof event.payload !== 'boolean') return;
  if (event.payload) {
    if (!recorderOwnsLifecycle.value) markRecordingStarting();
  } else if (!recorderOwnsLifecycle.value) {
    clearRecordingLaunch();
  }
}

function onRecordingStartFailed(event: { payload: unknown }): void {
  const payload = event.payload;
  recordingStartError.value =
    typeof payload === 'object'
    && payload !== null
    && 'message' in payload
    && typeof payload.message === 'string'
    && payload.message.length <= 300
      ? payload.message
      : recordingStartErrorMessage(payload);
  clearRecordingLaunch();
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
  if (m) await selectMeeting(m, { userSelected: false });
}

// A meeting-prep notification was clicked. The prep id is queued natively (the
// click usually *creates* this window, so an event payload would arrive before
// anything is listening) — claim it here, on mount and on the nudge event.
async function openPendingMeetingPrep(): Promise<void> {
  let prepId: number | null = null;
  try {
    prepId = await invoke<number | null>('take_pending_meeting_prep');
  } catch (e) {
    console.error('Failed to claim the pending meeting prep', e);
    return;
  }
  if (prepId == null) return;
  await openMeetingPrep(prepId);
}

// Surface the prep's meeting in the detail pane with its Prep tab active.
async function openMeetingPrep(prepId: number): Promise<void> {
  const byPrep = (): MeetingListItem | null =>
    displayMeetings.value.find((x) => x.prepId === prepId) ?? null;
  // The loaded list can predate the prep (rows carry prep_id), so reload before
  // giving up on a match.
  let m = byPrep();
  if (!m) {
    await loadMeetings();
    m = byPrep();
  }
  if (!m) {
    // Still nothing — the meeting may sit outside the loaded window. The prep
    // itself knows which meeting it belongs to.
    m = await meetingForPrep(prepId);
  }
  if (!m) {
    console.warn('Meeting prep', prepId, 'has no meeting row to open');
    return;
  }
  await selectMeeting(m, { userSelected: true });
  // selectMeeting is a no-op when that row is already selected — swap in the
  // resolved row anyway, so a prepId the selected row lacked reaches the pane.
  if (selectedItem.value !== m && selectedItem.value?.id === m.id) selectedItem.value = m;
  // The detail view mounts/loads asynchronously; it holds the request (keyed by
  // meeting id) until that meeting has loaded.
  await nextTick();
  detailView.value?.openPrepTab?.(m.id);
}

// Fallback resolver: ask the backend which meeting a prep belongs to, then
// produce a row for it — the loaded one when the list has it, otherwise a
// pinned row fetched by id (the meeting can sit outside the loaded window).
// Returns null when the prep or its meeting can't be resolved.
async function meetingForPrep(prepId: number): Promise<MeetingListItem | null> {
  try {
    const backend = activeBackend.value ?? (await getActiveBackend());
    const prep = await backend.getMeetingPrep(prepId);
    if (!prep?.meetingId) return null;
    const meetingId = prep.meetingId;
    const loaded = displayMeetings.value.find((x) => x.id === meetingId);
    // The row can predate the prep, so it may not carry prepId — without one
    // the detail pane renders no Prep tab. The notification knows better.
    if (loaded) return loaded.prepId === prepId ? loaded : { ...loaded, prepId };
    return await pinPrepMeeting(meetingId, prepId);
  } catch (e) {
    console.error('Failed to resolve the meeting for prep', prepId, e);
    return null;
  }
}

// Fetch a prep's meeting by id and keep it in the sidebar (like an ad-hoc
// recorded meeting) so a prep for a meeting outside the loaded window still has
// a row to select. Returns null when the meeting can't be fetched.
async function pinPrepMeeting(id: string, prepId: number): Promise<MeetingListItem | null> {
  try {
    const backend = activeBackend.value ?? (await getActiveBackend());
    const detail = await backend.getMeetingDetail({ id, title: '', timestamp: new Date().toISOString() });
    const row: MeetingListItem = {
      id,
      title: detail.title,
      timestamp: detail.startAt,
      endTimestamp: detail.endAt,
      prepId,
    };
    pinnedMeetings.value = new Map(pinnedMeetings.value).set(id, row);
    return row;
  } catch (e) {
    console.error('Failed to fetch the meeting for prep', prepId, e);
    return null;
  }
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
    if (m && selectedItem.value?.id !== id) await selectMeeting(m, { userSelected: false });
    return;
  }
  await loadMeetings();
  await pendingUploads.value?.refresh();
  if (prevId && selectedItem.value?.id === prevId && !displayMeetings.value.some((m) => m.id === prevId)) {
    // Discarded/crashed recording — its row is gone (and nothing pinned it);
    // fall back to the first available meeting.
    selectedItem.value = displayMeetings.value[0] ?? null;
    userSelectedMeetingId.value = null;
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
let unlistenRecordingState: UnlistenFn | null = null;
let unlistenRecordingStartFailed: UnlistenFn | null = null;
let unlistenRecordingReveal: UnlistenFn | null = null;
let unlistenVaultChanged: UnlistenFn | null = null;
let unlistenBackendChanged: UnlistenFn | null = null;
let unlistenPrepOpen: UnlistenFn | null = null;
let unlistenWindowResized: UnlistenFn | null = null;

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
  if (isWindows.value) {
    void syncLibraryWindowMaximized();
    void libraryWindow.onResized(() => {
      void syncLibraryWindowMaximized();
    }).then((un) => {
      unlistenWindowResized = un;
    });
  }
  void loadMeetings(true)
    .then(() => recoverActiveRecording())
    // A prep notification click that opened this window queued its prep before
    // any listener existed; claim it once the list is up.
    .then(() => openPendingMeetingPrep());
  void refreshRecordingState();
  void listen('recording://started', onRecordingStarted).then((un) => {
    unlistenRecordingStarted = un;
  });
  void listen('recording://state', onNativeRecordingState).then((un) => {
    unlistenRecordingState = un;
  });
  void listen('recording://start-failed', onRecordingStartFailed).then((un) => {
    unlistenRecordingStartFailed = un;
  });
  // The floating recorder pill asks (on click) to surface the meeting it's
  // recording — re-dock the strip even if the user had navigated away.
  void listen('recording://reveal', () => {
    void showRecordingMeeting();
  }).then((un) => {
    unlistenRecordingReveal = un;
  });
  // The local backend's vault directory can change from Settings; clear stale
  // selection state from the old vault before reloading the meeting list.
  void listen('vault://changed', () => {
    selectedItem.value = null;
    userSelectedMeetingId.value = null;
    pinnedMeetings.value = new Map();
    void loadMeetings(true);
  }).then((un) => {
    unlistenVaultChanged = un;
  });
  // Switching backends (in Settings) changes the whole meeting corpus. Close
  // any meeting held open from the previous backend — returning the detail to
  // the neutral Up Next state — and reload against the new backend.
  void listen(BACKEND_CHANGED_EVENT, () => {
    selectedItem.value = null;
    userSelectedMeetingId.value = null;
    pinnedMeetings.value = new Map();
    void loadMeetings();
  }).then((un) => {
    unlistenBackendChanged = un;
  });
  // Prep notification clicked while this window was already open.
  void listen('meeting-prep://open', () => {
    void openPendingMeetingPrep();
  }).then((un) => {
    unlistenPrepOpen = un;
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
  unlistenRecordingState?.();
  unlistenRecordingStartFailed?.();
  unlistenRecordingReveal?.();
  unlistenVaultChanged?.();
  unlistenBackendChanged?.();
  unlistenPrepOpen?.();
  unlistenWindowResized?.();
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

/* macOS transparent overlay; Windows promotes this row to full custom chrome. */
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
.recording-start-error {
  position: absolute;
  top: 34px;
  left: 50%;
  z-index: 20;
  display: flex;
  align-items: center;
  gap: 12px;
  max-width: min(520px, calc(100% - 32px));
  padding: 10px 12px;
  transform: translateX(-50%);
  border: 1px solid #e4aaa6;
  border-radius: 10px;
  background: #fff4f3;
  box-shadow: 0 6px 20px rgba(80, 20, 16, 0.12);
  color: #8f2722;
  font-size: 13px;
  line-height: 1.35;
}
.recording-start-error button {
  flex: 0 0 auto;
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  font: inherit;
  font-size: 18px;
  cursor: pointer;
}
.library--windows .recording-start-error {
  top: 46px;
}
.titlebar--windows {
  height: 40px;
  padding: 0;
  background: rgba(247, 246, 244, 0.98);
  border-bottom: 1px solid #e4e2de;
  box-shadow: 0 1px 0 rgba(255, 255, 255, 0.72) inset;
  user-select: none;
}
.titlebar-brand {
  height: 100%;
  display: flex;
  align-items: center;
  gap: 7px;
  padding-left: 12px;
  color: #363431;
}
.titlebar-logo {
  width: 18px;
  height: 18px;
  object-fit: contain;
}
.titlebar-title {
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.01em;
}
.titlebar-divider {
  width: 1px;
  height: 18px;
  margin-left: 11px;
  background: #d8d5d0;
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
.titlebar--windows .panel-toggle {
  width: 34px;
  height: 30px;
  margin-left: 5px;
  border-radius: 7px;
}
.titlebar--windows .add-btn {
  height: 26px;
  margin-right: 9px;
  border-radius: 13px;
}
.window-controls {
  align-self: stretch;
  display: flex;
}
.window-control {
  width: 46px;
  height: 40px;
  padding: 0;
  border: 0;
  background: transparent;
  color: #3d3b38;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: default;
  transition: background 80ms ease, color 80ms ease;
}
.window-control:hover { background: #e6e3df; }
.window-control--close:hover { background: #c42b1c; color: #ffffff; }
.titlebar--windows .panel-toggle:focus-visible,
.titlebar--windows .add-btn:focus-visible,
.window-control:focus-visible {
  outline: 2px solid #3b6fc4;
  outline-offset: -3px;
}
.window-control svg {
  width: 11px;
  height: 11px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1;
  shape-rendering: crispEdges;
}

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
.library--windows .sidebar { padding-top: 52px; }
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
.add-btn:disabled {
  opacity: 0.4;
  cursor: default;
}
.add-btn:disabled:hover { box-shadow: 1px 1px 0 #e7e5e2; transform: none; }
.add-btn--starting {
  min-width: 166px;
}
.start-spinner {
  width: 12px;
  height: 12px;
  flex: 0 0 auto;
  border: 2px solid #d6d6d6;
  border-top-color: #1c1c1c;
  border-radius: 50%;
  animation: start-spin 0.8s linear infinite;
}
@keyframes start-spin { to { transform: rotate(360deg); } }
/* Recording indicator: same pill, tinted red, pausing-glyph + "Recording". */
.add-btn--recording {
  color: #c5352f;
  border-color: #f0c5c3;
  background: #fdf3f2;
}
.add-btn--recording .add-btn-label { color: #c5352f; }
/* Mini live waveform: four bars pulsing on a staggered cycle. */
.rec-wave {
  display: inline-flex;
  align-items: center;
  gap: 1.5px;
  height: 12px;
}
.rec-wave-bar {
  width: 2px;
  height: 4px;
  border-radius: 1px;
  background: currentColor;
  animation: rec-wave 0.9s ease-in-out infinite;
}
.rec-wave-bar:nth-child(2) { animation-delay: 0.15s; }
.rec-wave-bar:nth-child(3) { animation-delay: 0.3s; }
.rec-wave-bar:nth-child(4) { animation-delay: 0.45s; }
@keyframes rec-wave {
  0%, 100% { height: 4px; }
  50% { height: 12px; }
}

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
/* Canceled meetings stay in the list (for context) but read as struck out,
   matching how the web app renders a canceled meeting's title. */
.mi-title--canceled {
  text-decoration: line-through;
  color: #8a8a8a;
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
.library--windows .detail-wrap { padding-top: 52px; }
.detail-card {
  flex: 1;
  min-height: 0;
  display: flex;
}
</style>
