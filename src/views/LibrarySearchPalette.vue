<template>
  <Teleport to="body">
    <Transition name="palette-fade">
      <div v-if="open" class="palette-backdrop" @mousedown.self="close">
        <div
          ref="panelRef"
          class="palette-panel"
          role="dialog"
          aria-modal="true"
          aria-label="Search notes"
          @keydown.tab="onPanelTab"
        >
          <div class="palette-input-row">
            <svg class="input-icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">
              <circle cx="11" cy="11" r="7" stroke="currentColor" stroke-width="2" />
              <path d="m16.5 16.5 4 4" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
            </svg>
            <input
              ref="inputRef"
              v-model="query"
              class="palette-input"
              type="text"
              placeholder="Search"
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
              v-if="homeCommandVisible"
              class="command-row"
              type="button"
              :class="{ active: activeIndex === 0 }"
              @mouseenter="activeIndex = 0"
              @click="goToNotes"
            >
              <svg class="row-icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">
                <path d="M3 10.5 12 4l9 6.5V20a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z" stroke="currentColor" stroke-width="2" stroke-linejoin="round" />
              </svg>
              <span>Home</span>
            </button>

            <div v-if="query.trim().length >= minQueryLength && loading" class="empty-row">Searching…</div>
            <div v-else-if="error" class="empty-row error">{{ error }}</div>
            <div v-else-if="query.trim().length >= minQueryLength && results.length === 0" class="empty-row">No results.</div>
            <button
              v-for="(result, i) in results"
              :key="result.id"
              class="result-row"
              type="button"
              :class="{ active: activeIndex === resultIndex(i) }"
              @mouseenter="activeIndex = resultIndex(i)"
              @click="selectResult(result)"
            >
              <svg class="row-icon meeting-icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">
                <rect x="4" y="5" width="12" height="14" rx="2" stroke="currentColor" stroke-width="2" />
                <path d="m16 10 4-2.5v9L16 14" stroke="currentColor" stroke-width="2" stroke-linejoin="round" />
                <path d="M7 9h6M7 13h4" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
              </svg>
              <span class="result-copy">
                <span class="result-title">
                  <template v-for="(part, partIndex) in highlightParts(result.title)" :key="partIndex">
                    <mark v-if="part.match">{{ part.text }}</mark>
                    <span v-else>{{ part.text }}</span>
                  </template>
                </span>
                <span class="result-meta">{{ resultMetadata(result) }}</span>
                <span v-if="previewText(result)" class="result-snippet">
                  <template v-for="(part, partIndex) in highlightParts(previewText(result))" :key="partIndex">
                    <mark v-if="part.match">{{ part.text }}</mark>
                    <span v-else>{{ part.text }}</span>
                  </template>
                </span>
              </span>
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

// Home is treated like a search result instead of permanent chrome. That keeps
// the palette focused on search, while still allowing a quick "home" command.
const homeCommandVisible = computed(() => {
  const term = query.value.trim().toLowerCase();
  return term.length > 0 && ('home'.startsWith(term) || term.includes('home'));
});

// Palette rows share one active index across the optional Home command and the
// remote results, so arrow/Enter behavior stays simple as rows appear/disappear.
function resultIndex(i: number): number {
  return homeCommandVisible.value ? i + 1 : i;
}

function maxActiveIndex(): number {
  const rowCount = results.value.length + (homeCommandVisible.value ? 1 : 0);
  return rowCount - 1;
}

// Opening the palette should put the caret straight into search, so keyboard
// users can type without first clicking the input.
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
  if (max < 0) return;
  activeIndex.value = (activeIndex.value + delta + max + 1) % (max + 1);
}

function activateSelected(): void {
  if (homeCommandVisible.value && activeIndex.value === 0) {
    goToNotes();
    return;
  }
  const result = results.value[activeIndex.value - (homeCommandVisible.value ? 1 : 0)];
  if (result) selectResult(result);
}

function previewText(result: MeetingListItem): string {
  return (result.snippet || result.matchedText || '').trim();
}

// Prefer explicit durations from local rows, then fall back to start/end from
// cloud meetings. Search results can come from either shape.
function durationMinutes(result: MeetingListItem): number | null {
  if (result.durationSeconds != null) return Math.max(1, Math.round(result.durationSeconds / 60));
  if (!result.endTimestamp) return null;
  const start = new Date(result.timestamp).getTime();
  const end = new Date(result.endTimestamp).getTime();
  if (Number.isNaN(start) || Number.isNaN(end) || end <= start) return null;
  return Math.max(1, Math.round((end - start) / 60000));
}

