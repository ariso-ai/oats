<template>
  <div ref="rootEl" class="card todos">
    <header class="card-head">
      <div class="head-titles">
        <h2 class="head-title">
          Todos<template v-if="filterMeeting"><span class="head-sep"> · </span><span class="head-filter">{{ filterMeeting.title }}</span></template>
        </h2>
      </div>
      <button
        v-if="filterMeeting"
        type="button"
        class="head-clear"
        aria-label="Show all todos"
        title="Show all todos"
        @click="emit('clear-filter')"
      >
        <svg viewBox="0 0 24 24" class="ic" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg>
      </button>
      <span v-if="!loading && !error" class="head-count" data-testid="todos-open-count">{{ openCount }} open</span>
    </header>

    <div v-if="loading" class="card-state"><span class="spinner" /><span>Loading todos…</span></div>
    <div v-else-if="error" class="card-state card-state--error">{{ error }}</div>

    <div v-else class="todos-body">
      <p v-if="writeError" class="todos-error" role="alert" aria-live="polite">{{ writeError }}</p>

      <p v-if="isEmpty" class="todos-empty">
        No todos yet. Action items from your meetings will show up here.
      </p>
      <p v-else-if="sections.open.length === 0" class="todos-empty">All caught up.</p>

      <section
        v-for="g in groups"
        :key="g.key"
        class="todos-sec"
        :class="{ 'todos-sec--completed': g.completed }"
        :aria-label="g.completed ? undefined : g.label"
      >
        <button
          v-if="g.completed"
          type="button"
          class="todos-disclosure"
          :aria-expanded="showCompleted"
          @click="showCompleted = !showCompleted"
        >
          <svg viewBox="0 0 24 24" class="ic chevron" :class="{ open: showCompleted }" aria-hidden="true"><path d="m9 6 6 6-6 6" /></svg>
          <span>Completed · last {{ completedWindowDays }} days</span>
          <span class="count">{{ g.rows.length }}</span>
        </button>
        <h3 v-else class="todos-sec-h">{{ g.label }} <span class="count">{{ g.rows.length }}</span></h3>
        <ul v-if="!g.completed || showCompleted" class="todos-list">
          <li
            v-for="row in g.rows"
            :key="row.key"
            class="todo-row"
            :class="{ 'todo-row--done': isDone(row), 'todo-row--busy': busy.has(row.key) }"
            tabindex="0"
            :data-row-key="row.key"
            @keydown="onRowKeydown($event, row)"
          >
            <input
              type="checkbox"
              class="todo-check"
              tabindex="-1"
              :checked="isDone(row)"
              :disabled="!refFor(row) || busy.has(row.key)"
              :title="refFor(row) ? undefined : 'Tick this one in the note itself'"
              :aria-label="isDone(row) ? `Mark as not done: ${row.text}` : `Mark as done: ${row.text}`"
              @change="onCheck(row, $event)"
            />
            <div class="todo-main">
              <span class="todo-text">{{ row.text }}</span>
              <!-- Open rows sit under their day header; Completed has none. -->
              <span class="todo-sub">{{ row.meeting.title }} · {{ g.completed ? day(row.meeting.timestamp) : fmtClock(row.meeting.timestamp) }}</span>
              <span v-if="g.completed && isDone(row)" class="todo-prov">{{ provenance(row) }}</span>
            </div>
            <button
              type="button"
              class="todo-act"
              :aria-label="`Open meeting: ${row.meeting.title}`"
              @click="emit('open-meeting', row.meeting)"
            >Open meeting</button>
          </li>
        </ul>
      </section>
    </div>

    <div v-if="toast" class="todos-toast" role="status">
      <span class="todos-toast-msg">{{ toast.message }}</span>
      <button type="button" class="todos-toast-undo" @click="undo">Undo</button>
    </div>
  </div>
</template>

