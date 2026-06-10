<template>
  <div class="card">
    <div v-if="loading" class="card-state">
      <span class="spinner" />
      <span>Loading meeting…</span>
    </div>
    <div v-else-if="error" class="card-state card-state--error">{{ error }}</div>

    <template v-else-if="detail">
      <!-- Header: title + subtitle, share / link / close -->
      <header class="card-head">
        <div class="head-titles">
          <input
            v-if="editingTitle"
            ref="titleInput"
            v-model="titleDraft"
            class="head-title head-title--input"
            type="text"
            :disabled="savingTitle"
            aria-label="Meeting title"
            @keydown.enter.prevent="commitTitle"
            @keydown.esc.prevent="cancelTitleEdit"
            @blur="commitTitle"
          />
          <h1
            v-else
            class="head-title"
            :class="{ 'head-title--editable': canEditTitle }"
            :role="canEditTitle ? 'button' : undefined"
            :tabindex="canEditTitle ? 0 : undefined"
            :title="canEditTitle ? 'Click to rename' : undefined"
            @click="startTitleEdit"
            @keydown.enter="startTitleEdit"
          >{{ detail.title }}</h1>
          <p class="head-sub">{{ subtitle }}</p>
        </div>
        <div class="head-actions">
          <button class="btn-share" type="button" title="Share">
            <svg viewBox="0 0 24 24" class="ic"><path d="M4 12v7a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-7" /><path d="M16 6l-4-4-4 4" /><path d="M12 2v13" /></svg>
            Share
          </button>
          <button class="btn-icon" type="button" aria-label="Copy link" title="Copy link">
            <svg viewBox="0 0 24 24" class="ic"><path d="M10 13a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1 1" /><path d="M14 11a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1-1" /></svg>
          </button>
          <button class="btn-icon btn-close" type="button" aria-label="Close" title="Close" @click="emit('close')">
            <svg viewBox="0 0 24 24" class="ic"><path d="M6 6l12 12M18 6L6 18" /></svg>
          </button>
        </div>
      </header>

      <!-- Meta band: duration · attendees · category -->
      <div v-if="hasMeta" class="card-meta">
        <div v-if="durationLabel" class="meta-item">
          <svg viewBox="0 0 24 24" class="ic"><path d="M12 8v4l3 2m6-2a9 9 0 1 1-18 0 9 9 0 0 1 18 0z" /></svg>
          <span class="dur">{{ durationLabel }}</span>
        </div>
        <div v-if="detail.participants.length" class="meta-item attendees">
          <span class="avatars">
            <span
              v-for="(p, i) in detail.participants.slice(0, 4)"
              :key="i"
              class="avatar"
              :style="{ background: avatarColor(i) }"
              :title="p.name || p.email || ''"
            >{{ initials(p.name || p.email) }}</span>
            <span v-if="detail.participants.length > 4" class="avatar avatar--more">+{{ detail.participants.length - 4 }}</span>
          </span>
          <span class="attendees-label">{{ detail.participants.length }} Attendees</span>
        </div>
        <span v-if="detail.meetingType" class="chip">
          <span class="chip-hash">#</span>{{ formatType(detail.meetingType) }}
        </span>
      </div>

      <div v-else class="divider" />

      <!-- Tabs + Chat -->
      <div v-if="availableTabs.length" class="card-tabs">
        <div class="segment">
          <button
            v-for="t in availableTabs"
            :key="t.key"
            class="seg-btn"
            :class="{ 'seg-btn--active': activeTab === t.key }"
            type="button"
            @click="activeTab = t.key"
          >{{ t.label }}</button>
        </div>
        <!-- add chat with meeting button later -->
      </div>

      <!-- Content -->
      <div class="card-content">
        <div v-if="!availableTabs.length" class="content-empty">
          {{ detail.isLocal ? 'No notes or transcript yet for this recording.' : 'No notes available for this meeting yet.' }}
        </div>

        <template v-if="activeTab === 'note'">
          <!-- Local note -->
          <div v-if="detail.isLocal && detail.note" class="md" v-html="renderMarkdown(detail.note)" />

          <!-- Ariso rich content -->
          <template v-if="!detail.isLocal">
            <section v-if="detail.digest" class="sec">
              <h3 class="sec-h">Quick Digest</h3>
              <div class="md" v-html="renderMarkdown(detail.digest)" />
            </section>

            <section v-if="detail.actionItems.length" class="sec">
              <h3 class="sec-h">Action Items <span class="count">{{ detail.actionItems.length }}</span></h3>
              <div class="ai-groups">
                <div v-for="(g, gi) in groupedActionItems" :key="gi" class="ai-group">
                  <div v-if="g.name" class="ai-owner">
                    <span class="avatar avatar--sm" :style="{ background: avatarColor(gi) }">{{ initials(g.name) }}</span>
                    <span class="ai-name">{{ g.name }}</span>
                  </div>
                  <ol class="ai-list" :class="{ 'ai-list--indent': g.name }">
                    <li v-for="(it, i) in g.items" :key="i">{{ it.item }}</li>
                  </ol>
                </div>
              </div>
            </section>

            <section v-if="detail.summary" class="sec">
              <div class="acc">
                <button class="acc-btn" type="button" @click="showFullNotes = !showFullNotes">
                  <span>Full Meeting Notes</span>
                  <svg viewBox="0 0 24 24" class="ic chevron" :class="{ open: showFullNotes }"><path d="M19 9l-7 7-7-7" /></svg>
                </button>
                <div v-if="showFullNotes" class="acc-body"><div class="md" v-html="renderMarkdown(detail.summary)" /></div>
              </div>
            </section>

            <section v-if="detail.score !== undefined" class="sec">
              <h3 class="sec-h">Meeting Assessment</h3>
              <div class="assess-score">
                <div class="score-circle" :style="{ background: scoreBadge?.bg, color: scoreBadge?.text, boxShadow: `0 0 0 4px ${scoreBadge?.ring}` }">{{ detail.score }}</div>
                <div>
                  <div class="score-label">{{ scoreBadge?.label }}</div>
                  <div class="score-sub">out of 5</div>
                </div>
              </div>
              <div v-if="detail.rationale" class="assess-block"><div class="assess-h">Why this score</div><p>{{ detail.rationale }}</p></div>
              <div v-if="detail.recommendation" class="assess-block"><div class="assess-h">Recommendation</div><p>{{ detail.recommendation }}</p></div>
            </section>

            <section v-if="hasCoaching" class="sec">
              <div class="coaching">
                <h3 class="sec-h">Your Coaching</h3>
                <div v-if="detail.coaching?.strengths?.length" class="coach-block">
                  <div class="coach-h coach-h--green">Strengths</div>
                  <ul class="coach-list"><li v-for="(s, i) in detail.coaching!.strengths" :key="i"><span class="bullet bullet--green">•</span>{{ s }}</li></ul>
                </div>
                <div v-if="detail.coaching?.improvements?.length" class="coach-block">
                  <div class="coach-h coach-h--amber">Areas to Grow</div>
                  <ul class="coach-list"><li v-for="(s, i) in detail.coaching!.improvements" :key="i"><span class="bullet bullet--amber">•</span>{{ s }}</li></ul>
                </div>
                <div v-if="detail.coaching?.patterns" class="coach-block coach-pattern">
                  <div class="coach-h coach-h--purple">Pattern Observed</div>
                  <p>{{ detail.coaching!.patterns }}</p>
                </div>
              </div>
            </section>
          </template>
        </template>

        <template v-else-if="activeTab === 'transcript'">
          <div v-if="loadingTranscript" class="card-state"><span class="spinner" /><span>Loading transcript…</span></div>
          <div v-else-if="transcriptText" class="md" v-html="renderMarkdown(transcriptText)" />
          <div v-else class="content-empty">No transcript available.</div>
        </template>

        <template v-else-if="activeTab === 'mynote'">
          <div class="notes-head">
            <h2 v-if="individualNote?.title" class="notes-title">{{ individualNote.title }}</h2>
          </div>
          <div v-if="loadingIndividualNote" class="card-state"><span class="spinner" /><span>Loading note…</span></div>
          <div v-else-if="individualNote?.content" class="md" v-html="renderMarkdown(individualNote.content)" />
          <div v-else class="content-empty">No personal note for this meeting.</div>
        </template>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue';
