<template>
  <div class="detail">
    <div v-if="loading" class="detail-state">
      <span class="spinner" />
      <span>Loading meeting details…</span>
    </div>

    <div v-else-if="error" class="detail-state detail-state--error">{{ error }}</div>

    <template v-else-if="detail">
      <!-- Branded stripe header: black base + diagonal yellow/purple band -->
      <header
        class="stripe"
        :class="detail.external ? 'stripe--external' : 'stripe--internal'"
      >
        <div class="stripe-titles">
          <h1 class="stripe-title">{{ detail.title }}</h1>
          <p class="stripe-date">{{ formatDateTime(detail.startAt) }}</p>
        </div>
        <span v-if="detail.visibility" class="vis-pill">{{ detail.visibility }}</span>
      </header>

      <!-- Metadata bar -->
      <div v-if="durationLabel || detail.participants.length || scoreBadge" class="meta">
        <div v-if="durationLabel" class="meta-item">
          <svg viewBox="0 0 24 24" class="ic"><path d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
          <span>{{ durationLabel }}</span>
        </div>
        <div v-if="detail.participants.length" class="avatars">
          <span
            v-for="(p, i) in detail.participants.slice(0, 5)"
            :key="i"
            class="avatar"
            :style="{ background: avatarColor(i) }"
            :title="p.name || p.email || ''"
          >{{ initials(p.name || p.email) }}</span>
          <span v-if="detail.participants.length > 5" class="avatar avatar--more">
            +{{ detail.participants.length - 5 }}
          </span>
        </div>
        <div
          v-if="scoreBadge"
          class="score-pill"
          :style="{ background: scoreBadge.bg, color: scoreBadge.text }"
        >
          <span class="score-dot" :style="{ background: scoreBadge.ring }" />
          {{ scoreBadge.label }}
        </div>
      </div>

      <!-- Body -->
      <div class="body">
        <!-- Quick Digest -->
        <section v-if="detail.digest" class="sec">
          <div class="sec-head">
            <svg viewBox="0 0 24 24" class="ic ic--purple"><path d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" /></svg>
            <h3>Quick Digest</h3>
          </div>
          <div class="md" v-html="renderMarkdown(detail.digest)" />
        </section>

        <!-- Action Items -->
        <section v-if="detail.actionItems.length" class="sec">
          <div class="sec-head">
            <svg viewBox="0 0 24 24" class="ic ic--purple"><path d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4" /></svg>
            <h3>Action Items</h3>
            <span class="count">{{ detail.actionItems.length }}</span>
          </div>
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

        <!-- Full Meeting Notes (expandable) -->
        <section v-if="detail.summary" class="sec">
          <div class="acc">
            <button class="acc-btn" @click="showFullNotes = !showFullNotes">
              <span class="acc-left">
                <svg viewBox="0 0 24 24" class="ic ic--gray"><path d="M4 6h16M4 12h16M4 18h7" /></svg>
                Full Meeting Notes
              </span>
              <svg viewBox="0 0 24 24" class="ic ic--gray chevron" :class="{ open: showFullNotes }"><path d="M19 9l-7 7-7-7" /></svg>
            </button>
            <div v-if="showFullNotes" class="acc-body">
              <div class="md" v-html="renderMarkdown(detail.summary)" />
            </div>
          </div>
        </section>

        <!-- Meeting Assessment -->
        <section v-if="detail.score !== undefined" class="sec">
          <div class="sec-head">
            <svg viewBox="0 0 24 24" class="ic ic--purple"><path d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" /></svg>
            <h3>Meeting Assessment</h3>
          </div>
          <div class="assess-score">
            <div
              class="score-circle"
              :style="{ background: scoreBadge?.bg, color: scoreBadge?.text, boxShadow: `0 0 0 4px ${scoreBadge?.ring}` }"
            >{{ detail.score }}</div>
            <div>
              <div class="score-label">{{ scoreBadge?.label }}</div>
              <div class="score-sub">out of 5</div>
            </div>
          </div>
          <div v-if="detail.rationale" class="assess-block">
            <div class="assess-h">Why this score</div>
            <p>{{ detail.rationale }}</p>
          </div>
          <div v-if="detail.recommendation" class="assess-block">
            <div class="assess-h">Recommendation</div>
            <p>{{ detail.recommendation }}</p>
          </div>
        </section>

        <!-- Your Coaching -->
        <section v-if="hasCoaching" class="sec">
          <div class="coaching">
            <div class="sec-head">
              <svg viewBox="0 0 24 24" class="ic ic--purple"><path d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" /></svg>
              <h3>Your Coaching</h3>
            </div>
            <div v-if="detail.coaching?.strengths?.length" class="coach-block">
              <div class="coach-h coach-h--green">Strengths</div>
              <ul class="coach-list">
                <li v-for="(s, i) in detail.coaching!.strengths" :key="i">
                  <span class="bullet bullet--green">•</span>{{ s }}
                </li>
              </ul>
            </div>
            <div v-if="detail.coaching?.improvements?.length" class="coach-block">
              <div class="coach-h coach-h--amber">Areas to Grow</div>
              <ul class="coach-list">
                <li v-for="(s, i) in detail.coaching!.improvements" :key="i">
                  <span class="bullet bullet--amber">•</span>{{ s }}
                </li>
              </ul>
            </div>
            <div v-if="detail.coaching?.patterns" class="coach-block coach-pattern">
              <div class="coach-h coach-h--purple">Pattern Observed</div>
              <p>{{ detail.coaching!.patterns }}</p>
            </div>
          </div>
        </section>

        <!-- Local recording: notes + transcript -->
        <section v-if="detail.isLocal && detail.note" class="sec">
          <div class="sec-head">
            <svg viewBox="0 0 24 24" class="ic ic--purple"><path d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" /></svg>
            <h3>Notes</h3>
          </div>
          <div class="md" v-html="renderMarkdown(detail.note)" />
        </section>
        <section v-if="detail.isLocal && detail.transcript" class="sec">
          <div class="acc">
            <button class="acc-btn" @click="showTranscript = !showTranscript">
              <span class="acc-left">
                <svg viewBox="0 0 24 24" class="ic ic--gray"><path d="M4 6h16M4 12h16M4 18h7" /></svg>
                Transcript
              </span>
              <svg viewBox="0 0 24 24" class="ic ic--gray chevron" :class="{ open: showTranscript }"><path d="M19 9l-7 7-7-7" /></svg>
            </button>
            <div v-if="showTranscript" class="acc-body">
              <div class="md" v-html="renderMarkdown(detail.transcript)" />
            </div>
          </div>
        </section>

        <div v-if="isEmpty" class="detail-state detail-state--empty">
          {{ detail.isLocal ? 'No notes or transcript yet for this recording.' : 'No notes available for this meeting yet.' }}
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { renderMarkdown } from '../utils/markdown';
import {
  getActiveBackend,
  type MeetingListItem,
  type MeetingDetail,
  type MeetingActionItem,
} from '../composables/useBackend';

