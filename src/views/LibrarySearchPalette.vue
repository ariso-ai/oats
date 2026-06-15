<template>
  <Teleport to="body">
    <Transition name="palette-fade">
      <div v-if="open" class="palette-backdrop" @mousedown.self="close">
        <div ref="panelRef" class="palette-panel" role="dialog" aria-modal="true" aria-label="Search notes">
          <div class="palette-input-row">
            <input
              ref="inputRef"
              v-model="query"
              class="palette-input"
              type="text"
              placeholder="Search notes"
              autocomplete="off"
              spellcheck="false"
              @keydown.down.prevent="moveActive(1)"
              @keydown.up.prevent="moveActive(-1)"
              @keydown.enter.prevent="activateSelected"
              @keydown.esc.prevent="close"
            />
            <button class="esc-chip" type="button" aria-label="Close search" @click="close">ESC</button>
          </div>

          <div class="palette-content">
            <button
              class="action-row"
              type="button"
              :class="{ active: activeIndex === 0 }"
              @mouseenter="activeIndex = 0"
              @click="focusInput"
            >
              <svg class="row-icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">
                <circle cx="11" cy="11" r="7" stroke="currentColor" stroke-width="2" />
                <path d="m16.5 16.5 4 4" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
              </svg>
              <span>Search notes</span>
              <span v-if="query.trim().length >= minQueryLength" class="row-count">{{ resultLabel }}</span>
            </button>

            <div class="section-label">Go to</div>
            <button
              class="nav-row"
              type="button"
              :class="{ active: activeIndex === 1 }"
              @mouseenter="activeIndex = 1"
              @click="goToNotes"
            >
              <svg class="row-icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">
                <path d="M3 10.5 12 4l9 6.5V20a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z" stroke="currentColor" stroke-width="2" stroke-linejoin="round" />
              </svg>
              <span>My notes</span>
            </button>

            <div class="section-label">More results</div>
            <div v-if="query.trim().length < minQueryLength" class="empty-row">Type at least {{ minQueryLength }} characters.</div>
            <div v-else-if="loading" class="empty-row">Searching…</div>
            <div v-else-if="error" class="empty-row error">{{ error }}</div>
            <div v-else-if="results.length === 0" class="empty-row">No results.</div>
            <button
              v-for="(result, i) in results"
              :key="result.id"
              class="result-row"
              type="button"
              :class="{ active: activeIndex === resultIndex(i) }"
              @mouseenter="activeIndex = resultIndex(i)"
              @click="selectResult(result)"
            >
              <svg class="row-icon doc-icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">
                <path d="M6 3h8l4 4v14H6z" stroke="currentColor" stroke-width="2" stroke-linejoin="round" />
                <path d="M14 3v5h5" stroke="currentColor" stroke-width="2" stroke-linejoin="round" />
              </svg>
              <span class="result-copy">
                <span class="result-title">
                  <template v-for="(part, partIndex) in highlightParts(result.title)" :key="partIndex">
                    <mark v-if="part.match">{{ part.text }}</mark>
                    <span v-else>{{ part.text }}</span>
                  </template>
                </span>
                <span v-if="previewText(result)" class="result-snippet">
                  <template v-for="(part, partIndex) in highlightParts(previewText(result))" :key="partIndex">
                    <mark v-if="part.match">{{ part.text }}</mark>
                    <span v-else>{{ part.text }}</span>
                  </template>
                </span>
              </span>
              <span class="result-date">{{ formatDate(result.timestamp) }}</span>
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import type { MeetingListItem } from '../composables/useBackend';

const minQueryLength = 2;
const debounceMs = 180;

const props = defineProps<{
  open: boolean;
  searchMeetings: (query: string) => Promise<MeetingListItem[]>;
}>();

const emit = defineEmits<{
  close: [];
  select: [meeting: MeetingListItem];
  goToNotes: [];
}>();