import { renderMarkdown } from '../utils/markdown';
import { useMeetingApi } from '../composables/useMeetingApi';
import {
  getActiveBackend,
  type MeetingListItem,
  type MeetingDetail,
  type MeetingActionItem,
  type MeetingCoaching,
} from '../composables/useBackend';

const props = defineProps<{ item: MeetingListItem | null }>();
const emit = defineEmits<{
  close: [];
  titleUpdated: [payload: { id: string; title: string }];
}>();

const loading = ref(false);
const error = ref<string | null>(null);
const detail = ref<MeetingDetail | null>(null);
const showFullNotes = ref(false);
const activeTab = ref<'note' | 'transcript' | 'mynote'>('note');

// Inline title editing. Only Ariso meetings have a server-side rename endpoint
// (PATCH /meeting-notes/{id}); local recordings render a plain title.
const editingTitle = ref(false);
const titleDraft = ref('');
const savingTitle = ref(false);
const titleInput = ref<HTMLInputElement | null>(null);
const canEditTitle = computed(() => !!detail.value && !detail.value.isLocal);

// Transcript is loaded lazily when the Transcript tab is opened (Ariso fetches
// /meeting-notes/{id}/transcript; local reads transcript.md).
const transcript = ref<string | null>(null);
const transcriptLoaded = ref(false);
const loadingTranscript = ref(false);