const props = defineProps<{ item: MeetingListItem | null }>();

const loading = ref(false);
const error = ref<string | null>(null);
const detail = ref<MeetingDetail | null>(null);
const showFullNotes = ref(false);
const showTranscript = ref(false);

// Monotonic token so a slow load for a previously-selected row can't overwrite
// the detail of the row the user clicked more recently.
let reqId = 0;

async function load(item: MeetingListItem | null): Promise<void> {
  detail.value = null;
  error.value = null;
  showFullNotes.value = false;
  showTranscript.value = false;
  if (!item) return;
  const my = ++reqId;
  loading.value = true;
  try {
    const backend = await getActiveBackend();
    const d = await backend.getMeetingDetail(item);
    if (my !== reqId) return; // a newer selection superseded this load
    detail.value = d;
  } catch (e) {
    if (my !== reqId) return;
    console.error('Failed to load meeting detail', e);
    error.value = 'Could not load this meeting.';
  } finally {
    if (my === reqId) loading.value = false;
  }
}

watch(() => props.item?.id, () => load(props.item), { immediate: true });

const hasCoaching = computed(() => {
  const c = detail.value?.coaching;
  return !!(c && (c.strengths?.length || c.improvements?.length || c.patterns));
});

const isEmpty = computed(() => {
  const d = detail.value;
  if (!d) return false;
  if (d.isLocal) return !d.note && !d.transcript;
  return (
    !d.digest &&
    !d.summary &&
    !d.actionItems.length &&
    d.score === undefined &&
    !hasCoaching.value
  );
});