const query = ref('');
const results = ref<MeetingListItem[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const activeIndex = ref(0);
const inputRef = ref<HTMLInputElement | null>(null);
const panelRef = ref<HTMLElement | null>(null);
let debounceTimer: number | undefined;
let searchRequestId = 0;

const resultLabel = computed(() => {
  if (loading.value) return 'Searching';
  return `${results.value.length} ${results.value.length === 1 ? 'result' : 'results'}`;
});

// Palette rows share one active index: search action, "My notes", then remote
// results. This keeps arrow/Enter behavior simple without a command framework.
function resultIndex(i: number): number {
  return i + 2;
}

function maxActiveIndex(): number {
  return Math.max(1, results.value.length + 1);
}

function focusInput(): void {
  inputRef.value?.focus();
}

function close(): void {
  emit('close');
}

function goToNotes(): void {
  emit('goToNotes');
}

function selectResult(result: MeetingListItem): void {
  emit('select', result);
}

function moveActive(delta: number): void {
  const max = maxActiveIndex();
  activeIndex.value = (activeIndex.value + delta + max + 1) % (max + 1);
}

function activateSelected(): void {
  if (activeIndex.value === 0) {
    focusInput();
    return;
  }
  if (activeIndex.value === 1) {
    goToNotes();
    return;
  }
  const result = results.value[activeIndex.value - 2];
  if (result) selectResult(result);
}

function previewText(result: MeetingListItem): string {
  return (result.snippet || result.matchedText || '').trim();
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

// Highlighting is deliberately local and display-only. It improves the
// Granola-like feel while the real search ranking remains owned by the backend.
function highlightParts(text: string): Array<{ text: string; match: boolean }> {
  const needle = query.value.trim();
  if (!needle) return [{ text, match: false }];
  const lower = text.toLowerCase();
  const target = needle.toLowerCase();
  const parts: Array<{ text: string; match: boolean }> = [];
  let cursor = 0;
  let index = lower.indexOf(target);
  while (index >= 0) {
    if (index > cursor) parts.push({ text: text.slice(cursor, index), match: false });
    parts.push({ text: text.slice(index, index + needle.length), match: true });
    cursor = index + needle.length;
    index = lower.indexOf(target, cursor);
  }
  if (cursor < text.length) parts.push({ text: text.slice(cursor), match: false });
  return parts.length ? parts : [{ text, match: false }];
}

async function runSearch(term: string): Promise<void> {
  const requestId = ++searchRequestId;
  loading.value = true;
  error.value = null;
  try {
    const next = await props.searchMeetings(term);
    if (requestId !== searchRequestId) return;
    results.value = next;
    activeIndex.value = 0;
  } catch (e) {
    if (requestId !== searchRequestId) return;
    console.error('Failed to search meetings', e);
    error.value = 'Search failed.';
    results.value = [];
  } finally {
    if (requestId === searchRequestId) loading.value = false;
  }
}

watch(
  () => props.open,
  async (open) => {
    if (!open) return;
    activeIndex.value = 0;
    await nextTick();
    focusInput();
  }
);

watch(query, (next) => {
  if (debounceTimer !== undefined) clearTimeout(debounceTimer);
  const term = next.trim();
  if (term.length < minQueryLength) {
    searchRequestId++;
    loading.value = false;
    error.value = null;
    results.value = [];
    activeIndex.value = 0;
    return;
  }
  debounceTimer = window.setTimeout(() => {
    void runSearch(term);
  }, debounceMs);
});

function onDocumentKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape' && props.open) {
    event.preventDefault();
    close();
  }
}

onMounted(() => {
  document.addEventListener('keydown', onDocumentKeydown);
});

onUnmounted(() => {
  if (debounceTimer !== undefined) clearTimeout(debounceTimer);
  document.removeEventListener('keydown', onDocumentKeydown);
});
</script>

<style scoped>
.palette-backdrop {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding-top: 46px;
  background: rgba(247, 246, 244, 0.72);
  backdrop-filter: blur(10px);
}
.palette-panel {
  width: min(980px, calc(100vw - 48px));
  max-height: min(720px, calc(100vh - 92px));
  overflow: hidden;
  border: 1px solid #d6d6d6;
  border-radius: 16px;
  background: #f7f6f4;
  box-shadow: 0 24px 80px rgba(28, 28, 28, 0.18);
  color: #1c1c1c;
  font-family: 'Polymath', -apple-system, system-ui, sans-serif;
}
.palette-input-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 22px 30px;
  border-bottom: 1px solid #e5e6e3;
}
.palette-input {
  min-width: 0;
  flex: 1;
  border: 0;
  outline: none;
  background: transparent;
  color: #1c1c1c;
  font: inherit;
  font-size: 25px;
  line-height: 1.25;
}
.palette-input::placeholder { color: #9a9a96; }
.esc-chip {
  border: 1px solid #d6d6d6;
  border-radius: 8px;
  background: #ffffff;
  color: #6f6f6f;
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  padding: 5px 8px;
  cursor: pointer;
}
.esc-chip:hover {
  color: #1c1c1c;
  background: #fbfbfb;
}
.palette-content {
  overflow-y: auto;
  max-height: calc(min(720px, calc(100vh - 92px)) - 83px);
  padding: 16px;
}
.action-row,
.nav-row,
.result-row {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 16px;
  border: 0;
  border-radius: 12px;
  background: transparent;
  color: #1c1c1c;
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.action-row {
  min-height: 72px;
  padding: 0 22px;
  font-size: 24px;
  font-weight: 600;
}
.action-row.active,
.nav-row.active,
.result-row.active,
.action-row:hover,
.nav-row:hover,
.result-row:hover {
  background: #ffffff;
  box-shadow: 1px 1px 0 #e5e6e3;
}
.row-icon {
  width: 24px;
  height: 24px;
  flex: 0 0 auto;
  color: #8f8c87;
}
.doc-icon { color: #9a9a96; }
.row-count {
  margin-left: auto;
  color: #6f6f6f;
  font-size: 22px;
  font-weight: 500;
}
.section-label {
  padding: 18px 16px 10px;
  color: #9a9a96;
  font-size: 20px;
  font-weight: 700;
}
.nav-row {
  min-height: 50px;
  padding: 0 22px;
  font-size: 22px;
  font-weight: 600;
}
.empty-row {
  padding: 14px 22px 20px 22px;
  color: #6f6f6f;
  font-size: 16px;
}
.empty-row.error { color: #dc2626; }
.result-row {
  min-height: 84px;
  padding: 9px 22px;
}
.result-copy {
  min-width: 0;
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 6px;
}
.result-title {
  overflow: hidden;
  color: #1c1c1c;
  font-size: 21px;
  font-weight: 600;
  line-height: 1.2;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.result-snippet {
  overflow: hidden;
  color: #6f6f6f;
  font-size: 18px;
  line-height: 1.25;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.result-date {
  flex: 0 0 auto;
  color: #6f6f6f;
  font-size: 20px;
}
mark {
  border-radius: 2px;
  background: #f7efdc;
  color: inherit;
}
.palette-fade-enter-active,
.palette-fade-leave-active {
  transition: opacity 0.12s ease;
}
.palette-fade-enter-from,
.palette-fade-leave-to {
  opacity: 0;
}
</style>