// Individual ("My note") — lazily loaded from /meeting-notes/{id}/individual-note.
const individualNote = ref<{ content: string; title: string | null } | null>(null);
const individualNoteLoaded = ref(false);
const loadingIndividualNote = ref(false);

let reqId = 0;

async function load(item: MeetingListItem | null): Promise<void> {
  // Bump the token first so any in-flight load for the previous selection
  // (including one cleared by item=null) is treated as stale on resolve.
  const my = ++reqId;
  loading.value = false;
  detail.value = null;
  error.value = null;
  showFullNotes.value = false;
  activeTab.value = 'note';
  editingTitle.value = false;
  savingTitle.value = false;
  transcript.value = null;
  transcriptLoaded.value = false;
  loadingTranscript.value = false;
  individualNote.value = null;
  individualNoteLoaded.value = false;
  loadingIndividualNote.value = false;
  if (!item) return;
  loading.value = true;
  try {
    const backend = await getActiveBackend();
    const d = await backend.getMeetingDetail(item);
    if (my !== reqId) return;
    detail.value = d;
    activeTab.value = firstTabFor(d); // default to the first available tab
  } catch (e) {
    if (my !== reqId) return;
    console.error('Failed to load meeting detail', e);
    error.value = 'Could not load this meeting.';
  } finally {
    if (my === reqId) loading.value = false;
  }
}

// Watch the detail-relevant fields, not just the id, so same-id metadata
// updates (e.g. local hasNote/hasTranscript flipping after a refresh) reload.
watch(
  () => [
    props.item?.id,
    props.item?.timestamp,
    props.item?.durationSeconds,
    props.item?.files?.hasNote,
    props.item?.files?.hasTranscript,
  ],
  () => load(props.item),
  { immediate: true }
);

async function startTitleEdit(): Promise<void> {
  if (!canEditTitle.value || editingTitle.value) return;
  titleDraft.value = detail.value?.title ?? '';
  editingTitle.value = true;
  await nextTick();
  titleInput.value?.focus();
  titleInput.value?.select();
}

function cancelTitleEdit(): void {
  editingTitle.value = false;
  savingTitle.value = false;
}

// Persist the edited title. Blur and Enter both route here; the savingTitle /
// editingTitle guards make the second call (Enter then blur) a no-op. A no-op
// or whitespace-only edit just closes the editor without hitting the API.
async function commitTitle(): Promise<void> {
  if (!editingTitle.value || savingTitle.value) return;
  const d = detail.value;
  if (!d) {
    editingTitle.value = false;
    return;
  }
  const next = titleDraft.value.trim();
  if (!next || next === d.title) {
    editingTitle.value = false;
    return;
  }
  savingTitle.value = true;
  const my = reqId;
  try {
    const { updateMeetingNotesTitle } = useMeetingApi();
    await updateMeetingNotesTitle(d.id, next);
    if (my !== reqId) return; // selection changed mid-save — drop the stale result
    d.title = next;
    emit('titleUpdated', { id: d.id, title: next });
    editingTitle.value = false;
  } catch (e) {
    console.error('Failed to update meeting title', e);
    // Keep the editor open so the user's text survives and they can retry.
  } finally {
    if (my === reqId) savingTitle.value = false;
  }
}