// Group action items by owner, preserving first-seen order; items without an
// owner fall into a single trailing unnamed group.
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
  return (
    name
      .split(/\s+/)
      .filter(Boolean)
      .map((n) => n[0])
      .join('')
      .toUpperCase()
      .slice(0, 2) || '?'
  );
}

function formatDateTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  });
}

const durationLabel = computed<string | null>(() => {
  const d = detail.value;
  if (!d) return null;
  let secs: number | null = null;
  if (d.durationSeconds != null) {
    secs = d.durationSeconds;
  } else if (d.endAt) {
    const ms = new Date(d.endAt).getTime() - new Date(d.startAt).getTime();
    if (Number.isFinite(ms) && ms > 0 && ms < 24 * 60 * 60 * 1000) secs = ms / 1000;
  }
  if (secs == null) return null;
  const mins = Math.floor(secs / 60);
  const hours = Math.floor(mins / 60);
  const rem = mins % 60;
  return hours > 0 ? `${hours}h ${rem}m` : `${mins}m`;
});
</script>

<style scoped>
.detail {
  height: 100%;
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: #ffffff;
}

/* States */
.detail-state {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 40px 24px;
  color: #86868b;
  font-size: 14px;
}
.detail-state--error { color: #dc2626; }
.detail-state--empty { justify-content: flex-start; }
.spinner {
  width: 18px;
  height: 18px;
  border: 2px solid #e5e7eb;
  border-bottom-color: #6c63c0;
  border-radius: 50%;
  animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

/* Stripe header */
.stripe {
  flex-shrink: 0;
  min-height: 72px;
  padding: 16px 24px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  background-color: #0d0d0d;
  background-repeat: no-repeat;
  background-size: 100% 100%;
}
.stripe--internal {
  background-image: linear-gradient(110deg, transparent 0%, transparent 45%, #facc15 45%, #facc15 72%, transparent 72%, transparent 78%, #6c63c0 78%, #6c63c0 84%, transparent 84%, transparent 100%);
}
.stripe--external {
  background-image: linear-gradient(110deg, transparent 0%, transparent 45%, #facc15 45%, #facc15 72%, transparent 72%, transparent 78%, #ec4899 78%, #ec4899 84%, transparent 84%, transparent 100%);
}
.stripe-titles { min-width: 0; }
.stripe-title {
  margin: 0;
  color: #fff;
  font-size: 18px;
  font-weight: 600;
  line-height: 1.3;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.stripe-date { margin: 2px 0 0; color: rgba(255, 255, 255, 0.8); font-size: 13px; }
.vis-pill {
  flex-shrink: 0;
  padding: 3px 10px;
  background: rgba(255, 255, 255, 0.85);
  color: #1d1d1f;
  font-size: 11px;
  border-radius: 999px;
  text-transform: capitalize;
}

/* Metadata bar */
.meta {
  flex-shrink: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 18px;
  padding: 10px 24px;
  background: #f9fafb;
  border-bottom: 1px solid #e5e7eb;
  font-size: 13px;
}
.meta-item { display: flex; align-items: center; gap: 6px; color: #4b5563; }
.ic { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; }
.ic--purple { color: #6c63c0; width: 18px; height: 18px; }
.ic--gray { color: #6b7280; width: 18px; height: 18px; }

.avatars { display: flex; align-items: center; }
.avatar {
  width: 24px;
  height: 24px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #fff;
  font-size: 10px;
  font-weight: 600;
  border: 2px solid #fff;
  margin-left: -6px;
}
.avatar:first-child { margin-left: 0; }
.avatar--more { background: #9ca3af; }
.avatar--sm { width: 24px; height: 24px; border: none; margin: 0; font-size: 10px; }

.score-pill {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 10px;
  border-radius: 999px;
  font-size: 12px;
  font-weight: 500;
}
.score-dot { width: 8px; height: 8px; border-radius: 50%; }

/* Body */
.body { flex: 1; min-height: 0; overflow-y: auto; padding: 24px; }
.sec { margin-bottom: 24px; }
.sec:last-child { margin-bottom: 0; }
.sec-head { display: flex; align-items: center; gap: 8px; margin-bottom: 12px; }
.sec-head h3 { margin: 0; font-size: 15px; font-weight: 600; color: #111827; }
.count {
  padding: 1px 8px;
  background: rgba(108, 99, 192, 0.1);
  color: #6c63c0;
  font-size: 12px;
  font-weight: 500;
  border-radius: 999px;
}

/* Action items */
.ai-groups { display: flex; flex-direction: column; gap: 16px; }
.ai-owner { display: flex; align-items: center; gap: 8px; margin-bottom: 8px; }
.ai-name { font-size: 14px; font-weight: 500; color: #111827; }
.ai-list { margin: 0; padding-left: 20px; display: flex; flex-direction: column; gap: 6px; color: #374151; font-size: 14px; }
.ai-list--indent { margin-left: 32px; }

/* Accordion (full notes / transcript) */
.acc { border: 1px solid #e5e7eb; border-radius: 8px; overflow: hidden; }
.acc-btn {
  width: 100%;
  padding: 12px 16px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  background: #fff;
  border: none;
  cursor: pointer;
  font-size: 14px;
  font-weight: 500;
  color: #111827;
}
.acc-btn:hover { background: #f9fafb; }
.acc-left { display: flex; align-items: center; gap: 8px; }
.chevron { transition: transform 0.15s; }
.chevron.open { transform: rotate(180deg); }
.acc-body { padding: 16px; background: #f9fafb; border-top: 1px solid #e5e7eb; }

/* Assessment */
.assess-score { display: flex; align-items: center; gap: 12px; margin-bottom: 16px; }
.score-circle {
  width: 48px;
  height: 48px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 20px;
  font-weight: 700;
}
.score-label { font-weight: 600; color: #111827; font-size: 14px; }
.score-sub { font-size: 12px; color: #6b7280; }
.assess-block { margin-top: 12px; font-size: 14px; }
.assess-h { font-weight: 500; color: #374151; margin-bottom: 4px; }
.assess-block p { margin: 0; color: #4b5563; line-height: 1.5; }

/* Coaching */
.coaching {
  background: linear-gradient(135deg, rgba(108, 99, 192, 0.05), rgba(108, 99, 192, 0.1));
  border: 1px solid rgba(108, 99, 192, 0.2);
  border-radius: 8px;
  padding: 16px;
}
.coach-block { margin-top: 14px; }
.coach-block:first-of-type { margin-top: 0; }
.coach-h { font-weight: 500; font-size: 14px; margin-bottom: 6px; }
.coach-h--green { color: #15803d; }
.coach-h--amber { color: #b45309; }
.coach-h--purple { color: #6c63c0; }
.coach-pattern { padding-top: 12px; border-top: 1px solid rgba(108, 99, 192, 0.2); }
.coach-pattern p { margin: 0; color: #374151; font-size: 14px; line-height: 1.5; }
.coach-list { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 6px; color: #374151; font-size: 14px; }
.coach-list li { display: flex; align-items: flex-start; gap: 8px; line-height: 1.45; }
.bullet { flex-shrink: 0; margin-top: 1px; }
.bullet--green { color: #22c55e; }
.bullet--amber { color: #f59e0b; }

/* Rendered markdown (.md) — prose-like styling */
.md { color: #374151; font-size: 14px; line-height: 1.6; }
.md :deep(h1), .md :deep(h2), .md :deep(h3) { color: #111827; font-weight: 600; margin: 16px 0 8px; }
.md :deep(h1) { font-size: 16px; }
.md :deep(h2) { font-size: 15px; }
.md :deep(h3) { font-size: 14px; }
.md :deep(p) { margin: 0 0 10px; }
.md :deep(ul), .md :deep(ol) { margin: 0 0 10px; padding-left: 22px; display: flex; flex-direction: column; gap: 4px; }
.md :deep(li) { line-height: 1.5; }
.md :deep(strong) { font-weight: 600; color: #111827; }
.md :deep(code) { background: #f3f4f6; padding: 1px 5px; border-radius: 4px; font-size: 0.9em; }
.md :deep(a) { color: #6c63c0; text-decoration: underline; }
.md :deep(blockquote) { margin: 0 0 10px; padding-left: 12px; border-left: 3px solid #e5e7eb; color: #6b7280; }
.md :deep(*:last-child) { margin-bottom: 0; }
</style>
