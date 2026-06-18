<template>
  <div class="up-next">
    <!-- Big serif date/time heading for the empty home state. -->
    <p class="greeting">{{ greeting }}</p>
    <!-- Greeting prompt: oats mark + chat bubble, with the impromptu-record CTA
         tucked under the bubble's bottom-right corner. -->
    <div class="prompt">
      <img class="prompt-logo" :src="oatsLogo" alt="oats" />
      <div class="prompt-body">
        <p class="prompt-bubble">Ready for the next meet? Or do you want to spin up an impromptu meeting?</p>
        <button class="impromptu-btn" type="button" title="Start an impromptu recording" @click="$emit('record')">
          <span class="impromptu-label">Impromptu Meeting</span>
          <span class="impromptu-icon" aria-hidden="true">
            <svg viewBox="0 0 16 16">
              <circle cx="8" cy="8" r="6.5" fill="none" stroke="#e0443e" stroke-width="1.5" />
              <circle cx="8" cy="8" r="3.5" fill="#e0443e" />
            </svg>
          </span>
        </button>
      </div>
    </div>

    <template v-if="featured">
      <!-- Today is clear — say so, then show the next day's meetings below. -->
      <div v-if="showingNextDay" class="up-next-empty up-next-empty--notice">
        <p>No upcoming meetings today.</p>
      </div>

      <!-- "Up Next • in 22min" label, the day's date, and prev/next navigation
           across the upcoming meetings. -->
      <div class="up-next-head">
        <div class="up-next-head-main">
          <span class="up-next-label">Up Next<template v-if="featuredRel"> • {{ featuredRel }}</template></span>
          <span class="up-next-day">{{ dayLabel }}</span>
        </div>
        <div v-if="upcoming.length > 1" class="up-next-nav">
          <button
            class="nav-chevron"
            type="button"
            aria-label="Previous meeting"
            :disabled="safeIndex === 0"
            @click="step(-1)"
          >
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M10 3 5 8l5 5" /></svg>
          </button>
          <button
            class="nav-chevron"
            type="button"
            aria-label="Next meeting"
            :disabled="safeIndex >= upcoming.length - 1"
            @click="step(1)"
          >
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 3l5 5-5 5" /></svg>
          </button>
        </div>
      </div>

      <div class="card">
        <!-- Featured next meeting: time + title on the left, attendees on the
             right, then the one actionable control we can back with data. -->
        <header class="card-head">
          <div class="head-top">
            <div class="head-titles">
              <p class="head-time">{{ fmtClock(featured.timestamp) }}</p>
              <button
                class="head-title"
                type="button"
                title="Open meeting"
                @click="$emit('select', featured)"
              >{{ featured.title }}</button>
            </div>
            <div v-if="attendees.length" class="avatars">
              <template v-for="(p, i) in attendees.slice(0, 3)" :key="i">
                <img
                  v-if="p.avatarUrl"
                  class="avatar"
                  :src="p.avatarUrl"
                  :alt="p.name || p.email || ''"
                  :title="p.name || p.email || ''"
                />
                <span
                  v-else
                  class="avatar"
                  :style="{ background: avatarColor(i) }"
                  :title="p.name || p.email || ''"
                >{{ initials(p.name || p.email) }}</span>
              </template>
              <span v-if="attendees.length > 3" class="avatar avatar--more">+{{ attendees.length - 3 }}</span>
            </div>
          </div>
          <div class="head-actions">
            <button class="action-btn" type="button" @click="$emit('start', featured)">
              Start Meeting Early
            </button>
          </div>
        </header>

        <!-- Later upcoming meetings, name on the left, time on the right. -->
        <button
          v-for="m in visibleRest"
          :key="m.id"
          class="meeting-row"
          type="button"
          @click="$emit('select', m)"
        >
          <span class="row-title">{{ m.title }}</span>
          <span class="row-sub">{{ rowSub(m) }}</span>
        </button>
        <div v-if="moreCount > 0" class="meeting-row meeting-row--more">
          {{ moreCount }} more…
        </div>
      </div>

      <!-- Compact preview of the next day's meetings, beneath today's card. -->
      <template v-if="nextDayPreview">
        <div class="next-day-head">{{ nextDayPreview.label }}</div>
        <div class="card card--compact">
          <button
            v-for="m in visibleNextDay"
            :key="m.id"
            class="meeting-row"
            type="button"
            @click="$emit('select', m)"
          >
            <span class="row-title">{{ m.title }}</span>
            <span class="row-sub">{{ rowSub(m) }}</span>
          </button>
          <div v-if="nextDayMore > 0" class="meeting-row meeting-row--more">
            {{ nextDayMore }} more…
          </div>
        </div>
      </template>
    </template>

    <div v-else class="up-next-empty">
      <p>No upcoming meetings.</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import {
  getActiveBackend,
  type MeetingListItem,
  type MeetingParticipantInfo,
} from '../composables/useBackend';
import {
  groupTodaysMeetings,
  nextDaySection,
  todayLabel,
  upcomingRelLabel,
} from '../composables/groupMeetingsByDate';
import oatsLogo from '../assets/oats-light.svg';

