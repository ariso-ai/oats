<template>
  <teleport to="body">
    <div class="search-overlay" @click="close">
      <div class="search-card" role="dialog" aria-label="Search meetings" @click.stop>
        <div class="search-field">
          <svg class="search-ic" width="16" height="16" viewBox="0 0 18 18" fill="none" aria-hidden="true">
            <circle cx="7.75" cy="7.75" r="5" stroke="currentColor" stroke-width="1.6" />
            <line x1="11.6" y1="11.6" x2="15.5" y2="15.5" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
          </svg>
          <input
            ref="inputEl"
            v-model="query"
            class="search-input"
            type="text"
            placeholder="Search meetings"
            spellcheck="false"
            autocomplete="off"
            @keydown.down.prevent="move(1)"
            @keydown.up.prevent="move(-1)"
            @keydown.enter.prevent="choose(results[highlight])"
            @keydown.esc.prevent="close"
          />
        </div>

        <div v-if="results.length" class="search-results">
          <button
            v-for="(m, i) in results"
            :key="m.id"
            class="search-result"
            :class="{ highlighted: i === highlight }"
            type="button"
            @click="choose(m)"
            @mousemove="highlight = i"
          >
            <span class="sr-title">{{ m.title }}</span>
            <span class="sr-sub">{{ subFor(m) }}</span>
          </button>
        </div>
        <p v-else class="search-empty">{{ query ? 'No matching meetings.' : 'No meetings yet.' }}</p>
      </div>
    </div>
  </teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted } from 'vue';
import type { MeetingListItem } from '../composables/useBackend';

const props = defineProps<{ meetings: MeetingListItem[] }>();
const emit = defineEmits<{ (e: 'select', m: MeetingListItem): void; (e: 'close'): void }>();

const MAX_RESULTS = 50;

const query = ref('');
const highlight = ref(0);
const inputEl = ref<HTMLInputElement | null>(null);

// Title-only, case-insensitive substring match over the already-loaded list.
const results = computed<MeetingListItem[]>(() => {
  const q = query.value.trim().toLowerCase();
  const list = q
    ? props.meetings.filter((m) => m.title.toLowerCase().includes(q))
    : props.meetings;
  return list.slice(0, MAX_RESULTS);
});

// Keep the highlight in range as the result set shrinks/grows.
watch(results, () => {
  if (highlight.value >= results.value.length) highlight.value = 0;
});

function move(delta: number): void {
  const n = results.value.length;
  if (!n) return;
  highlight.value = (highlight.value + delta + n) % n;
}

function choose(m: MeetingListItem | undefined): void {
  if (m) emit('select', m);
}

function close(): void {
  emit('close');
}

function subFor(m: MeetingListItem): string {
  const d = new Date(m.timestamp);
  const time = Number.isNaN(d.getTime())
    ? m.timestamp
    : d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });
  if (m.durationSeconds != null) return `${time} • ${Math.max(1, Math.round(m.durationSeconds / 60))}min`;
  return time;
}

onMounted(() => {
  void nextTick(() => inputEl.value?.focus());
});
</script>

<style scoped>
.search-overlay {
  position: fixed;
  inset: 0;
  z-index: 70;
  display: flex;
  justify-content: center;
  align-items: flex-start;
  padding-top: 96px;
  background: rgba(28, 28, 28, 0.18);
}
.search-card {
  width: 460px;
  max-width: calc(100vw - 48px);
  background: #ffffff;
  border: 1px solid #e5e6e3;
  border-radius: 14px;
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.18);
  overflow: hidden;
  font-family: 'Polymath', -apple-system, system-ui, sans-serif;
  color: #1c1c1c;
}
.search-field {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 14px 16px;
  border-bottom: 1px solid #eceae6;
}
.search-ic { color: #8a8a86; flex-shrink: 0; }
.search-input {
  flex: 1;
  border: none;
  outline: none;
  background: transparent;
  font-family: inherit;
  font-size: 16px;
  color: #1c1c1c;
}
.search-input::placeholder { color: #9a9a96; }

.search-results {
  max-height: 360px;
  overflow-y: auto;
  padding: 6px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.search-results::-webkit-scrollbar { width: 6px; }
.search-results::-webkit-scrollbar-thumb { background: #d6d6d6; border-radius: 3px; }

.search-result {
  display: flex;
  flex-direction: column;
  gap: 3px;
  text-align: left;
  width: 100%;
  padding: 9px 12px;
  border: none;
  border-radius: 10px;
  background: transparent;
  cursor: pointer;
}
.search-result.highlighted { background: rgba(0, 0, 0, 0.05); }
.sr-title {
  font-size: 15px;
  font-weight: 500;
  color: #1c1c1c;
  line-height: 1.25;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.sr-sub { font-size: 12px; color: #6f6f6f; }

.search-empty {
  padding: 22px 16px;
  margin: 0;
  font-size: 14px;
  color: #6f6f6f;
  text-align: center;
}
</style>