// Local recordings already carry their transcript on the detail; Ariso loads it
// lazily into `transcript`. Either way this is what the Transcript tab renders.
const transcriptText = computed<string | null>(() =>
  detail.value?.isLocal ? detail.value?.transcript ?? null : transcript.value
);

function coachingPresent(c?: MeetingCoaching | null): boolean {
  return !!(c && (c.strengths?.length || c.improvements?.length || c.patterns));
}
function notesPresent(d: MeetingDetail): boolean {
  return d.isLocal
    ? !!d.note
    : !!(d.digest || d.summary || d.actionItems.length || d.score !== undefined || coachingPresent(d.coaching));
}
function firstTabFor(d: MeetingDetail): 'note' | 'transcript' | 'mynote' {
  if (notesPresent(d)) return 'note';
  if (d.hasTranscript) return 'transcript';
  if (d.hasIndividualNote) return 'mynote';
  return 'note';
}

// Tabs appear only when their content exists: Note (meeting notes), Transcript,
// then My note (the requester's individual note).
const availableTabs = computed<{ key: 'note' | 'transcript' | 'mynote'; label: string }[]>(() => {
  const d = detail.value;
  if (!d) return [];
  const out: { key: 'note' | 'transcript' | 'mynote'; label: string }[] = [];
  if (notesPresent(d)) out.push({ key: 'note', label: 'Note' });
  if (d.hasTranscript) out.push({ key: 'transcript', label: 'Transcript' });
  if (d.hasIndividualNote) out.push({ key: 'mynote', label: 'My note' });
  return out;
});

// Fetch the Ariso transcript the first time the tab is opened. Local recordings
// already have their content, so they skip the round trip.
async function loadTranscript(): Promise<void> {
  const d = detail.value;
  if (!props.item || !d || d.isLocal || transcriptLoaded.value || loadingTranscript.value) return;
  const my = reqId;
  loadingTranscript.value = true;
  try {
    const backend = await getActiveBackend();
    const t = await backend.getMeetingTranscript(props.item);
    if (my !== reqId) return;
    transcript.value = t;
    transcriptLoaded.value = true;
  } catch (e) {
    if (my !== reqId) return;
    console.error('Failed to load transcript', e);
    transcript.value = null;
    transcriptLoaded.value = true;
  } finally {
    if (my === reqId) loadingTranscript.value = false;
  }
}

// Fetch the requester's individual note the first time the My-note tab opens.
async function loadIndividualNote(): Promise<void> {
  if (!props.item || individualNoteLoaded.value || loadingIndividualNote.value) return;
  const my = reqId;
  loadingIndividualNote.value = true;
  try {
    const backend = await getActiveBackend();
    const n = await backend.getIndividualNote(props.item);
    if (my !== reqId) return;
    individualNote.value = n;
    individualNoteLoaded.value = true;
  } catch (e) {
    if (my !== reqId) return;
    console.error('Failed to load individual note', e);
    individualNote.value = null;
    individualNoteLoaded.value = true;
  } finally {
    if (my === reqId) loadingIndividualNote.value = false;
  }
}

watch(activeTab, (t) => {
  if (t === 'transcript') void loadTranscript();
  else if (t === 'mynote') void loadIndividualNote();
});

