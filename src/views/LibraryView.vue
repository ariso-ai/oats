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
      <!-- Which backend oats is on, named as in Settings, next to the sidebar
           toggle. Clicking it opens a menu to switch backend or open Settings;
           picking ariso.ai while signed out shows the sign-in box instead. -->
      <div
        v-if="accountPill"
        ref="accountPillWrap"
        class="account-pill-wrap"
        @keydown.escape="closeBackendPopover({ restoreFocus: true })"
      >
        <button
          ref="backendPillButton"
          type="button"
          class="account-pill"
          :title="backendPillTitle"
          aria-haspopup="menu"
          :aria-expanded="backendPopover !== null"
          @click="toggleBackendMenu"
          @keydown.down.prevent="openBackendMenu"
        >
          <span class="account-pill-label">{{ accountPill === 'local' ? 'Local' : 'ariso.ai' }}</span>
          <svg v-if="accountPill === 'local'" class="account-pill-icon" viewBox="0 0 24 24" aria-hidden="true">
            <rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
            <line x1="8" y1="21" x2="16" y2="21" />
            <line x1="12" y1="17" x2="12" y2="21" />
          </svg>
          <svg v-else class="account-pill-icon" viewBox="0 0 24 24" aria-hidden="true">
            <path d="M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z" />
          </svg>
        </button>
        <div v-if="backendPopover" class="backend-popover">
          <div
            v-if="backendPopover === 'menu'"
            ref="backendMenu"
            class="backend-menu"
            role="menu"
            aria-label="Backend"
            @keydown="onBackendMenuKeydown"
          >
            <button
              type="button"
              role="menuitemradio"
              class="backend-menu-item"
              :class="{
                'backend-menu-item--active': activeBackend?.id === 'ariso',
                'backend-menu-item--signed-out': !accountSignedIn,
              }"
              :aria-checked="activeBackend?.id === 'ariso'"
              :aria-label="accountSignedIn ? undefined : 'ariso.ai, not signed in'"
              :title="accountSignedIn ? undefined : 'Not signed in — click to sign in'"
              :disabled="recording"
              tabindex="-1"
              @click="chooseAriso"
            >
              <span>ariso.ai</span>
              <svg class="account-pill-icon" viewBox="0 0 24 24" aria-hidden="true">
                <path d="M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z" />
              </svg>
            </button>
            <button
              type="button"
              role="menuitemradio"
              class="backend-menu-item"
              :class="{ 'backend-menu-item--active': activeBackend?.id === 'local' }"
              :aria-checked="activeBackend?.id === 'local'"
              :disabled="recording"
              tabindex="-1"
              @click="chooseLocal"
            >
              <span>Local</span>
              <svg class="account-pill-icon" viewBox="0 0 24 24" aria-hidden="true">
                <rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
                <line x1="8" y1="21" x2="16" y2="21" />
                <line x1="12" y1="17" x2="12" y2="21" />
              </svg>
            </button>
            <p v-if="recording" class="backend-menu-hint">Backend can't be changed while recording.</p>
            <div class="backend-menu-sep" role="separator" />
            <button
              type="button"
              role="menuitem"
              class="backend-menu-item"
              tabindex="-1"
              @click="chooseSettings"
            >
              <span>Settings</span>
              <Cog6ToothIcon class="account-pill-icon backend-menu-gear" aria-hidden="true" />
            </button>
          </div>
          <div
            v-else
            class="sign-in-popover"
            role="dialog"
            aria-labelledby="sign-in-popover-title"
          >
            <p id="sign-in-popover-title" class="sign-in-popover-title">Sign in to ariso.ai</p>
            <SignInButtons
              ref="signInButtons"
              :signing-in-with="accountSigningInWith"
              :error-message="accountErrorMessage"
              @sign-in="account.signIn"
              @cancel="account.cancelSignIn"
            />
          </div>
        </div>
      </div>
      <!-- While a recording runs off-screen (its meeting isn't shown), the
           Start button becomes a "Recording" indicator that re-docks the strip
           when clicked. A recording attached to no meeting has nowhere to
           re-dock, so it renders as a plain badge rather than a button whose
           click would silently do nothing. Otherwise it starts a recording,
           disabled while the strip is already on-screen so a second recording
           can't begin. -->
      <component
        :is="recordingMeetingId ? 'button' : 'span'"
        v-if="recordingOffscreen"
        class="add-btn add-btn--recording"
        :class="{ 'add-btn--static': !recordingMeetingId }"
        :type="recordingMeetingId ? 'button' : undefined"
        :role="recordingMeetingId ? undefined : 'status'"
        :aria-label="recordingMeetingId ? 'Show current recording' : undefined"
        :title="recordingMeetingId ? 'Show current recording' : 'Recording in progress'"
        @click="showRecordingMeeting"
      >
        <span class="rec-wave" aria-hidden="true">
          <span class="rec-wave-bar" />
          <span class="rec-wave-bar" />
          <span class="rec-wave-bar" />
          <span class="rec-wave-bar" />
        </span>
        <span class="add-btn-label">In recording</span>
      </component>
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

      <!-- Todo: the user's open action items, grouped by day. Selecting a row
           opens the meeting it came from in the detail pane. -->
      <template v-if="activeView === 'todo'">
        <p v-if="todoLoading" class="hint">Loading…</p>
        <p v-else-if="todoError" class="hint">{{ todoError }}</p>
        <p v-else-if="todoSections.length === 0" class="hint">No action items.</p>
        <div v-else class="meeting-list">
          <template v-for="section in todoSections" :key="section.key">
            <div class="group-label">{{ section.label }}</div>
            <button
              v-for="row in section.rows"
              :key="row.key"
              class="meeting-item todo-item"
              :class="{ selected: selectedItem?.id === row.meeting.id }"
              :aria-pressed="selectedItem?.id === row.meeting.id"
              @click="selectMeeting(row.meeting, { userSelected: true })"
            >
              <span class="mi-head">
                <span class="mi-title">{{ row.text }}</span>
              </span>
              <span class="mi-sub">{{ todoSub(row) }}</span>
            </button>
          </template>
        </div>
      </template>

      <template v-else>
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
              <!-- While a recording is still being turned into a transcript/notes,
                   the row says so instead of showing its time/duration — that
                   line is the only place the sidebar can tell "still working" and
                   "this meeting will never have notes" apart. -->
              <span v-if="rowProcessingLabel(m)" class="mi-sub mi-sub--processing">
                <span class="mi-spinner" aria-hidden="true" />
                {{ rowProcessingLabel(m) }}
              </span>
              <span v-else class="mi-sub" :class="{ 'mi-sub--now': isNextNow(m) }">{{ subFor(m) }}</span>
            </button>
          </template>
          <p v-if="displayedSections.length === 0" class="hint">{{ emptyListHint }}</p>
        </div>
      </template>

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
          <button
            class="nav-tab"
            :class="{ 'nav-tab--active': activeView === 'todo' }"
            type="button"
            title="Todos"
            :disabled="!activeBackend?.supportsActionItems"
            @click="openTodoView"
          >
            <svg viewBox="0 0 24 24" class="nav-ic"><path d="M9 6h11M9 12h11M9 18h11" /><path d="m3 6 1.5 1.5L7 5M3 12l1.5 1.5L7 11M3 18l1.5 1.5L7 17" /></svg>
            <span>Todos</span>
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
          @content-ready="onContentReady"
          @tasks-changed="onTasksChanged"
          @deleted="onNoteDeleted"
        />
        <UpNextCard
          v-else
          :meetings="displayMeetings"
          :now="now"
          :org-name="orgName"
          :org-logo="orgLogo"
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
import {
  groupActionItemsByDay,
  type ActionItemEntry,
  type ActionItemRow,
} from '../composables/actionItemSections';
import MeetingDetailView from './MeetingDetailView.vue';
import UpNextCard from './UpNextCard.vue';
import LibrarySearchPalette from './LibrarySearchPalette.vue';
import RecorderStrip from './RecorderStrip.vue';
import PendingUploads from './PendingUploads.vue';
import { emitNotificationsSync } from '../composables/useMeetingNotifications';
import { useMeetingProcessing } from '../composables/useMeetingProcessing';
import { shouldConfirmAriJoin } from '../composables/autoJoin';
import { useAriJoinConfirm } from '../composables/useAriJoinConfirm';
import AriJoinConfirmDialog from './AriJoinConfirmDialog.vue';
import { decideStartRecording } from '../composables/decideStartRecording';
import { useRecordingStartChoice } from '../composables/useRecordingStartChoice';
import RecordingStartChoiceDialog from './RecordingStartChoiceDialog.vue';
import {
  recordingBlockedMessage,
  recordingBlockedPayload,
  recordingStartErrorMessage,
} from '../composables/recordingStartError';
import { useAccountState } from '../composables/useAccountState';
import { useOrganizationInfo } from '../composables/useOrganizationInfo';
import { AUTH_CHANGED_EVENT, local, setBackendSetting } from '../tauri';
import { Cog6ToothIcon } from '@heroicons/vue/24/outline';
import SignInButtons from './SignInButtons.vue';
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
// Cloud meetings uploaded in this session that the server hasn't produced a
// transcript or notes for yet. Local recordings carry their own on-disk state on
// the list row instead, so they never enter this set.
const processingMeetings = useMeetingProcessing();
const PROCESSING_LABEL = 'Processing…';
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
// meeting is the one on-screen. A recording attached to no meeting (an Ariso
// auto-recording that matched no calendar event; a local one still resolving
// its id) belongs to no row, so it never docks — a pill above a meeting reads
// as "this meeting is being recorded" (#318). Mirrors RecorderStrip's own
// visibility rule; the two must agree, or the docked strip and the titlebar
// indicator both show (or neither does).
const recorderStripVisible = computed(
  () =>
    recordingActive.value &&
    recordingMeetingId.value != null &&
    recordingMeetingId.value === selectedItem.value?.id
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

const activeView = ref<'today' | 'meetings' | 'todo'>('meetings');
const todoEntries = ref<ActionItemEntry[]>([]);
const todoLoading = ref(false);
const todoError = ref<string | null>(null);
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

const todoSections = computed(() => groupActionItemsByDay(todoEntries.value, now.value));

// Which meeting an action item came from, so a row reads on its own.
function todoSub(row: ActionItemRow): string {
  return `${row.meeting.title} · ${fmtClock(row.meeting.timestamp)}`;
}

async function openTodoView(): Promise<void> {
  activeView.value = 'todo';
  await loadActionItems();
}

// Bump per call so a slow load can't overwrite a newer one.
let loadActionItemsRequest = 0;

// The backend owns its own window (Ariso: the last two weeks; local: the whole
// vault). It rejects only when nothing at all could be loaded. A `silent`
// refresh keeps the current rows on screen instead of swapping in "Loading…",
// supersedes any load in flight (which may predate the change being picked
// up), and keeps the rows it has if the refresh fails.
async function loadActionItems({ silent = false } = {}): Promise<void> {
  if (todoLoading.value && !silent) return;
  const backend = activeBackend.value ?? (await getActiveBackend());
  activeBackend.value = backend;
  if (!backend.supportsActionItems) return;
  const requestId = ++loadActionItemsRequest;
  if (!silent) {
    todoLoading.value = true;
    todoError.value = null;
  }
  try {
    const loaded = await backend.listActionItems();
    if (requestId !== loadActionItemsRequest) return;
    todoEntries.value = loaded;
    todoError.value = null;
  } catch (e) {
    if (requestId !== loadActionItemsRequest) return;
    console.error('Failed to load action items', e);
    if (!silent) {
      todoEntries.value = [];
      todoError.value = 'Could not load action items.';
    }
  } finally {
    if (requestId === loadActionItemsRequest) todoLoading.value = false;
  }
}

// A task ticked in the open meeting's AI notes closes its todo; drop it from
// the list in place. Outside the Todo tab, opening the tab reloads anyway.
function onTasksChanged(): void {
  if (activeView.value === 'todo') void loadActionItems({ silent: true });
}

// Switching backends swaps the whole corpus: drop the previous backend's action
// items, and leave the Todo tab when the new backend has none (offline mode),
// where the tab is disabled and would otherwise stay highlighted over an
// empty pane. A load still in flight belongs to the old backend: invalidate it
// so it can't land its items here, and so the reload below isn't skipped.
watch(
  () => activeBackend.value?.id,
  () => {
    loadActionItemsRequest++;
    todoLoading.value = false;
    todoEntries.value = [];
    todoError.value = null;
    if (activeView.value === 'todo' && !activeBackend.value?.supportsActionItems) {
      activeView.value = 'meetings';
    } else if (activeView.value === 'todo') {
      void loadActionItems();
    }
  }
);

const emptyListHint = computed(() =>
  activeView.value === 'today' ? 'No meetings today.' : 'No past meetings.'
);

// Titlebar backend indicator. This window's own account state, kept current by
// the backend's AUTH_CHANGED_EVENT broadcast. It is only ever refreshed on
// Ariso: Local mode makes no session or profile request from this window.
const account = useAccountState();
const {
  isSignedIn: accountSignedIn,
  checked: accountChecked,
  email: accountEmail,
  signingInWith: accountSigningInWith,
  errorMessage: accountErrorMessage,
} = account;
type AccountPill = 'local' | 'checking' | 'signed-in' | 'signed-out';
const accountPill = computed<AccountPill | null>(() => {
  const id = activeBackend.value?.id;
  if (id === 'local') return 'local';
  if (id !== 'ariso') return null; // backend not loaded yet
  if (!accountChecked.value) return 'checking';
  return accountSignedIn.value ? 'signed-in' : 'signed-out';
});
const backendPillTitle = computed(() => {
  switch (accountPill.value) {
    case 'local':
      return 'Local mode — recordings stay on this device';
    case 'signed-in':
      return accountEmail.value ? `ariso.ai — ${accountEmail.value}` : 'ariso.ai';
    case 'signed-out':
      return 'ariso.ai — not signed in';
    default:
      return 'ariso.ai';
  }
});

// The pill's popover holds the backend menu, or the sign-in box once ariso.ai
// is picked without a session.
const backendPopover = ref<'menu' | 'sign-in' | null>(null);
const accountPillWrap = ref<HTMLElement | null>(null);
const backendPillButton = ref<HTMLButtonElement | null>(null);
const backendMenu = ref<HTMLElement | null>(null);
const signInButtons = ref<{ focus: () => void } | null>(null);
let switchingBackend = false;

function backendMenuItems(): HTMLElement[] {
  return [
    ...(backendMenu.value?.querySelectorAll<HTMLElement>('[role^="menuitem"]:not(:disabled)') ?? []),
  ];
}

function focusBackendMenuItem(idx: number): void {
  const items = backendMenuItems();
  if (items.length === 0) return;
  items[((idx % items.length) + items.length) % items.length].focus();
}

function onBackendMenuKeydown(event: KeyboardEvent): void {
  const items = backendMenuItems();
  const current = items.indexOf(document.activeElement as HTMLElement);
  const moves: Record<string, number> = {
    ArrowDown: current + 1,
    ArrowUp: current - 1,
    Home: 0,
    End: items.length - 1,
  };
  if (!(event.key in moves)) return;
  event.preventDefault();
  focusBackendMenuItem(moves[event.key]);
}

async function openBackendMenu(): Promise<void> {
  backendPopover.value = 'menu';
  await nextTick();
  // Land on the active backend, as Settings' backend picker does.
  const items = backendMenuItems();
  (items.find((el) => el.getAttribute('aria-checked') === 'true') ?? items[0])?.focus();
}

function toggleBackendMenu(): void {
  if (backendPopover.value) closeBackendPopover();
  else void openBackendMenu();
}

function closeBackendPopover({ restoreFocus = false } = {}): void {
  if (!backendPopover.value) return;
  backendPopover.value = null;
  if (restoreFocus) backendPillButton.value?.focus();
}

async function showSignInBox(): Promise<void> {
  backendPopover.value = 'sign-in';
  await nextTick();
  signInButtons.value?.focus();
}

// Tags this window's own backend-changed broadcasts, which it has already
// applied by the time they come back to it.
const backendSwitchSource = `library-${Math.random().toString(36).slice(2)}`;

// The switch Settings' backend picker makes: persist it, then tell every
// window and the native orchestrators (tray next-meeting, notifications).
// Refused while a recording owns the recorder, as in Settings.
async function switchBackend(next: 'ariso' | 'local'): Promise<boolean> {
  if (recording.value || switchingBackend) return false;
  switchingBackend = true;
  try {
    await setBackendSetting(next);
  } catch (e) {
    console.error('Failed to switch backend', e);
    return false;
  } finally {
    switchingBackend = false;
  }
  void emit(BACKEND_CHANGED_EVENT, { source: backendSwitchSource }).catch((err) => {
    console.warn('Failed to broadcast backend change', err);
  });
  void emitNotificationsSync().catch((err) => {
    console.warn('Failed to broadcast sync after backend change', err);
  });
  await onBackendChanged();
  return true;
}

// ariso.ai: switch to it if needed, then sign in if there's no session.
async function chooseAriso(): Promise<void> {
  if (activeBackend.value?.id !== 'ariso' && !(await switchBackend('ariso'))) return;
  if (!accountChecked.value) await account.refresh();
  if (activeBackend.value?.id !== 'ariso') return; // switched away meanwhile
  if (accountSignedIn.value) closeBackendPopover({ restoreFocus: true });
  else await showSignInBox();
}

async function chooseLocal(): Promise<void> {
  closeBackendPopover({ restoreFocus: true });
  if (activeBackend.value?.id === 'local' || !(await switchBackend('local'))) return;
  // Settings owns the on-device models: their first-time download confirmation
  // and progress. Its backend-changed listener starts that flow; bring it up
  // when there's something to see.
  try {
    const status = await local.modelStatus();
    if (status.state !== 'ready' || status.llmReady !== true) await openSettings();
  } catch (e) {
    console.warn('Failed to read on-device model status', e);
    await openSettings();
  }
}

async function chooseSettings(): Promise<void> {
  closeBackendPopover();
  await openSettings();
}

// A click elsewhere dismisses the popover, except while a browser flow is
// pending: its Cancel button lives there.
function onDocumentMousedown(event: MouseEvent): void {
  if (!backendPopover.value || accountSigningInWith.value) return;
  if (accountPillWrap.value?.contains(event.target as Node)) return;
  closeBackendPopover();
}

// However the session arrived (the sign-in box, Settings, Onboarding, a tray
// row), the sign-in box has nothing left to offer.
watch(accountSignedIn, (signedIn) => {
  if (signedIn && backendPopover.value === 'sign-in') closeBackendPopover();
});

// Covers both the first load on mount and every backend switch, since each
// re-reads the active backend. On Local, forget the account instead of asking,
// so a later switch back doesn't flash the stale state.
watch(
  () => activeBackend.value?.id,
  (id) => {
    if (id === 'ariso') {
      void account.refresh();
    } else if (id === 'local') {
      if (backendPopover.value === 'sign-in') closeBackendPopover();
      if (accountSigningInWith.value) void account.cancelSignIn();
      account.reset();
    }
  }
);

// The signed-in user's Ariso org brands the Up Next greeting. Fetched only on
// Ariso with a session — Local mode never asks — and refetched when the account
// changes (the email tells one user from the next).
const organization = useOrganizationInfo();
const { name: orgName, logo: orgLogo } = organization;
watch(
  () => [activeBackend.value?.id, accountSignedIn.value, accountEmail.value] as const,
  ([id, signedIn]) => {
    organization.reset();
    if (id === 'ariso' && signedIn) void organization.refresh();
  }
);

// Switching backends changes the whole meeting corpus. Close any meeting held
// open from the previous backend — returning the detail to the neutral Up Next
// state — and reload against the new backend.
async function onBackendChanged(): Promise<void> {
  selectedItem.value = null;
  userSelectedMeetingId.value = null;
  pinnedMeetings.value = new Map();
  // The tracked ids are Ariso's; keeping them alive would keep polling its
  // API from offline mode and re-show "Processing…" on a later switch back.
  processingMeetings.reset();
  await loadMeetings();
}

// The organization follows the account watcher above, which resets it on any
// change of account. Resetting here too would blank it for good whenever the
// session changes without the account changing.
function onAuthChanged(): void {
  if (activeBackend.value?.id === 'ariso') void account.refresh(true);
}

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

// A recording whose transcript/notes are still being produced. Local reads the
// on-disk pipeline state the list row already carries; cloud has no such signal,
// so it falls back to this session's upload tracking (see useMeetingProcessing).
// Returns the label to show, or null when the row's normal sub-line applies.
function rowProcessingLabel(m: MeetingListItem): string | null {
  if (activeBackend.value?.id === 'local') {
    // `recording`/`transcribing` are authoritative live states from disk.
    // `failed` is deliberately excluded — the failure is reported by the detail
    // panel's chip with a Retry, and a row that kept spinning forever would be
    // a lie.
    if (m.status === 'recording' || m.status === 'transcribing') return PROCESSING_LABEL;
    // "Has a transcript but no note" is an *inference* that notes are still
    // generating. It holds only while the notes are still pending: a recording
    // that settled without a note (nothing was said, or generation failed) is
    // done, and its detail panel names why. Even pending can't tell a running
    // pipeline from one that died mid-generation, so bound it: generation runs
    // for minutes, and an old recording with no note is stuck, not busy.
    if (
      m.status === 'done' &&
      m.files?.hasTranscript &&
      !m.files.hasNote &&
      (m.files.notesStatus ?? 'pending') === 'pending'
    ) {
      return finishedRecently(m) ? PROCESSING_LABEL : null;
    }
    return null;
  }
  return processingMeetings.isProcessing(m.id) ? PROCESSING_LABEL : null;
}

// How long after a recording ends its missing note still reads as "generating".
const NOTES_PENDING_WINDOW_MS = 60 * 60 * 1000;

// Measured from the recording's end (start + duration), so a long recording
// isn't already "old" the moment it stops. Reads the ticking `now`, so a row
// that crosses the line drops the indicator on its own.
function finishedRecently(m: MeetingListItem): boolean {
  const start = new Date(m.timestamp).getTime();
  if (Number.isNaN(start)) return false;
  const end = start + (m.durationSeconds ?? 0) * 1000;
  return now.value.getTime() - end < NOTES_PENDING_WINDOW_MS;
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

// The open local note was deleted from the detail panel. Drop the row and the
// selection right away so the list reflects it without waiting on a round trip,
// then reload from disk so the removal survives navigation and relaunch.
//
// Deliberately does NOT go through `clearSelection()`: that flushes the notes
// editor via `saveNotesNow()`, and the recording's folder no longer exists.
function onNoteDeleted(payload: { id: string }): void {
  meetings.value = meetings.value.filter((m) => m.id !== payload.id);
  pinnedMeetings.value.delete(payload.id);
  if (selectedItem.value?.id === payload.id) {
    // Invalidate any in-flight selection so a racing `selectMeeting` can't
    // re-seat the deleted note after this clears it.
    selectionReqId++;
    selectedItem.value = null;
    userSelectedMeetingId.value = null;
  }
  void loadMeetings(false, true);
}

// The open local recording finished generating. Its row still shows the stale
// "Processing…" sub-line the list payload described, so pull fresh list data —
// but only when the row actually claims to be processing, so simply opening an
// already-finished recording doesn't cost a list round trip.
async function onContentReady(payload: { id: string }): Promise<void> {
  const row = displayMeetings.value.find((m) => m.id === payload.id);
  if (row && rowProcessingLabel(row)) await loadMeetings();
}

// A tracked cloud meeting's transcript/notes just landed. Reload so its row
// swaps "Processing…" back for its normal sub-line and the detail panel picks up
// the content. The tracked set is normally 0-1 items, so this is cheap.
watch(processingMeetings.version, () => {
  void loadMeetings();
});

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
// `silent` re-syncs the list without the full-list "Loading…" placeholder, for
// callers that have already updated the UI optimistically and only need disk to
// confirm — blanking the list there would undo the immediate feedback.
async function loadMeetings(autoSelectFirst = false, silent = false): Promise<void> {
  const requestId = ++loadMeetingsRequest;
  if (!silent) loading.value = true;
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
    if (requestId === loadMeetingsRequest && !silent) loading.value = false;
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
    // The upload landed but the server still has to transcribe it. RecorderStrip
    // clears recordingMeetingId only *after* this callback (see the comment on
    // the 'closed' branch), so the id is still readable here — track it so the
    // row and detail panel say "processing" once the pill's checkmark closes.
    if (activeBackend.value?.id === 'ariso' && recordingMeetingId.value) {
      processingMeetings.markUploaded(recordingMeetingId.value);
    }
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

// The title of the meeting holding the recorder, for a blocked start. Only this
// window can resolve it: the backend knows the id but has no meeting titles.
function blockingMeetingTitle(id: string | null): string | null {
  if (id === null) return null;
  // displayMeetings already layers pinned ad-hoc meetings over the loaded list,
  // so a meeting recorded moments ago is findable here.
  const title = displayMeetings.value.find((m) => m.id === id)?.title?.trim();
  return title ? title : null;
}

function onRecordingStartFailed(event: { payload: unknown }): void {
  const payload = event.payload;
  // A start blocked by the incumbent recorder pill names which recording is in
  // the way and what clears it (#320); every other failure arrives with a
  // ready-made message.
  const blocked = recordingBlockedPayload(payload);
  if (blocked) {
    recordingStartError.value = recordingBlockedMessage(
      blocked.reason,
      blockingMeetingTitle(blocked.meetingId),
    );
  } else {
    recordingStartError.value =
      typeof payload === 'object'
      && payload !== null
      && 'message' in payload
      && typeof payload.message === 'string'
      && payload.message.length <= 300
        ? payload.message
        : recordingStartErrorMessage(payload);
  }
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
let unlistenAuthChanged: UnlistenFn | null = null;

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
  // A backend switch made in Settings (or another window).
  void listen(BACKEND_CHANGED_EVENT, (event) => {
    const source = (event.payload as { source?: unknown } | null)?.source;
    if (source === backendSwitchSource) return;
    void onBackendChanged();
  }).then((un) => {
    unlistenBackendChanged = un;
  });
  // Prep notification clicked while this window was already open.
  void listen('meeting-prep://open', () => {
    void openPendingMeetingPrep();
  }).then((un) => {
    unlistenPrepOpen = un;
  });
  // A sign-in or sign-out anywhere, or a rejected session cleared natively.
  void listen(AUTH_CHANGED_EVENT, onAuthChanged).then((un) => {
    unlistenAuthChanged = un;
  });
  clockTimer = window.setInterval(() => {
    now.value = new Date();
  }, 30_000);
  window.addEventListener('focus', onWindowFocus);
  window.addEventListener('keydown', onGlobalKeydown);
  document.addEventListener('mousedown', onDocumentMousedown);
});

onUnmounted(() => {
  if (clockTimer !== undefined) clearInterval(clockTimer);
  window.removeEventListener('focus', onWindowFocus);
  window.removeEventListener('keydown', onGlobalKeydown);
  document.removeEventListener('mousedown', onDocumentMousedown);
  unlistenRecordingStarted?.();
  unlistenRecordingState?.();
  unlistenRecordingStartFailed?.();
  unlistenRecordingReveal?.();
  unlistenVaultChanged?.();
  unlistenBackendChanged?.();
  unlistenPrepOpen?.();
  unlistenWindowResized?.();
  unlistenAuthChanged?.();
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
/* The top padding matches the detail pane's, so the search box lines up with
   the detail card. The search box, meeting rows and nav pill all span the
   sidebar's content width, 18px in from each side. */
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
/* Same pill as a non-interactive badge (an unattached recording has no meeting
   to re-dock on), so it must not invite a click it can't honor. */
.add-btn--static { cursor: default; }
.add-btn--static:hover { box-shadow: 1px 1px 0 #e7e5e2; transform: none; }
/* Backend indicator: a compact pill beside the sidebar toggle, matching the
   Start recording pill at the other end of the titlebar. Name first, then the
   backend's icon, as in Settings' backend picker. */
.account-pill-wrap {
  position: relative;
  display: flex;
  margin-left: 6px;
}
.titlebar--windows .account-pill-wrap { margin-left: 4px; }
.account-pill {
  max-width: 160px;
  height: 22px;
  padding: 0 7px 0 9px;
  gap: 5px;
  border-radius: 11px;
  background: #ffffff;
  border: 1px solid #d6d6d6;
  box-shadow: 1px 1px 0 #e7e5e2;
  color: #1a1a1a;
  font-family: inherit;
  display: flex;
  align-items: center;
  cursor: pointer;
  transition: transform 0.1s, box-shadow 0.1s;
}
.titlebar--windows .account-pill { height: 26px; border-radius: 13px; }
.account-pill:hover { box-shadow: 0 0 0 #e7e5e2; transform: translate(1px, 1px); }
.account-pill:focus-visible {
  outline: 2px solid #3b6fc4;
  outline-offset: 2px;
}
.account-pill-icon {
  width: 13px;
  height: 13px;
  flex: 0 0 auto;
  fill: none;
  stroke: currentColor;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.backend-menu-gear { stroke-width: 1.5; }
.account-pill-label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  font-size: 11px;
  font-weight: 600;
  line-height: 1;
  white-space: nowrap;
}
.backend-popover {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  z-index: 30;
}
/* Same card and rows as Settings' backend picker. */
.backend-menu {
  min-width: 160px;
  box-sizing: border-box;
  padding: 4px;
  background: #ffffff;
  border: 1px solid #e5e6e3;
  border-radius: 12px;
  box-shadow: 2px 2px 0 #e7e5e2;
}
.backend-menu-item {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 6px 10px;
  border: none;
  border-radius: 999px;
  background: transparent;
  color: #1c1c1c;
  font-family: inherit;
  font-size: 13px;
  white-space: nowrap;
  cursor: pointer;
}
.backend-menu-item:hover:not(:disabled) { background: rgba(0, 0, 0, 0.03); }
.backend-menu-item:focus-visible {
  background: #f5f5f7;
  outline: 2px solid #6366f1;
  outline-offset: -2px;
}
.backend-menu-item--active,
.backend-menu-item--active:hover:not(:disabled),
.backend-menu-item--active:focus-visible {
  background: #1c1c1c;
  color: #ffffff;
}
.backend-menu-item:disabled { opacity: 0.5; cursor: not-allowed; }
/* ariso.ai without a session: grayed, but still clickable, since it leads to
   the sign-in box. Local never checks the session, so it reads as signed out
   there too. When it's the active backend, a light fill still marks it. */
.backend-menu-item--signed-out { color: #8a8a86; }
.backend-menu-item--signed-out.backend-menu-item--active,
.backend-menu-item--signed-out.backend-menu-item--active:hover:not(:disabled),
.backend-menu-item--signed-out.backend-menu-item--active:focus-visible {
  background: #efeeeb;
  color: #8a8a86;
}
.backend-menu-hint {
  margin: 4px 10px;
  font-size: 11px;
  color: #6f6f6f;
  white-space: nowrap;
}
.backend-menu-sep {
  height: 1px;
  margin: 4px 6px;
  background: #e5e6e3;
}
.sign-in-popover {
  width: 260px;
  box-sizing: border-box;
  padding: 14px;
  background: #ffffff;
  border: 1px solid #e5e6e3;
  border-radius: 12px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.12);
}
.sign-in-popover-title {
  margin: 0 0 12px;
  font-size: 13px;
  font-weight: 600;
  color: #1c1c1c;
}
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
  width: 100%;
  min-height: 42px;
  margin: 0 0 10px;
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

.hint { flex: 1; font-size: 14px; color: #6f6f6f; padding: 0 6px; }

/* Meeting list with top/bottom fade so the first/last rows dissolve into the
   backdrop on scroll, matching the design. */
.meeting-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 6px;
  /* No left padding: rows start flush with the search box and nav pill. */
  padding: 6px 6px 6px 0;
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
/* An action item is a sentence, not a title: let it wrap to two lines rather
   than truncate mid-thought, and keep its source meeting on a single line. */
.todo-item .mi-title {
  white-space: normal;
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
}
.todo-item .mi-sub {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.mi-sub--now { color: #2e8b4f; font-weight: 500; }
/* Sits where the time/duration sub-line normally does, so a processing row is
   the same height as every other one. */
.mi-sub--processing {
  display: flex;
  align-items: center;
  gap: 6px;
}
.mi-spinner {
  flex: 0 0 10px;
  width: 10px;
  height: 10px;
  border: 1.5px solid #dedbd6;
  border-bottom-color: #6f6f6f;
  border-radius: 50%;
  animation: mi-spin 0.7s linear infinite;
}
@keyframes mi-spin { to { transform: rotate(360deg); } }

/* Bottom nav */
.bottom-nav {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  padding-top: 24px;
}
.nav-pill {
  flex: 1;
  min-width: 0;
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
  /* Share the pill's width, so the tabs fill it edge to edge. */
  flex: 1 1 auto;
  justify-content: center;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px;
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