const props = defineProps<{
  meetings: MeetingListItem[];
  now: Date;
}>();

defineEmits<{
  (e: 'select', meeting: MeetingListItem): void;
  (e: 'start', meeting: MeetingListItem): void;
  (e: 'record'): void;
}>();

// How many later meetings to list before collapsing into "N more…".
const MAX_VISIBLE_REST = 4;

// Avatar palette shared with MeetingDetailView so a person keeps the same
// colour across the app.
const AVATAR_COLORS = ['#6c63c0', '#0ea5e9', '#f59e0b', '#ec4899', '#22c55e', '#64748b'];
function avatarColor(i: number): string {
  return AVATAR_COLORS[i % AVATAR_COLORS.length];
}
function initials(name?: string): string {
  if (!name) return '?';
  return name.split(/\s+/).filter(Boolean).map((n) => n[0]).join('').toUpperCase().slice(0, 2) || '?';
}

// "Tuesday June 9 1:42PM" — weekday, month, day, and the current clock with no
// space before AM/PM, matching the Figma heading.
const greeting = computed(() => {
  const d = props.now;
  const weekday = d.toLocaleDateString(undefined, { weekday: 'long' });
  const month = d.toLocaleDateString(undefined, { month: 'long' });
  const time = d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' }).replace(/\s+/g, '');
  return `${weekday} ${month} ${d.getDate()} ${time}`;
});

// Today's still-to-come meetings, matching the sidebar's "today / upcoming"
// split: future meetings and ones in progress, ordered earliest-first.
const todaysUpcoming = computed<MeetingListItem[]>(() => {
  const section = groupTodaysMeetings(props.meetings, props.now).find((s) => s.key === 'upcoming');
  return section?.items ?? [];
});

// The next calendar day that has meetings. Shown either as the main card (when
// today is already done) or as a compact preview beneath today's card.
const nextDay = computed(() => nextDaySection(props.meetings, props.now));
const showingNextDay = computed(() => todaysUpcoming.value.length === 0 && nextDay.value !== null);

// The meetings the main card pages through: today's upcoming, else the next day's.
const upcoming = computed<MeetingListItem[]>(() =>
  todaysUpcoming.value.length ? todaysUpcoming.value : (nextDay.value?.items ?? [])
);

// Date heading for whichever day the main card is showing.
const dayLabel = computed(() =>
  showingNextDay.value ? nextDay.value!.label : todayLabel(props.now)
);

// Next day's meetings as a compact preview below today's card — only when today
// has its own meetings (otherwise the next day already fills the main card).
const nextDayPreview = computed(() =>
  showingNextDay.value ? null : nextDay.value
);
const visibleNextDay = computed(() => nextDayPreview.value?.items.slice(0, MAX_VISIBLE_REST) ?? []);
const nextDayMore = computed(
  () => (nextDayPreview.value?.items.length ?? 0) - visibleNextDay.value.length
);

// Which upcoming meeting is featured in the header; the chevrons page through
// them. Clamp on read so a shrinking list (a meeting starting) never points
// past the end.
const featuredIndex = ref(0);
const safeIndex = computed(() =>
  upcoming.value.length === 0 ? 0 : Math.min(featuredIndex.value, upcoming.value.length - 1)
);
const featured = computed<MeetingListItem | null>(() => upcoming.value[safeIndex.value] ?? null);
const featuredRel = computed(() =>
  featured.value ? upcomingRelLabel(featured.value, props.now).replace(/^in /, '') : ''
);

// Everything after the featured meeting fills the list, capped with a "N more…"
// tail so the card never runs away.
const rest = computed(() => upcoming.value.slice(safeIndex.value + 1));
const visibleRest = computed(() => rest.value.slice(0, MAX_VISIBLE_REST));
const moreCount = computed(() => rest.value.length - visibleRest.value.length);

