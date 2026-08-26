<template>
  <teleport to="body">
    <div class="sp-overlay" @click="emit('close')" />
    <div class="sp-pop" :style="panelStyle">
      <div class="sp-head">
        <h3>Assign speakers</h3>
        <span class="sp-count">{{ rows.length }} detected</span>
        <button class="sp-close" type="button" aria-label="Close" @click="emit('close')">
          <svg viewBox="0 0 24 24" class="ic"><path d="M6 6l12 12M18 6L6 18" /></svg>
        </button>
      </div>

      <p v-if="assignment.error.value" class="sp-err" role="alert">
        ⚠ {{ assignment.error.value }}
      </p>

      <ul class="sp-list">
        <li v-for="row in rows" :key="row.speaker" class="sp-row">
          <div class="sp-row-main">
            <!-- Labelled rather than a bare glyph: hearing the voice is how a
                 host actually decides who a speaker is, so it earns the same
                 pill treatment as the actions beside it. -->
            <button
              v-if="row.speakerIndex !== null"
              type="button"
              class="sp-play"
              :class="{ playing: assignment.playingSpeakerIndex.value === row.speakerIndex }"
              :aria-label="
                assignment.playingSpeakerIndex.value === row.speakerIndex
                  ? `Stop voice sample for ${row.speaker}`
                  : `Play voice sample for ${row.speaker}`
              "
              @click="assignment.toggleVoiceSample(row.speakerIndex)"
            >
              <svg v-if="assignment.playingSpeakerIndex.value === row.speakerIndex" viewBox="0 0 24 24" class="ic-solid">
                <path d="M6 4h4v16H6zm8 0h4v16h-4z" />
              </svg>
              <svg v-else viewBox="0 0 24 24" class="ic-solid"><path d="M8 5v14l11-7z" /></svg>
              {{ assignment.playingSpeakerIndex.value === row.speakerIndex ? 'Stop' : 'Play' }}
            </button>

            <div class="sp-who">
              <div class="sp-names">
                <span class="sp-speaker">{{ row.speaker }}</span>
                <template v-if="row.displayName !== row.speaker">
                  <span class="sp-arrow">→</span>
                  <span class="sp-name">{{ row.displayName }}</span>
                </template>
                <template v-else-if="row.suggestion">
                  <span class="sp-arrow">→</span>
                  <span class="sp-name sp-name--guess">{{ row.suggestion.name }}</span>
                </template>
                <span
                  v-if="row.suggestion"
                  class="sp-score"
                  :class="{ 'sp-score--weak': !!row.suggestion.outrankedBy }"
                >{{ Math.round(row.suggestion.confidence * 100) }}% match</span>
              </div>
              <!-- Shown rather than hidden: if diarization split one person
                   across two voices, seeing both is how the host notices. -->
              <p v-if="row.suggestion?.outrankedBy" class="sp-outranked">
                {{ row.suggestion.outrankedBy.speakerLabel }} is a stronger match for
                {{ row.suggestion.name }}
                ({{ Math.round(row.suggestion.outrankedBy.confidence * 100) }}%).
                Assign this speaker to use it anyway.
              </p>
            </div>

            <div class="sp-actions">
              <span v-if="row.confirmed" class="sp-confirmed">
                <svg viewBox="0 0 24 24" class="ic"><path d="M5 13l4 4L19 7" /></svg>
                Confirmed
              </span>
              <button
                v-if="row.canConfirm"
                class="sp-btn sp-btn--confirm"
                type="button"
                @click="assignment.confirmSpeaker(row.speaker)"
              >Confirm</button>
              <!-- Offered even once confirmed: a confirmation can be wrong, and
                   re-labelling is the only way to correct it. -->
              <button
                class="sp-btn"
                type="button"
                @click="toggleAssign(row.speaker)"
              >{{ row.assignLabel }}</button>
            </div>
          </div>

          <!-- Inline rather than a floating popover: the panel is narrow and
               its list scrolls, so anything absolutely positioned inside would
               be clipped by the scroll container. -->
          <div v-if="assignment.assigningSpeaker.value === row.speaker" class="sp-assign">
            <input
              ref="assignInput"
              v-model="assignment.searchQuery.value"
              type="text"
              class="sp-input"
              placeholder="Name, email, or teammate…"
              :aria-label="`Assign ${row.speaker} to`"
              @input="assignment.debouncedSearch()"
              @keydown.esc.prevent="assignment.closeAssignment()"
              @keydown.enter.prevent="onAssignEnter"
            />
            <div v-if="assignment.searching.value" class="sp-hint">Searching…</div>
            <ul v-else-if="assignment.searchResults.value.length" class="sp-results">
              <li v-for="member in assignment.searchResults.value" :key="String(member.id)">
                <button class="sp-result" type="button" @click="assignment.assignToMember(member)">
                  <span class="sp-avatar">{{ initials(member.name || member.email) }}</span>
                  <span class="sp-result-text">
                    <span class="sp-result-name">{{ member.name || member.email }}</span>
                    <span v-if="member.name" class="sp-result-email">{{ member.email }}</span>
                  </span>
                </button>
              </li>
            </ul>
            <div v-else-if="assignment.searchQuery.value.trim().length >= 2" class="sp-results">
              <div class="sp-hint">No teammates found</div>
              <button class="sp-result" type="button" @click="assignment.assignToLabel()">
                <span class="sp-avatar sp-avatar--plain">
                  {{ assignment.searchQuery.value.includes('@') ? '@' : '#' }}
                </span>
                <span class="sp-result-text">
                  <span class="sp-result-name">{{ assignment.searchQuery.value.trim() }}</span>
                  <span class="sp-result-email">
                    {{ assignment.searchQuery.value.includes('@') ? 'Use as email' : 'Use as name' }}
                  </span>
                </span>
              </button>
            </div>
          </div>
        </li>
      </ul>
    </div>
  </teleport>