<script setup lang="ts">
// The Todos tab's detail pane: every action item across the user's meetings,
// checkable in place. A check strikes the row where it is and offers Undo for a
// few seconds; the row only moves to Completed when the list is next rebuilt
// (after the Undo window), so a mis-click stays visible where it happened.
import { computed, nextTick, onBeforeUnmount, reactive, ref, watch } from 'vue';
import type { ActionItemEntry, ActionItemRef, MeetingListItem } from '../composables/useBackend';
import {
  parseCompletionDate,
  splitTodoSections,
  todoRows,
  type ActionItemRow,
} from '../composables/actionItemSections';
import { fmtClock, shortDayLabel } from '../composables/groupMeetingsByDate';

const props = defineProps<{
  entries: ActionItemEntry[];
  loading: boolean;
  error: string | null;
  /** Today, for the day headers. Pass a value that only changes when the day
   *  does, or every change regroups the list. */
  now: Date;
  /** Sidebar filter: show only this meeting's todos. */
  filterMeeting: MeetingListItem | null;
  /** How far back Completed reaches; the list was fetched with this window. */
  completedWindowDays: number;
  /** Check off / reopen one todo; resolves to the ref for the next write. */
  setDone: (ref: ActionItemRef, done: boolean) => Promise<ActionItemRef>;
  /** The todos changed and the list should be rebuilt. A callback rather than
   *  an emit: it can fire as the pane unmounts (leaving the tab inside the
   *  Undo window), when Vue would drop an emit. */
  listChanged: () => void;
}>();

const emit = defineEmits<{
  'open-meeting': [meeting: MeetingListItem];
  'clear-filter': [];
}>();

const UNDO_MS = 5000;

const rootEl = ref<HTMLElement | null>(null);
const showCompleted = ref(false);
const writeError = ref<string | null>(null);

const rows = computed(() =>
  todoRows(
    props.filterMeeting
      ? props.entries.filter((e) => e.meeting.id === props.filterMeeting!.id)
      : props.entries
  )
);
const sections = computed(() => splitTodoSections(rows.value, props.now));
// One list for the template: the day sections, then Completed.
const groups = computed(() => [
  ...sections.value.open.map((s) => ({ ...s, completed: false })),
  ...(sections.value.completed.length
    ? [{ key: 'completed', label: 'Completed', rows: sections.value.completed, completed: true }]
    : []),
]);
const isEmpty = computed(() => rows.value.length === 0);

// ── optimistic state ──────────────────────────────────────────────────────
// A row's state while its write is in flight or awaiting the next rebuild,
// keyed by the row's stable key. `ref` is the one the next write must use.
interface Override {
  done: boolean;
  ref: ActionItemRef;
}
const overrides = reactive(new Map<string, Override>());
const busy = reactive(new Set<string>());

const isDone = (row: ActionItemRow): boolean =>
  overrides.get(row.key)?.done ?? !!row.item.completion;
const refFor = (row: ActionItemRow): ActionItemRef | undefined =>
  overrides.get(row.key)?.ref ?? row.item.ref;

const openCount = computed(() => rows.value.filter((r) => !isDone(r)).length);

// A rebuild that agrees with an override retires it; so does one where the
// row is gone. Rows still being written keep theirs.
watch(
  () => props.entries,
  (entries) => {
    const fresh = new Map(todoRows(entries).map((r) => [r.key, r]));
    for (const [key, o] of overrides) {
      if (busy.has(key)) continue;
      const row = fresh.get(key);
      if (!row || !!row.item.completion === o.done) overrides.delete(key);
    }
  }
);

/** Write one row's state. Resolves true on success. */
async function write(row: ActionItemRow, done: boolean): Promise<boolean> {
  const ref = refFor(row);
  if (!ref || busy.has(row.key)) return false;
  const previous = overrides.get(row.key);
  overrides.set(row.key, { done, ref });
  busy.add(row.key);
  writeError.value = null;
  try {
    const nextRef = await props.setDone(ref, done);
    overrides.set(row.key, { done, ref: nextRef });
    return true;
  } catch (e) {
    console.error('Failed to update todo', e);
    if (previous) overrides.set(row.key, previous);
    else overrides.delete(row.key);
    writeError.value = `Could not update "${row.text}". Please try again.`;
    return false;
  } finally {
    busy.delete(row.key);
  }
}