// Attendees aren't on the list item — only on the meeting detail — so fetch the
// featured meeting's detail to surface them. Guarded against races as the
// chevrons page between meetings.
const attendees = ref<MeetingParticipantInfo[]>([]);
let attendeesReq = 0;
watch(
  () => featured.value?.id ?? null,
  async (id) => {
    attendees.value = [];
    const meeting = featured.value;
    if (!id || !meeting) return;
    const my = ++attendeesReq;
    try {
      const backend = await getActiveBackend();
      const detail = await backend.getMeetingDetail(meeting);
      if (my === attendeesReq) attendees.value = detail.participants ?? [];
    } catch (e) {
      if (my === attendeesReq) attendees.value = [];
      console.warn('Up Next: failed to load attendees', e);
    }
  },
  { immediate: true }
);

// Reset paging when the card switches between today and the next day, so the
// chevrons don't start mid-list against a freshly-swapped set of meetings.
watch(
  () => (showingNextDay.value ? nextDay.value?.key ?? null : 'today'),
  () => { featuredIndex.value = 0; }
);

function step(delta: number): void {
  const next = safeIndex.value + delta;
  if (next < 0 || next > upcoming.value.length - 1) return;
  featuredIndex.value = next;
}

function fmtClock(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime())
    ? iso
    : d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
}

// "4:00 PM • 45min" — duration from the explicit field (local recordings) or
// derived from the start/end span (scheduled meetings); omitted when unknown.
function fmtDuration(m: MeetingListItem): string {
  let mins: number | null = null;
  if (m.durationSeconds != null) {
    mins = Math.max(1, Math.round(m.durationSeconds / 60));
  } else if (m.endTimestamp) {
    const span = new Date(m.endTimestamp).getTime() - new Date(m.timestamp).getTime();
    if (!Number.isNaN(span) && span > 0) mins = Math.max(1, Math.round(span / 60_000));
  }
  if (mins == null) return '';
  const hours = Math.floor(mins / 60);
  const rem = mins % 60;
  if (hours && rem) return `${hours}hr ${rem}min`;
  if (hours) return `${hours}hr`;
  return `${mins}min`;
}

function rowSub(m: MeetingListItem): string {
  const dur = fmtDuration(m);
  return dur ? `${fmtClock(m.timestamp)} • ${dur}` : fmtClock(m.timestamp);
}
</script>

<style scoped>
.up-next {
  width: 100%;
  max-width: 560px;
  margin: 0 auto;
  align-self: flex-start;
  max-height: 100%;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  min-height: 0;
}

/* Display-face date/time heading, centered across the detail area. The top
   padding drops it onto the same line as the sidebar's start-recording button
   (the sidebar has 12px more top padding than the detail area). */
.greeting {
  font-family: 'Instrument Serif', Georgia, 'Times New Roman', serif;
  font-size: clamp(34px, 3.6vw, 48px);
  font-weight: 400;
  line-height: 1.05;
  color: #1c1c1c;
  text-align: center;
  padding: 14px 16px 8px;
  flex-shrink: 0;
}

/* Prompt sitting between the date heading and the Up Next card: oats mark on
   the left, a chat bubble, and the record CTA tucked under it. */
.prompt {
  display: flex;
  align-items: flex-start;
  gap: 14px;
  padding: 6px 2px 26px;
  flex-shrink: 0;
}
.prompt-logo {
  width: 52px;
  height: 52px;
  flex-shrink: 0;
  margin-top: 2px;
}
.prompt-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}
.prompt-bubble {
  background: #ecebe8;
  border-radius: 8px 20px 20px 20px;
  padding: 14px 20px;
  font-size: 15px;
  line-height: 1.4;
  color: #1c1c1c;
}

/* Primary split CTA — black "Impromptu Meeting" label + record-dot segment,
   overlapping the bubble's bottom-right corner. */
.impromptu-btn {
  align-self: flex-end;
  position: relative;
  z-index: 1;
  margin-top: -16px;
  display: inline-flex;
  align-items: stretch;
  padding: 0;
  border: none;
  border-radius: 12px;
  background: #1c1c1c;
  box-shadow: 2px 2px 0 rgba(0, 0, 0, 0.12);
  overflow: hidden;
  cursor: pointer;
  font-family: inherit;
  transition: transform 0.1s, box-shadow 0.1s;
}
.impromptu-btn:hover { box-shadow: 1px 1px 0 rgba(0, 0, 0, 0.12); transform: translate(1px, 1px); }
.impromptu-label {
  display: flex;
  align-items: center;
  padding: 8px 12px 8px 16px;
  font-size: 14px;
  font-weight: 600;
  color: #ffffff;
  white-space: nowrap;
}
.impromptu-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 8px;
  border-left: 1px solid rgba(255, 255, 255, 0.22);
}
.impromptu-icon svg { width: 16px; height: 16px; display: block; }