// Search results should carry the same lightweight meeting context as sidebar
// rows: when it happened and roughly how long it was.
function resultMetadata(result: MeetingListItem): string {
  const d = new Date(result.timestamp);
  const when = Number.isNaN(d.getTime())
    ? result.timestamp
    : `${d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })} at ${d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })}`;
  const minutes = durationMinutes(result);
  return minutes == null ? when : `${when} • ${minutes}min`;
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
  // A changed query means old rows are no longer trustworthy. Clear them
  // before debounce/network work so Enter or click cannot open a stale meeting.
  searchRequestId++;
  loading.value = true;
  error.value = null;
  results.value = [];
  activeIndex.value = 0;
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

// Trap Tab/Shift+Tab inside the palette so focus cannot escape into the page
// behind the aria-modal dialog. We cycle between the first and last focusable
// elements within panelRef instead of marking the rest of the app inert, which
// would conflict with Teleport's body-level mount point.
const focusableSelector =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function onPanelTab(event: KeyboardEvent): void {
  const panel = panelRef.value;
  if (!panel) return;
  const focusable = Array.from(panel.querySelectorAll<HTMLElement>(focusableSelector)).filter(
    (el) => !el.hasAttribute('disabled') && el.tabIndex !== -1
  );
  if (focusable.length === 0) {
    event.preventDefault();
    return;
  }
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  const active = document.activeElement as HTMLElement | null;
  const inPanel = active != null && panel.contains(active);
  if (event.shiftKey) {
    if (!inPanel || active === first) {
      event.preventDefault();
      last.focus();
    }
  } else {
    if (!inPanel || active === last) {
      event.preventDefault();
      first.focus();
    }
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
  padding: 100px 24px 24px;
  background: rgba(244, 243, 240, 0.58);
  backdrop-filter: blur(8px);
}
.palette-panel {
  width: min(600px, calc(100vw - 48px));
  max-height: min(442px, calc(100vh - 124px));
  overflow: hidden;
  border: 1px solid #d6d6d6;
  border-radius: 13px;
  background: #fbfbfa;
  box-shadow: 0 18px 54px rgba(28, 28, 28, 0.18);
  color: #1c1c1c;
  font-family: 'Polymath', -apple-system, system-ui, sans-serif;
}
.palette-input-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 14px 18px;
  border-bottom: 1px solid #e5e6e3;
}
.input-icon {
  width: 18px;
  height: 18px;
  flex: 0 0 auto;
  color: #8f8c87;
}
.palette-input {
  min-width: 0;
  flex: 1;
  border: 0;
  outline: none;
  background: transparent;
  color: #1c1c1c;
  font: inherit;
  font-size: 17px;
  line-height: 1.3;
}
.palette-input::placeholder { color: #9a9a96; }
.esc-chip {
  border: 1px solid #d6d6d6;
  border-radius: 7px;
  background: #ffffff;
  color: #6f6f6f;
  font: inherit;
  font-size: 11px;
  font-weight: 600;
  padding: 3px 6px;
  cursor: pointer;
}
.esc-chip:hover {
  color: #1c1c1c;
  background: #fbfbfb;
}
.palette-content {
  overflow-y: auto;
  max-height: calc(min(442px, calc(100vh - 124px)) - 54px);
  padding: 8px;
}
.command-row,
.result-row {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 12px;
  border: 1px solid transparent;
  border-radius: 10px;
  background: transparent;
  color: #1c1c1c;
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.command-row {
  min-height: 44px;
  padding: 0 14px;
  font-size: 17px;
  font-weight: 600;
}
.command-row.active,
.result-row.active {
  background: #ffffff;
  border-color: #1c1c1c;
  box-shadow: 3px 3px 0 #e7e5e2;
}
.command-row:hover,
.result-row:hover {
  background: rgba(0, 0, 0, 0.03);
}
.command-row.active:hover,
.result-row.active:hover {
  background: #ffffff;
}
.row-icon {
  width: 20px;
  height: 20px;
  flex: 0 0 auto;
  color: #8f8c87;
}
.meeting-icon { color: #8f8c87; }
.empty-row {
  padding: 10px 14px 14px;
  color: #6f6f6f;
  font-size: 14px;
}
.empty-row.error { color: #dc2626; }
.result-row {
  min-height: 76px;
  padding: 9px 14px;
}
.result-copy {
  min-width: 0;
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 4px;
}
.result-title {
  overflow: hidden;
  color: #1c1c1c;
  font-size: 16px;
  font-weight: 600;
  line-height: 1.25;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.result-meta {
  overflow: hidden;
  color: #6f6f6f;
  font-size: 13px;
  line-height: 1.25;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.result-snippet {
  overflow: hidden;
  color: #7b7b76;
  font-size: 13px;
  line-height: 1.25;
  text-overflow: ellipsis;
  white-space: nowrap;
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