</template>

<script setup lang="ts">
// Speaker → person assignment for an Ariso audio meeting, hung off the
// speakers chip in the meeting's metadata band.
//
// Shaped like the attendees dropdown it sits beside — a fixed panel over a
// full-viewport click-catcher — rather than a centred modal: assigning a
// speaker is a quick correction made while reading the notes, not a task worth
// blacking the card out for. Teleported to <body> so the card's
// `overflow: hidden` can't clip it, exactly like ShareMeetingPopover.
import { computed, nextTick, ref } from 'vue';
import type { SpeakerAssignment } from '../composables/useSpeakerAssignment';

interface AnchorRect { bottom: number; left: number }

const props = defineProps<{ assignment: SpeakerAssignment; anchor: AnchorRect | null }>();
const emit = defineEmits<{ close: [] }>();

const PANEL_WIDTH = 380;

const assignInput = ref<HTMLInputElement[] | HTMLInputElement | null>(null);

const panelStyle = computed<Record<string, string>>(() => {
  const a = props.anchor;
  const width = `${PANEL_WIDTH}px`;
  if (!a) return { position: 'fixed', top: '120px', left: '24px', width };
  const left = Math.max(8, Math.min(a.left, window.innerWidth - PANEL_WIDTH - 8));
  return { position: 'fixed', top: `${a.bottom + 6}px`, left: `${left}px`, width };
});

/**
 * One view-model per speaker row. Resolving the getters here rather than in the
 * template collapses several calls per row down to one each, and puts the row's
 * conditions somewhere the type checker can see them.
 */
const rows = computed(() =>
  props.assignment.assignable.value.map((speaker) => {
    const displayName = props.assignment.getSpeakerDisplayName(speaker);
    const confirmed = props.assignment.confirmed.value.has(speaker);
    const autoMatch = props.assignment.getSpeakerAutoMatch(speaker);
    // The guess is shown only while it is still just a guess.
    const suggestion =
      autoMatch && !props.assignment.assignments.value[speaker] && !confirmed
        ? autoMatch
        : null;
    return {
      speaker,
      speakerIndex: props.assignment.getSpeakerIndex(speaker),
      displayName,
      confirmed,
      suggestion,
      // One-click accept is offered on the strongest match for a person only.
      // A weaker one still shows — two diarized voices matching one person is
      // routine — but accepting both in turn would give them to whichever was
      // clicked last, so taking the weaker one has to go through Assign.
      canConfirm: !confirmed && !!autoMatch && !autoMatch.outrankedBy,
      assignLabel: displayName !== speaker || confirmed ? 'Change' : 'Assign',
    };
  })
);

function toggleAssign(speaker: string): void {
  if (props.assignment.assigningSpeaker.value === speaker) {
    props.assignment.closeAssignment();
    return;
  }
  props.assignment.openAssignment(speaker);
  void nextTick(() => {
    const el = assignInput.value;
    (Array.isArray(el) ? el[0] : el)?.focus();
  });
}

// Enter takes the typed text as-is, but only once searching has settled on
// nothing — otherwise it would assign a raw string over a teammate the host is
// still waiting to see.
function onAssignEnter(): void {
  const q = props.assignment.searchQuery.value.trim();
  if (q.length < 2 || props.assignment.searching.value) return;
  if (props.assignment.searchResults.value.length) return;
  void props.assignment.assignToLabel();
}

function initials(input?: string): string {
  if (!input) return '?';
  return (
    input
      .split(/\s+/)
      .filter(Boolean)
      .map((n) => n[0])
      .join('')
      .toUpperCase()
      .slice(0, 2) || '?'
  );
}
</script>