/* "Up Next • …" label row with the prev/next chevrons. */
.up-next-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 0 2px 10px;
  flex-shrink: 0;
}
.up-next-head-main {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.up-next-label {
  font-size: 14px;
  font-weight: 600;
  color: #6f6f6f;
}
.up-next-day {
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.04em;
  color: #9a9a9a;
}
.up-next-nav {
  display: flex;
  align-items: center;
  gap: 2px;
}
.nav-chevron {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  padding: 0;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: #8b8b8b;
  cursor: pointer;
  transition: background 0.12s, color 0.12s;
}
.nav-chevron:hover:not(:disabled) { background: #ecebe8; color: #1c1c1c; }
.nav-chevron:disabled { opacity: 0.35; cursor: default; }
.nav-chevron svg { width: 15px; height: 15px; fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }

/* The card itself. */
.card {
  background: #ffffff;
  border: 1px solid #e5e6e3;
  border-radius: 16px;
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
}

/* Compact preview card for the next day: just rows, with a little breathing
   room so the first/last rows sit clear of the rounded corners. */
.card--compact {
  padding: 6px 0;
  overflow: hidden;
}

/* Date heading above the next day's compact preview. */
.next-day-head {
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.04em;
  color: #9a9a9a;
  padding: 0 2px 8px;
  margin-top: 18px;
  flex-shrink: 0;
}

.card-head {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 24px;
  border-bottom: 1px solid #e5e6e3;
}
.head-top {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}
.head-titles {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}
.head-time {
  font-size: 14px;
  color: #6f6f6f;
}
.head-title {
  align-self: flex-start;
  max-width: 100%;
  padding: 0;
  border: none;
  background: transparent;
  font-family: inherit;
  font-size: 20px;
  font-weight: 600;
  color: #1c1c1c;
  text-align: left;
  text-decoration: underline;
  text-underline-offset: 2px;
  cursor: pointer;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.head-title:hover { color: #000000; }

/* Overlapping attendee avatars, mirroring MeetingDetailView. */
.avatars {
  display: flex;
  align-items: center;
  flex-shrink: 0;
}
.avatar {
  width: 23px;
  height: 23px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #fff;
  font-size: 9px;
  font-weight: 600;
  border: 2px solid #ffffff;
  margin-left: -5px;
  object-fit: cover;
  overflow: hidden;
  box-sizing: border-box;
}
.avatar:first-child { margin-left: 0; }
.avatar--more {
  width: 24px;
  height: 24px;
  background: #ecebe8;
  border: 1px solid #d6d6d6;
  color: #6f6f6f;
  font-size: 10px;
  font-weight: 400;
}

.head-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}
.action-btn {
  display: inline-flex;
  align-items: center;
  padding: 8px 12px;
  border: 1px solid #d6d6d6;
  border-radius: 12px;
  background: #ffffff;
  box-shadow: 2px 2px 0 #d6d6d6;
  font-family: inherit;
  font-size: 14px;
  font-weight: 600;
  color: #1a1a1a;
  cursor: pointer;
  transition: transform 0.1s, box-shadow 0.1s;
}
.action-btn:hover { box-shadow: 1px 1px 0 #d6d6d6; transform: translate(1px, 1px); }

/* Later upcoming meetings: title left, time right. */
.meeting-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  width: 100%;
  padding: 12px 24px;
  border: none;
  background: transparent;
  font-family: inherit;
  text-align: left;
  cursor: pointer;
  transition: background 0.12s;
}
.meeting-row:hover { background: rgba(0, 0, 0, 0.03); }
.row-title {
  font-size: 14px;
  color: #1c1c1c;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.row-sub {
  flex-shrink: 0;
  font-size: 14px;
  color: #6f6f6f;
}
.meeting-row--more {
  justify-content: center;
  color: #6f6f6f;
  cursor: default;
}
.meeting-row--more:hover { background: transparent; }

.up-next-empty {
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
/* Compact variant shown above the next day's card when today is already done. */
.up-next-empty--notice {
  flex: 0 0 auto;
  justify-content: flex-start;
  padding: 14px 16px;
  margin-bottom: 12px;
}
</style>