const subtitle = computed(() => {
  const d = detail.value;
  if (!d) return '';
  const parts: string[] = [];
  const dt = new Date(d.startAt);
  if (!Number.isNaN(dt.getTime())) {
    parts.push(dt.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' }));
    parts.push(dt.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' }));
  }
  if (durationLabel.value) parts.push(durationLabel.value);
  return parts.join(' • ');
});

const hasCoaching = computed(() => {
  const c = detail.value?.coaching;
  return !!(c && (c.strengths?.length || c.improvements?.length || c.patterns));
});

// The meta band (duration · attendees · category) renders only when at least
// one of its fields is present; otherwise a plain divider separates header and tabs.
const hasMeta = computed(
  () => !!(durationLabel.value || detail.value?.participants.length || detail.value?.meetingType)
);

const otesEmpty = computed(() => {
  const d = detail.value;
  if (!d) return false;
  if (d.isLocal) return !d.note;
  return !d.digest && !d.summary && !d.actionItems.length && d.score === undefined && !hasCoaching.value;
});

const groupedActionItems = computed<{ name?: string; items: MeetingActionItem[] }[]>(() => {
  const groups = new Map<string, MeetingActionItem[]>();
  const ungrouped: MeetingActionItem[] = [];
  for (const it of detail.value?.actionItems ?? []) {
    if (it.name) {
      if (!groups.has(it.name)) groups.set(it.name, []);
      groups.get(it.name)!.push(it);
    } else {
      ungrouped.push(it);
    }
  }
  const out = [...groups.entries()].map(([name, items]) => ({ name, items }));
  if (ungrouped.length) out.push({ name: undefined, items: ungrouped });
  return out;
});

const SCORE_BADGES = [
  null,
  { label: 'Poor', bg: '#fee2e2', text: '#b91c1c', ring: '#ef4444' },
  { label: 'Unproductive', bg: '#ffedd5', text: '#c2410c', ring: '#f97316' },
  { label: 'Fair', bg: '#f3f4f6', text: '#374151', ring: '#6b7280' },
  { label: 'Productive', bg: '#dbeafe', text: '#1d4ed8', ring: '#3b82f6' },
  { label: 'Exceptional', bg: '#dcfce7', text: '#15803d', ring: '#22c55e' },
] as const;

const scoreBadge = computed(() => {
  const s = detail.value?.score;
  if (s === undefined || s < 1 || s > 5) return null;
  return SCORE_BADGES[s];
});

const AVATAR_COLORS = ['#6c63c0', '#0ea5e9', '#f59e0b', '#ec4899', '#22c55e', '#64748b'];
function avatarColor(i: number): string {
  return AVATAR_COLORS[i % AVATAR_COLORS.length];
}

function initials(name?: string): string {
  if (!name) return '?';
  return name.split(/\s+/).filter(Boolean).map((n) => n[0]).join('').toUpperCase().slice(0, 2) || '?';
}

function formatType(t: string): string {
  return t.replace(/[_-]+/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

function formatDateTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, { month: 'short', day: 'numeric', year: 'numeric', hour: 'numeric', minute: '2-digit' });
}

const durationLabel = computed<string | null>(() => {
  const d = detail.value;
  if (!d) return null;
  let secs: number | null = null;
  if (d.durationSeconds != null) secs = d.durationSeconds;
  else if (d.endAt) {
    const ms = new Date(d.endAt).getTime() - new Date(d.startAt).getTime();
    if (Number.isFinite(ms) && ms > 0 && ms < 24 * 60 * 60 * 1000) secs = ms / 1000;
  }
  if (secs == null) return null;
  const mins = Math.floor(secs / 60);
  const hours = Math.floor(mins / 60);
  const rem = mins % 60;
  return hours > 0 ? `${hours}h ${rem}m` : `${Math.max(1, mins)}m`;
});
</script>

<style scoped>
.card {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: #ffffff;
  border: 1px solid #e5e6e3;
  border-radius: 16px;
  overflow: hidden;
  font-family: 'Polymath', -apple-system, system-ui, sans-serif;
  color: #1c1c1c;
}

.card-state { display: flex; align-items: center; justify-content: center; gap: 10px; flex: 1; color: #6f6f6f; font-size: 14px; }
.card-state--error { color: #dc2626; }
.spinner { width: 18px; height: 18px; border: 2px solid #e5e6e3; border-bottom-color: #1c1c1c; border-radius: 50%; animation: spin 0.7s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }

.ic { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; flex-shrink: 0; }

/* Header */
.card-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; padding: 22px 24px 12px; border-bottom: 1px solid #e5e6e3; }
.head-titles { min-width: 0; }
.head-title { margin: 0; font-size: 22px; font-weight: 700; line-height: 1.2; color: #1c1c1c; }
.head-title--editable { cursor: text; }
.head-title--input {
  display: block; width: 100%; height: 1.2em; margin: 0; padding: 0; box-sizing: border-box;
  position: relative; top: -1px;
  font-family: inherit; font-size: 22px; font-weight: 700; line-height: 1.2; color: #1c1c1c;
  border: none; background: transparent; outline: none; appearance: none;
}
.head-title--input:disabled { opacity: 0.6; }
.head-sub { margin: 4px 0 0; font-size: 13px; color: #6f6f6f; }
.head-actions { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
.btn-share {
  display: flex; align-items: center; gap: 6px;
  height: 32px; padding: 0 12px;
  background: #fff; border: 1px solid #d6d6d6; border-radius: 8px;
  box-shadow: 2px 2px 0 #e7e5e2;
  font-family: inherit; font-size: 14px; font-weight: 600; color: #1a1a1a; cursor: pointer;
}
.btn-share:hover { background: #fbfbfb; }
.btn-icon {
  width: 32px; height: 32px; display: flex; align-items: center; justify-content: center;
  background: #fff; border: 1px solid #d6d6d6; border-radius: 8px; box-shadow: 2px 2px 0 #e7e5e2;
  color: #1a1a1a; cursor: pointer;
}
.btn-icon:hover { background: #fbfbfb; }
.btn-close { border-radius: 50%; }

/* Meta band — full-bleed strip below the header (Figma 2827:34384) */
.card-meta { display: flex; flex-wrap: wrap; align-items: center; gap: 16px; padding: 11px 24px; background: #f7f6f4; border-bottom: 1px solid #e5e6e3; font-size: 14px; }
.meta-item { display: flex; align-items: center; gap: 4px; color: #6f6f6f; }
.meta-item .ic { width: 15px; height: 15px; }
.dur { color: #1c1c1c; font-size: 14px; }
.attendees { gap: 0; }
.avatars { display: flex; align-items: center; }
.avatar { width: 23px; height: 23px; border-radius: 50%; display: flex; align-items: center; justify-content: center; color: #fff; font-size: 9px; font-weight: 600; border: 2px solid #f7f6f4; margin-left: -5px; }
.avatar:first-child { margin-left: 0; }
.avatar--more { width: 24px; height: 24px; background: #ecebe8; border: 1px solid #d6d6d6; color: #6f6f6f; font-size: 10px; font-weight: 400; }
.avatar--sm { width: 22px; height: 22px; border: none; margin: 0; font-size: 9px; }
.attendees-label { color: #6f6f6f; font-size: 12px; padding-left: 8px; }
.chip {
  display: inline-flex; align-items: center; gap: 4px;
  padding: 2px 8px; background: #ecebe8; border: 1px solid #d2d2d2; border-radius: 12px;
  font-size: 12px; color: #6f6f6f;
}
.chip-hash { color: #6f6f6f; font-weight: 400; }

.divider { height: 1px; background: #e5e6e3; }

/* Tabs */
.card-tabs { display: flex; align-items: center; gap: 12px; padding: 16px 24px; }
.segment { display: inline-flex; gap: 2px; padding: 3px; background: #ecebe8; border-radius: 10px; }
.seg-btn {
  padding: 6px 12px; border: none; background: transparent; border-radius: 7px;
  font-family: inherit; font-size: 14px; font-weight: 500; color: #6f6f6f; cursor: pointer;
  white-space: nowrap;
}
.seg-btn:disabled { color: #b6b5b1; cursor: default; }
.seg-btn--active { background: #ffffff; color: #1c1c1c; box-shadow: 1px 1px 0 #d6d6d6; }
.btn-chat {
  margin-left: auto;
  height: 32px; padding: 0 14px;
  background: #000; color: #fff; border: none; border-radius: 8px;
  box-shadow: 2px 2px 0 rgba(0, 0, 0, 0.25);
  font-family: inherit; font-size: 14px; font-weight: 600; cursor: pointer; white-space: nowrap;
}
.btn-chat:hover { background: #1a1a1a; }

/* Content */
.card-content { flex: 1; min-height: 0; overflow-y: auto; padding: 8px 24px 24px; }
.notes-head { margin-bottom: 14px; }
.notes-title { margin: 0; font-size: 20px; font-weight: 700; color: #1c1c1c; }
.notes-date { margin: 4px 0 0; font-size: 13px; color: #6f6f6f; }
.content-empty { color: #6f6f6f; font-size: 14px; padding: 8px 0; }

.sec { margin-bottom: 22px; }
.sec:last-child { margin-bottom: 0; }
.sec-h { margin: 0 0 10px; font-size: 15px; font-weight: 600; color: #1c1c1c; display: flex; align-items: center; gap: 8px; }
.count { padding: 1px 8px; background: #ecebe8; color: #535353; font-size: 12px; font-weight: 500; border-radius: 999px; }

.ai-groups { display: flex; flex-direction: column; gap: 16px; }
.ai-owner { display: flex; align-items: center; gap: 8px; margin-bottom: 8px; }
.ai-name { font-size: 14px; font-weight: 500; color: #1c1c1c; }
.ai-list { margin: 0; padding-left: 20px; display: flex; flex-direction: column; gap: 6px; color: #535353; font-size: 14px; }
.ai-list--indent { margin-left: 30px; }

.acc { border: 1px solid #e5e6e3; border-radius: 10px; overflow: hidden; }
.acc-btn { width: 100%; padding: 12px 14px; display: flex; align-items: center; justify-content: space-between; background: #fff; border: none; cursor: pointer; font-family: inherit; font-size: 14px; font-weight: 500; color: #1c1c1c; }
.acc-btn:hover { background: #fbfbfb; }
.chevron { width: 18px; height: 18px; color: #6f6f6f; transition: transform 0.15s; }
.chevron.open { transform: rotate(180deg); }
.acc-body { padding: 14px; background: #faf9f7; border-top: 1px solid #e5e6e3; }

.assess-score { display: flex; align-items: center; gap: 12px; margin-bottom: 14px; }
.score-circle { width: 48px; height: 48px; border-radius: 50%; display: flex; align-items: center; justify-content: center; font-size: 20px; font-weight: 700; }
.score-label { font-weight: 600; color: #1c1c1c; font-size: 14px; }
.score-sub { font-size: 12px; color: #6f6f6f; }
.assess-block { margin-top: 12px; font-size: 14px; }
.assess-h { font-weight: 500; color: #535353; margin-bottom: 4px; }
.assess-block p { margin: 0; color: #6f6f6f; line-height: 1.5; }

.coaching { background: linear-gradient(135deg, rgba(108, 99, 192, 0.05), rgba(108, 99, 192, 0.1)); border: 1px solid rgba(108, 99, 192, 0.2); border-radius: 10px; padding: 16px; }
.coach-block { margin-top: 14px; }
.coach-block:first-of-type { margin-top: 0; }
.coach-h { font-weight: 500; font-size: 14px; margin-bottom: 6px; }
.coach-h--green { color: #15803d; }
.coach-h--amber { color: #b45309; }
.coach-h--purple { color: #6c63c0; }
.coach-pattern { padding-top: 12px; border-top: 1px solid rgba(108, 99, 192, 0.2); }
.coach-pattern p { margin: 0; color: #535353; font-size: 14px; line-height: 1.5; }
.coach-list { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 6px; color: #535353; font-size: 14px; }
.coach-list li { display: flex; align-items: flex-start; gap: 8px; line-height: 1.45; }
.bullet { flex-shrink: 0; margin-top: 1px; }
.bullet--green { color: #22c55e; }
.bullet--amber { color: #f59e0b; }

/* Rendered markdown */
.md { color: #535353; font-size: 14px; line-height: 1.6; }
.md :deep(h1), .md :deep(h2), .md :deep(h3) { color: #1c1c1c; font-weight: 600; margin: 16px 0 8px; }
.md :deep(h1) { font-size: 16px; }
.md :deep(h2) { font-size: 15px; }
.md :deep(h3) { font-size: 14px; }
.md :deep(p) { margin: 0 0 10px; }
.md :deep(ul), .md :deep(ol) { margin: 0 0 10px; padding-left: 22px; display: flex; flex-direction: column; gap: 4px; }
.md :deep(li) { line-height: 1.5; }
.md :deep(li.task-list-item) { list-style: none; margin-left: -22px; display: flex; align-items: flex-start; gap: 8px; }
.md :deep(li.task-list-item input[type="checkbox"]) { margin: 4px 0 0; flex: none; }
.md :deep(strong) { font-weight: 600; color: #1c1c1c; }
.md :deep(code) { background: #f0eeed; padding: 1px 5px; border-radius: 4px; font-size: 0.9em; }
.md :deep(a) { color: #6c63c0; text-decoration: underline; }
.md :deep(blockquote) { margin: 0 0 10px; padding-left: 12px; border-left: 3px solid #e5e6e3; color: #6f6f6f; }
.md :deep(*:last-child) { margin-bottom: 0; }
</style>