<style scoped>
.sp-overlay { position: fixed; inset: 0; z-index: 60; }
.sp-pop {
  z-index: 61; background: #fff; border: 1px solid #e5e6e3; border-radius: 12px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.12);
  font-family: 'Polymath', -apple-system, system-ui, sans-serif;
  color: #1c1c1c; max-height: 70vh; display: flex; flex-direction: column; overflow: hidden;
}
.ic { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; }
.ic-solid { width: 11px; height: 11px; fill: currentColor; }

.sp-head { display: flex; align-items: center; gap: 8px; padding: 12px 14px; border-bottom: 1px solid #e5e6e3; }
.sp-head h3 { margin: 0; font-size: 15px; font-weight: 600; }
.sp-count { font-size: 11px; color: #9b9b9b; flex: 1; }
.sp-close { background: none; border: none; padding: 2px; color: #6f6f6f; cursor: pointer; display: flex; }
.sp-close:hover { color: #1c1c1c; }

.sp-err { margin: 0; padding: 8px 14px; background: #fef2f2; border-bottom: 1px solid #fecaca; font-size: 12px; color: #b91c1c; }

.sp-list { list-style: none; margin: 0; padding: 0; overflow-y: auto; flex: 1; min-height: 0; }
.sp-row { padding: 10px 14px; border-bottom: 1px solid #f0eeed; }
.sp-row:last-child { border-bottom: none; }
.sp-row-main { display: flex; align-items: center; gap: 8px; }

.sp-play {
  display: inline-flex; align-items: center; gap: 4px; flex-shrink: 0;
  padding: 3px 9px 3px 7px; border-radius: 999px; border: 1px solid rgba(108, 99, 192, 0.25);
  background: rgba(108, 99, 192, 0.1); color: #6c63c0;
  font-family: inherit; font-size: 11px; font-weight: 600; cursor: pointer;
}
.sp-play:hover { background: rgba(108, 99, 192, 0.18); }
.sp-play.playing { background: #6c63c0; border-color: #6c63c0; color: #fff; }

.sp-who { flex: 1; min-width: 0; }
.sp-names { display: flex; align-items: center; gap: 5px; flex-wrap: wrap; font-size: 13px; }
.sp-speaker { font-weight: 600; flex-shrink: 0; }
.sp-arrow { color: #9b9b9b; }
.sp-name { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.sp-name--guess { font-weight: 500; color: #6f6f6f; }
.sp-score { flex-shrink: 0; font-size: 10px; padding: 1px 6px; border-radius: 999px; color: #15803d; background: #f0fdf4; }
.sp-score--weak { color: #b45309; background: #fffbeb; }
.sp-outranked { margin: 3px 0 0; font-size: 11px; line-height: 1.4; color: #8a8a8a; }

.sp-actions { display: flex; align-items: center; gap: 5px; flex-shrink: 0; }
.sp-confirmed { display: inline-flex; align-items: center; gap: 3px; font-size: 11px; color: #15803d; background: #f0fdf4; padding: 2px 7px; border-radius: 999px; }
.sp-confirmed .ic { width: 11px; height: 11px; }
.sp-btn {
  padding: 3px 9px; border-radius: 999px; border: none; background: #f0eeed; color: #535353;
  font-family: inherit; font-size: 11px; font-weight: 600; cursor: pointer;
}
.sp-btn:hover { background: #e5e3e0; }
.sp-btn--confirm { background: #f0fdf4; color: #15803d; }
.sp-btn--confirm:hover { background: #dcfce7; }

.sp-assign { margin-top: 8px; }
.sp-input { width: 100%; height: 32px; padding: 0 10px; border: 1px solid #d6d6d6; border-radius: 8px; font-family: inherit; font-size: 13px; }
.sp-input:focus { outline: none; border-color: #6c63c0; }
.sp-hint { padding: 6px 2px; font-size: 11px; color: #9b9b9b; }
.sp-results { list-style: none; margin: 4px 0 0; padding: 0; max-height: 160px; overflow-y: auto; }
.sp-result { width: 100%; display: flex; align-items: center; gap: 8px; padding: 6px 8px; background: none; border: none; border-radius: 8px; font-family: inherit; text-align: left; cursor: pointer; }
.sp-result:hover { background: #f7f6f4; }
.sp-avatar { width: 24px; height: 24px; border-radius: 50%; background: #6c63c0; color: #fff; font-size: 10px; font-weight: 600; display: flex; align-items: center; justify-content: center; flex-shrink: 0; }
.sp-avatar--plain { background: #ecebe8; color: #6f6f6f; }
.sp-result-text { display: flex; flex-direction: column; min-width: 0; }
.sp-result-name { font-size: 13px; color: #1f1f1f; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.sp-result-email { font-size: 11px; color: #9b9b9b; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