// ── undo toast ────────────────────────────────────────────────────────────
interface Toast {
  message: string;
  rows: ActionItemRow[];
  done: boolean;
}
const toast = ref<Toast | null>(null);
let toastTimer: ReturnType<typeof setTimeout> | null = null;
// A write landed that the list hasn't been rebuilt with yet.
let refreshPending = false;

function flushRefresh(): void {
  if (!refreshPending) return;
  refreshPending = false;
  props.listChanged();
}

function clearToastTimer(): void {
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = null;
}

function showToast(t: Toast): void {
  clearToastTimer();
  toast.value = t;
  toastTimer = setTimeout(() => {
    toast.value = null;
    toastTimer = null;
    flushRefresh();
  }, UNDO_MS);
}

async function toggle(row: ActionItemRow, done: boolean): Promise<void> {
  if (!(await write(row, done))) return;
  refreshPending = true;
  showToast({ message: `${done ? 'Completed' : 'Reopened'} "${row.text}"`, rows: [row], done });
}

async function undo(): Promise<void> {
  const t = toast.value;
  if (!t) return;
  clearToastTimer();
  toast.value = null;
  await Promise.all(t.rows.map((row) => write(row, !t.done)));
  refreshPending = false;
  props.listChanged();
}

function onCheck(row: ActionItemRow, e: Event): void {
  const box = e.target as HTMLInputElement;
  void toggle(row, box.checked).then(() => {
    // The box is bound one-way: put the DOM back in step with the state.
    box.checked = isDone(row);
  });
}

// ── keyboard ──────────────────────────────────────────────────────────────
function onRowKeydown(e: KeyboardEvent, row: ActionItemRow): void {
  const li = e.currentTarget as HTMLElement;
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault();
    const rows = [...(rootEl.value?.querySelectorAll<HTMLElement>('.todo-row') ?? [])];
    const i = rows.indexOf(li);
    rows[e.key === 'ArrowDown' ? i + 1 : i - 1]?.focus();
    return;
  }
  // Keys aimed at the row's own button keep their native meaning.
  if (e.target !== li) return;
  if (e.key === ' ') {
    e.preventDefault();
    void toggle(row, !isDone(row));
  } else if (e.key === 'Enter') {
    e.preventDefault();
    emit('open-meeting', row.meeting);
  }
}

// ── formatting ────────────────────────────────────────────────────────────
function day(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : shortDayLabel(d);
}

function provenance(row: ActionItemRow): string {
  const c = row.item.completion;
  const d = parseCompletionDate(c?.at);
  const who = c?.by === 'ari' ? 'Marked done by Ari' : 'Done by you';
  return d ? `${who} · ${shortDayLabel(d)}` : who;
}

// The filter changing re-scopes the pane; start back at its top.
watch(
  () => props.filterMeeting?.id,
  async () => {
    await nextTick();
    rootEl.value?.querySelector('.todos-body')?.scrollTo?.({ top: 0 });
  }
);

onBeforeUnmount(() => {
  clearToastTimer();
  flushRefresh();
});
</script>

<style scoped>
.card {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  min-height: 0;
  position: relative;
  background: #ffffff;
  border: 1px solid #e5e6e3;
  border-radius: 16px;
  overflow: hidden;
  font-family: 'Polymath', -apple-system, system-ui, sans-serif;
}
.card-state { display: flex; align-items: center; justify-content: center; gap: 10px; flex: 1; color: #6f6f6f; font-size: 14px; }
.card-state--error { color: #dc2626; }
.spinner { width: 18px; height: 18px; border: 2px solid #e5e6e3; border-bottom-color: #1c1c1c; border-radius: 50%; animation: spin 0.7s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
.ic { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; flex-shrink: 0; }

.card-head { display: flex; align-items: center; gap: 12px; padding: 22px 24px 12px; border-bottom: 1px solid #e5e6e3; }
.head-titles { flex: 1; min-width: 0; }
.head-title { margin: 0; font-size: 22px; font-weight: 700; line-height: 1.2; color: #1c1c1c; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.head-sep, .head-filter { font-weight: 500; color: #535353; }
.head-clear { display: flex; padding: 4px; border: none; border-radius: 6px; background: none; color: #6f6f6f; cursor: pointer; }
.head-clear:hover { background: #ecebe8; color: #1c1c1c; }
.head-count { flex-shrink: 0; font-size: 13px; font-weight: 500; color: #6f6f6f; }

.todos-body { flex: 1; min-height: 0; overflow-y: auto; padding: 8px 24px 72px; }
.todos-error { margin: 12px 0 0; font-size: 13px; color: #b91c1c; }
.todos-empty { margin: 16px 0 0; font-size: 14px; color: #6f6f6f; }

.todos-sec { margin-top: 18px; }
.todos-sec-h {
  margin: 0 0 6px; display: flex; align-items: center; gap: 8px;
  font-size: 11px; font-weight: 600; letter-spacing: 1.5px; color: #9a9a96;
}
.count { padding: 1px 8px; background: #ecebe8; color: #535353; font-size: 12px; font-weight: 500; letter-spacing: 0; border-radius: 999px; }

.todos-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
.todo-row {
  display: flex; align-items: flex-start; gap: 10px;
  padding: 8px 10px; margin: 0 -10px; border-radius: 10px;
  outline: none; transition: background 0.12s, opacity 0.12s;
}
.todo-row:hover { background: #f7f6f4; }
.todo-row:focus-visible { background: #f5f4f2; box-shadow: inset 0 0 0 1px #1c1c1c; }
.todo-row--busy { opacity: 0.6; }
.todo-check { margin: 3px 0 0; width: 15px; height: 15px; flex-shrink: 0; cursor: pointer; accent-color: #1c1c1c; }
.todo-check:disabled { cursor: default; }
/* A todo with no way to tick it here (a legacy bullet) reads as inert; a row
   mid-write is already dimmed by .todo-row--busy. */
.todo-row:not(.todo-row--busy) .todo-check:disabled { opacity: 0.35; }
.todo-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
.todo-text { font-size: 14px; line-height: 1.4; color: #1c1c1c; overflow-wrap: anywhere; }
.todo-row--done .todo-text { text-decoration: line-through; color: #9a9a9a; }
.todo-sub { font-size: 12px; color: #6f6f6f; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.todo-prov { font-size: 12px; color: #9a9a9a; }

.todo-act {
  flex-shrink: 0; border: none; background: none; padding: 2px 8px; border-radius: 6px;
  font-family: inherit; font-size: 12px; font-weight: 500; color: #6f6f6f; cursor: pointer;
  opacity: 0; transition: opacity 0.12s, background 0.12s;
}
.todo-act:hover { background: #ecebe8; color: #1c1c1c; }
/* Revealed on hover, and on keyboard focus so it stays reachable. */
.todo-row:hover .todo-act, .todo-row:focus-within .todo-act { opacity: 1; }

.todos-disclosure {
  display: flex; align-items: center; gap: 6px; padding: 4px 0; border: none; background: none;
  font-family: inherit; font-size: 13px; font-weight: 500; color: #535353; cursor: pointer;
}
.todos-disclosure:hover { color: #1c1c1c; }
.chevron { width: 14px; height: 14px; transition: transform 0.15s; }
.chevron.open { transform: rotate(90deg); }
.todos-sec--completed .todos-list { margin-top: 6px; }

.todos-toast {
  position: absolute; left: 50%; bottom: 18px; transform: translateX(-50%);
  display: flex; align-items: center; gap: 14px; max-width: calc(100% - 48px);
  padding: 9px 10px 9px 16px; border-radius: 12px;
  background: #1c1c1c; color: #ffffff; font-size: 13px;
  box-shadow: 0 6px 24px rgba(0, 0, 0, 0.18);
}
.todos-toast-msg { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.todos-toast-undo {
  flex-shrink: 0; border: none; border-radius: 7px; padding: 4px 10px;
  background: rgba(255, 255, 255, 0.14); color: #ffffff;
  font-family: inherit; font-size: 13px; font-weight: 600; cursor: pointer;
}
.todos-toast-undo:hover { background: rgba(255, 255, 255, 0.24); }
</style>
