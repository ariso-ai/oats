// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';
import { nextTick } from 'vue';

const listMeetings = vi.fn();
const searchMeetings = vi.fn();
const getMeetingDetail = vi.fn();
const backendId = vi.fn(() => 'local');
const usesMeetingPicker = vi.fn(() => false);
const supportsSearch = vi.fn(() => false);
const supportsActionItems = vi.fn(() => false);
const listActionItems = vi.fn();
const openRecordingFile = vi.fn();
const readRecordingAudio = vi.fn();
const readRecordingNote = vi.fn();
const writeRecordingNote = vi.fn();
const invoke = vi.fn(() => Promise.resolve());
const getAllWebviewWindows = vi.fn(() => Promise.resolve([] as { label: string }[]));
const emitNotificationsSync = vi.fn(() => Promise.resolve());
const getMeetingPrep = vi.fn();
const openPrepTab = vi.fn();
const saveNotesNow = vi.fn(() => Promise.resolve());
const listPendingUploads = vi.fn(() => Promise.resolve([]));
const minimizeWindow = vi.fn(() => Promise.resolve());
const toggleMaximizeWindow = vi.fn(() => Promise.resolve());
const closeWindow = vi.fn(() => Promise.resolve());
const isWindowMaximized = vi.fn(() => Promise.resolve(false));
const onWindowResized = vi.fn(() => Promise.resolve(() => undefined));
type SignInResult = { success?: boolean; sessionToken?: string; error?: string };
const checkSession = vi.fn((): Promise<unknown> => Promise.resolve(null));
const googleSignIn = vi.fn((): Promise<SignInResult> => Promise.resolve({ success: true }));
const microsoftSignIn = vi.fn((): Promise<SignInResult> => Promise.resolve({ success: true }));
const cancelSignIn = vi.fn(() => Promise.resolve());
const apiRequest = vi.fn(
  (_method: string, _path: string): Promise<{ status: number; data: unknown }> =>
    Promise.resolve({ status: 200, data: {} })
);
const setBackendSetting = vi.fn((_backend: string) => Promise.resolve());
const localModelStatus = vi.fn(
  (): Promise<{ state: string; llmReady?: boolean }> => Promise.resolve({ state: 'ready', llmReady: true })
);

vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock('@tauri-apps/api/webviewWindow', () => ({
  getAllWebviewWindows: () => getAllWebviewWindows(),
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    minimize: () => minimizeWindow(),
    toggleMaximize: () => toggleMaximizeWindow(),
    close: () => closeWindow(),
    isMaximized: () => isWindowMaximized(),
    onResized: (handler: () => void) => onWindowResized(handler),
  }),
}));

// In-test event bus standing in for Tauri's app-wide events: `listen` records
// handlers, `emitEvent` drives them the way the Rust side would.
type EventHandler = (e: { payload: unknown }) => void;
const eventHandlers = new Map<string, EventHandler[]>();
const emitAppEvent = vi.fn(() => Promise.resolve());
vi.mock('@tauri-apps/api/event', () => ({
  listen: (name: string, cb: EventHandler) => {
    const arr = eventHandlers.get(name) ?? [];
    arr.push(cb);
    eventHandlers.set(name, arr);
    return Promise.resolve(() => {
      const list = eventHandlers.get(name) ?? [];
      const i = list.indexOf(cb);
      if (i >= 0) list.splice(i, 1);
    });
  },
  emit: (...a: unknown[]) => emitAppEvent(...a),
}));

function emitEvent(name: string, payload: unknown): void {
  for (const cb of [...(eventHandlers.get(name) ?? [])]) cb({ payload });
}
vi.mock('../composables/useBackend', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../composables/useBackend')>();
  return {
    ...actual,
    getActiveBackend: () =>
      Promise.resolve({
        id: backendId(),
        usesMeetingPicker: usesMeetingPicker(),
        supportsSearch: supportsSearch(),
        supportsActionItems: supportsActionItems(),
        listMeetings: () => listMeetings(),
        listActionItems: () => listActionItems(),
        searchMeetings: (query: string) => searchMeetings(query),
        getMeetingDetail: (meeting: unknown) => getMeetingDetail(meeting),
        getMeetingPrep: (prepId: number) => getMeetingPrep(prepId),
      }),
  };
});
vi.mock('../composables/useMeetingNotifications', () => ({
  emitNotificationsSync: () => emitNotificationsSync(),
}));
// A miniature stand-in for the session-scoped cloud tracker: `markUploaded`
// records the id the way the real one does, and `resolveProcessing` is the
// test's hand on "the poll found content for this meeting".
// Hoisted so the (also hoisted) mock factory can reach it — the refs themselves
// are created inside the factory, which is the first place `vue` is loadable.
const processingMock = vi.hoisted(() => ({
  markUploaded: vi.fn(),
  tracked: null as null | import('vue').Ref<Set<string>>,
  version: null as null | import('vue').Ref<number>,
}));
const markUploaded = processingMock.markUploaded;
vi.mock('../composables/useMeetingProcessing', async () => {
  const { ref } = await import('vue');
  const tracked = ref(new Set<string>());
  const version = ref(0);
  processingMock.tracked = tracked;
  processingMock.version = version;
  return {
    useMeetingProcessing: () => ({
      markUploaded: (id: string | number) => {
        processingMock.markUploaded(id);
        tracked.value = new Set(tracked.value).add(String(id));
      },
      isProcessing: (id: string | number) => tracked.value.has(String(id)),
      version,
      reset: () => {
        tracked.value = new Set();
      },
    }),
  };
});
// The test's hand on "the poll found content for this meeting".
function resolveProcessing(id: string): void {
  const tracked = processingMock.tracked!;
  const next = new Set(tracked.value);
  next.delete(id);
  tracked.value = next;
  processingMock.version!.value++;
}
// RecordingAudioPlayer (rendered for local rows) and openNote/openTranscript go
// through ../tauri; keep those mocked so jsdom never touches real IPC.
// pending.list() is also mocked so the PendingUploads child never calls Tauri IPC.
vi.mock('../tauri', () => ({
  AUTH_CHANGED_EVENT: 'auth://changed',
  SIGN_IN_CANCELED_ERROR: 'Sign-in canceled',
  auth: {
    checkSession: () => checkSession(),
    googleSignIn: () => googleSignIn(),
    microsoftSignIn: () => microsoftSignIn(),
    cancelSignIn: () => cancelSignIn(),
    signOut: () => Promise.resolve(),
  },
  api: {
    request: (method: string, path: string) => apiRequest(method, path),
  },
  setBackendSetting: (backend: string) => setBackendSetting(backend),
  local: {
    modelStatus: () => localModelStatus(),
    openRecordingFile: (id: string, kind: string) => openRecordingFile(id, kind),
    readRecordingAudio: (id: string) => readRecordingAudio(id),
    readRecordingNote: (id: string) => readRecordingNote(id),
    writeRecordingNote: (id: string, markdown: string) => writeRecordingNote(id, markdown),
  },
  pending: {
    list: () => listPendingUploads(),
  },
}));

import LibraryView from './LibraryView.vue';
import UpNextCard from './UpNextCard.vue';

function item(over: Record<string, unknown>) {
  return {
    id: 'x',
    title: 'T',
    timestamp: '2026-06-02T10:00:00Z',
    durationSeconds: 10,
    status: 'done',
    files: { hasAudio: false, hasNote: false, hasTranscript: false },
    ...over,
  };
}

// MeetingDetailView mounts a TipTap editor jsdom can't fully host; stub it to a
// lightweight placeholder when a test only cares about the surrounding Library
// chrome (selection, the recorder strip, the titlebar button).
const detailStub = {
  name: 'MeetingDetailView',
  emits: ['close', 'title-updated', 'content-ready', 'tasks-changed', 'deleted'],
  props: ['item'],
  methods: {
    openPrepTab: (meetingId?: string) => openPrepTab(meetingId),
    saveNotesNow: () => saveNotesNow(),
  },
  template:
    '<div class="detail-stub" :data-meeting="item?.id" :data-prep="item?.prepId"><button class="btn-close" @click="$emit(\'close\')">x</button></div>',
};
function mountWithDetailStub() {
  return mount(LibraryView, { global: { stubs: { MeetingDetailView: detailStub } } });
}

// Auto-unmount between tests so each component's window 'focus' listener is
// removed — otherwise a dispatched focus event would also fire stale listeners.
enableAutoUnmount(afterEach);
beforeEach(() => {
  vi.clearAllMocks();
  eventHandlers.clear();
  getAllWebviewWindows.mockResolvedValue([]);
  getMeetingDetail.mockImplementation((meeting) =>
    Promise.resolve({
      id: meeting.id,
      title: meeting.title,
      startAt: meeting.timestamp,
      participants: [],
      actionItems: [],
      audioClips: [],
      isLocal: true,
      durationSeconds: meeting.durationSeconds,
      hasTranscript: meeting.files?.hasTranscript ?? false,
    })
  );
  invoke.mockResolvedValue(undefined);
  getMeetingPrep.mockResolvedValue(null);
  listPendingUploads.mockResolvedValue([]);
  isWindowMaximized.mockResolvedValue(false);
  onWindowResized.mockResolvedValue(() => undefined);
  readRecordingNote.mockResolvedValue('');
  writeRecordingNote.mockResolvedValue(undefined);
  backendId.mockReturnValue('local');
  usesMeetingPicker.mockReturnValue(false);
  supportsSearch.mockReturnValue(true);
  supportsActionItems.mockReturnValue(false);
  listActionItems.mockResolvedValue([]);
  searchMeetings.mockResolvedValue([]);
  processingMock.tracked!.value = new Set();
  processingMock.version!.value = 0;
  checkSession.mockResolvedValue(null);
  googleSignIn.mockResolvedValue({ success: true });
  microsoftSignIn.mockResolvedValue({ success: true });
  cancelSignIn.mockResolvedValue(undefined);
  apiRequest.mockResolvedValue({ status: 200, data: {} });
  setBackendSetting.mockResolvedValue(undefined);
  localModelStatus.mockResolvedValue({ state: 'ready', llmReady: true });
});
afterEach(() => {
  // Restore real timers even if a fake-timer test failed before its own
  // vi.useRealTimers() — otherwise leaked fake timers break later tests.
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('LibraryView', () => {
  it('integrates the sidebar toggle and native-equivalent controls into Windows title chrome', async () => {
    vi.spyOn(window.navigator, 'userAgent', 'get').mockReturnValue('Mozilla/5.0 (Windows NT 10.0; Win64; x64)');
    listMeetings.mockResolvedValue([]);

    const wrapper = mount(LibraryView);
    await flushPromises();

    const titlebar = wrapper.get('.titlebar--windows');
    expect(titlebar.text()).toContain('Meetings');
    expect(titlebar.find('.panel-toggle').exists()).toBe(true);
    expect(titlebar.findAll('.window-control')).toHaveLength(3);

    await titlebar.get('.panel-toggle').trigger('click');
    expect(wrapper.find('.sidebar').exists()).toBe(false);
  });

  it('wires the Windows titlebar controls to the current Tauri window', async () => {
    vi.spyOn(window.navigator, 'userAgent', 'get').mockReturnValue('Mozilla/5.0 (Windows NT 10.0; Win64; x64)');
    listMeetings.mockResolvedValue([]);

    const wrapper = mount(LibraryView);
    await flushPromises();

    await wrapper.get('[aria-label="Minimize window"]').trigger('click');
    await wrapper.get('[aria-label="Maximize window"]').trigger('click');
    await wrapper.get('[aria-label="Close window"]').trigger('click');

    expect(minimizeWindow).toHaveBeenCalledOnce();
    expect(toggleMaximizeWindow).toHaveBeenCalledOnce();
    expect(closeWindow).toHaveBeenCalledOnce();
  });

  it('shows a restore control when the Windows library is maximized', async () => {
    vi.spyOn(window.navigator, 'userAgent', 'get').mockReturnValue('Mozilla/5.0 (Windows NT 10.0; Win64; x64)');
    isWindowMaximized.mockResolvedValue(true);
    listMeetings.mockResolvedValue([]);

    const wrapper = mount(LibraryView);
    await flushPromises();

    expect(wrapper.find('[aria-label="Restore window"]').exists()).toBe(true);
    expect(wrapper.find('[aria-label="Maximize window"]').exists()).toBe(false);
  });

  it('keeps custom Windows controls out of the macOS overlay titlebar', async () => {
    vi.spyOn(window.navigator, 'userAgent', 'get').mockReturnValue('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)');
    listMeetings.mockResolvedValue([]);

    const wrapper = mount(LibraryView);
    await flushPromises();

    expect(wrapper.find('.titlebar--windows').exists()).toBe(false);
    expect(wrapper.find('.window-controls').exists()).toBe(false);
    expect(wrapper.find('.panel-toggle').exists()).toBe(true);
  });

  it('shows an empty state when there are no meetings', async () => {
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.text()).toContain('No meetings yet');
  });

  it('renders a meeting-item row per meeting', async () => {
    listMeetings.mockResolvedValue([
      item({ id: 'b', title: 'Second', durationSeconds: 75 }),
      item({ id: 'a', title: 'First', durationSeconds: 3661 }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    const rows = wrapper.findAll('.meeting-item');
    expect(rows).toHaveLength(2);
    expect(rows[0].text()).toContain('Second');
    expect(rows[1].text()).toContain('First');
  });

  it('broadcasts native sync after a successful meeting list refresh', async () => {
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Synced' })]);
    mount(LibraryView);
    await flushPromises();
    expect(emitNotificationsSync).toHaveBeenCalledTimes(1);
  });

  it('opens the search palette from the sidebar trigger for searchable backends', async () => {
    backendId.mockReturnValue('ariso');
    supportsSearch.mockReturnValue(true);
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Synced' })]);

    const wrapper = mount(LibraryView);
    await flushPromises();

    await wrapper.get('.search-trigger').trigger('click');
    await flushPromises();

    expect(document.body.querySelector<HTMLInputElement>('.palette-input')?.placeholder).toBe('Search');
    expect(document.body.textContent).not.toContain('Search notes');
    expect(document.body.querySelector('.palette-panel')).not.toBeNull();
  });

  it('shows the search trigger for local backend and opens the shared palette', async () => {
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Local' })]);
    const wrapper = mount(LibraryView);
    await flushPromises();

    await wrapper.get('.search-trigger').trigger('click');
    await flushPromises();

    expect(document.body.querySelector('.palette-panel')).not.toBeNull();
  });

  it('hides the search trigger when the backend does not support search', async () => {
    supportsSearch.mockReturnValue(false);
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Local' })]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.find('.search-trigger').exists()).toBe(false);
  });

  it('opens search with Ctrl+K but not Alt+K on non-Mac platforms', async () => {
    vi.spyOn(window.navigator, 'platform', 'get').mockReturnValue('Linux x86_64');
    backendId.mockReturnValue('ariso');
    supportsSearch.mockReturnValue(true);
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Existing' })]);

    mount(LibraryView);
    await flushPromises();

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', altKey: true }));
    await flushPromises();
    expect(document.body.querySelector('.palette-panel')).toBeNull();

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true }));
    await flushPromises();
    expect(document.body.querySelector('.palette-panel')).not.toBeNull();
  });

  it('searches remotely from the palette and renders returned rows', async () => {
    vi.useFakeTimers();
    backendId.mockReturnValue('ariso');
    supportsSearch.mockReturnValue(true);
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Existing' })]);
    searchMeetings.mockResolvedValue([
      item({
        id: 's1',
        title: 'Search Note',
        timestamp: '2026-06-07T10:00:00Z',
        endTimestamp: '2026-06-07T10:01:00Z',
        snippet: 'Discussed note search',
        files: undefined,
      }),
    ]);

    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.get('.search-trigger').trigger('click');
    await flushPromises();

    const input = document.body.querySelector<HTMLInputElement>('.palette-input');
    expect(input).not.toBeNull();
    input!.value = 'note';
    input!.dispatchEvent(new Event('input'));
    await vi.advanceTimersByTimeAsync(180);
    await flushPromises();

    expect(searchMeetings).toHaveBeenCalledWith('note');
    expect(document.body.textContent).toContain('Search Note');
    expect(document.body.textContent).toContain('Jun 7');
    expect(document.body.textContent).toContain('1min');
    expect(document.body.textContent).toContain('Discussed note search');
    vi.useRealTimers();
  });

  it('resets the search query when the backend changes', async () => {
    backendId.mockReturnValue('ariso');
    supportsSearch.mockReturnValue(true);
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Existing' })]);

    const wrapper = mount(LibraryView);
    await flushPromises();

    await wrapper.get('.search-trigger').trigger('click');
    await flushPromises();
    const input = document.body.querySelector<HTMLInputElement>('.palette-input')!;
    input.value = 'note';
    input.dispatchEvent(new Event('input'));
    await flushPromises();
    expect(document.body.querySelector<HTMLInputElement>('.palette-input')!.value).toBe('note');

    // Switch backends via the focus-driven reload; the palette is keyed by the
    // active backend, so it remounts and the previous query is discarded.
    backendId.mockReturnValue('local');
    window.dispatchEvent(new Event('focus'));
    await flushPromises();

    const inputs = [...document.body.querySelectorAll<HTMLInputElement>('.palette-input')];
    expect(inputs.length).toBeGreaterThan(0);
    expect(inputs.every((el) => el.value === '')).toBe(true);
  });

  it('clears stale search rows as soon as the query changes', async () => {
    vi.useFakeTimers();
    backendId.mockReturnValue('ariso');
    supportsSearch.mockReturnValue(true);
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Existing' })]);
    searchMeetings
      .mockResolvedValueOnce([item({ id: 'alpha', title: 'Alpha Result', files: undefined })])
      .mockResolvedValueOnce([item({ id: 'beta', title: 'Beta Result', files: undefined })]);

    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.get('.search-trigger').trigger('click');
    await flushPromises();
    const input = document.body.querySelector<HTMLInputElement>('.palette-input')!;

    input.value = 'alpha';
    input.dispatchEvent(new Event('input'));
    await vi.advanceTimersByTimeAsync(180);
    await flushPromises();
    expect(document.body.textContent).toContain('Alpha Result');

    input.value = 'beta';
    input.dispatchEvent(new Event('input'));
    await flushPromises();

    expect(document.body.textContent).not.toContain('Alpha Result');
    expect(document.body.textContent).toContain('Searching');
    await vi.advanceTimersByTimeAsync(180);
    await flushPromises();
    expect(document.body.textContent).toContain('Beta Result');
    vi.useRealTimers();
  });

  it('shows Home only when the search query matches it', async () => {
    vi.useFakeTimers();
    backendId.mockReturnValue('ariso');
    supportsSearch.mockReturnValue(true);
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Existing' })]);
    searchMeetings.mockResolvedValue([]);

    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.get('.search-trigger').trigger('click');
    await flushPromises();
    const input = document.body.querySelector<HTMLInputElement>('.palette-input')!;

    expect(document.body.textContent).not.toContain('Home');
    input.value = 'note';
    input.dispatchEvent(new Event('input'));
    await vi.advanceTimersByTimeAsync(180);
    await flushPromises();
    expect(document.body.textContent).not.toContain('Home');

    input.value = 'home';
    input.dispatchEvent(new Event('input'));
    await vi.advanceTimersByTimeAsync(180);
    await flushPromises();
    expect(document.body.textContent).toContain('Home');
    vi.useRealTimers();
  });

  it('the Home search command clears the selected meeting', async () => {
    vi.useFakeTimers();
    backendId.mockReturnValue('ariso');
    supportsSearch.mockReturnValue(true);
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Existing' })]);
    searchMeetings.mockResolvedValue([]);

    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.text()).toContain('Existing');

    await wrapper.get('.search-trigger').trigger('click');
    await flushPromises();
    const input = document.body.querySelector<HTMLInputElement>('.palette-input')!;
    input.value = 'home';
    input.dispatchEvent(new Event('input'));
    await vi.advanceTimersByTimeAsync(180);
    await flushPromises();

    document.body.querySelector<HTMLButtonElement>('.command-row')!.click();
    await flushPromises();

    expect(document.body.querySelector('.palette-panel')).toBeNull();
    expect(wrapper.text()).toContain('Ready for the next meet?');
    vi.useRealTimers();
  });

  it('ignores stale search responses in the palette', async () => {
    vi.useFakeTimers();
    backendId.mockReturnValue('ariso');
    supportsSearch.mockReturnValue(true);
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Existing' })]);
    let resolveFirst: (value: unknown) => void = () => {};
    searchMeetings
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveFirst = resolve;
          })
      )
      .mockResolvedValueOnce([item({ id: 'new', title: 'New Result', files: undefined })]);

    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.get('.search-trigger').trigger('click');
    await flushPromises();
    const input = document.body.querySelector<HTMLInputElement>('.palette-input')!;

    input.value = 'old';
    input.dispatchEvent(new Event('input'));
    await vi.advanceTimersByTimeAsync(180);
    await flushPromises();
    input.value = 'new';
    input.dispatchEvent(new Event('input'));
    await vi.advanceTimersByTimeAsync(180);
    await flushPromises();
    resolveFirst([item({ id: 'old', title: 'Old Result', files: undefined })]);
    await flushPromises();

    expect(document.body.textContent).toContain('New Result');
    expect(document.body.textContent).not.toContain('Old Result');
    vi.useRealTimers();
  });

  it('stops spinning and shows an error when a search request never settles', async () => {
    vi.useFakeTimers();
    backendId.mockReturnValue('ariso');
    supportsSearch.mockReturnValue(true);
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Existing' })]);
    // A hung backend request: the promise never resolves or rejects, which is
    // what left cmd-K stuck in a persistent "Searching…" state (issue #210).
    searchMeetings.mockImplementation(() => new Promise(() => {}));

    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.get('.search-trigger').trigger('click');
    await flushPromises();
    const input = document.body.querySelector<HTMLInputElement>('.palette-input')!;

    input.value = 'note';
    input.dispatchEvent(new Event('input'));
    await vi.advanceTimersByTimeAsync(180);
    await flushPromises();
    // Loading progress shows briefly while the request is in flight.
    expect(document.body.textContent).toContain('Searching');

    // Once the search timeout elapses it must stop spinning and surface a clear
    // error state instead of hanging on "Searching…" forever.
    await vi.advanceTimersByTimeAsync(15000);
    await flushPromises();
    expect(document.body.textContent).not.toContain('Searching');
    const errorRow = document.body.querySelector('.empty-row.error');
    expect(errorRow).not.toBeNull();
    expect(errorRow!.textContent).toContain('timed out');
    vi.useRealTimers();
  });

  it('selecting a search result closes the palette and opens that meeting', async () => {
    vi.useFakeTimers();
    backendId.mockReturnValue('ariso');
    supportsSearch.mockReturnValue(true);
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Existing' })]);
    searchMeetings.mockResolvedValue([item({ id: '42', title: 'Found Meeting', files: undefined })]);

    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.get('.search-trigger').trigger('click');
    await flushPromises();
    const input = document.body.querySelector<HTMLInputElement>('.palette-input')!;
    input.value = 'found';
    input.dispatchEvent(new Event('input'));
    await vi.advanceTimersByTimeAsync(180);
    await flushPromises();

    document.body.querySelector<HTMLButtonElement>('.result-row')!.click();
    await flushPromises();

    expect(document.body.querySelector('.palette-panel')).toBeNull();
    expect(getMeetingDetail).toHaveBeenLastCalledWith(expect.objectContaining({ id: '42' }));
    vi.useRealTimers();
  });

  it('auto-selects the first meeting item', async () => {
    listMeetings.mockResolvedValue([
      item({ id: 'a', title: 'Standup' }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    const btn = wrapper.find('.meeting-item');
    expect(btn.attributes('aria-pressed')).toBe('true');
    expect(btn.classes()).toContain('selected');
  });

  it('clicking a meeting item selects it (aria-pressed becomes true)', async () => {
    listMeetings.mockResolvedValue([
      item({ id: 'a', title: 'Standup' }),
      item({ id: 'b', title: 'Planning' }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();

    const rows = wrapper.findAll('.meeting-item');
    expect(rows[0].attributes('aria-pressed')).toBe('true');
    expect(rows[1].attributes('aria-pressed')).toBe('false');

    await rows[1].trigger('click');
    await flushPromises();

    expect(rows[0].attributes('aria-pressed')).toBe('false');
    expect(rows[1].attributes('aria-pressed')).toBe('true');
    expect(rows[1].classes()).toContain('selected');
  });

  it('keeps the latest clicked meeting selected when pending note saves finish out of order', async () => {
    listMeetings.mockResolvedValue([
      item({ id: 'a', title: 'Standup' }),
      item({ id: 'b', title: 'Planning' }),
      item({ id: 'c', title: 'Retro' }),
    ]);
    const saveResolvers: Array<() => void> = [];
    writeRecordingNote.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          saveResolvers.push(resolve);
        })
    );

    const wrapper = mount(LibraryView);
    await flushPromises();
    const rows = wrapper.findAll('.meeting-item');

    void rows[1].trigger('click');
    await flushPromises();
    void rows[2].trigger('click');
    await flushPromises();

    expect(saveResolvers).toHaveLength(2);
    saveResolvers[1]();
    await flushPromises();
    saveResolvers[0]();
    await flushPromises();

    expect(rows[1].attributes('aria-pressed')).toBe('false');
    expect(rows[2].attributes('aria-pressed')).toBe('true');
    expect(rows[2].classes()).toContain('selected');
  });

  it('shows the meeting title and time subtitle in each row', async () => {
    listMeetings.mockResolvedValue([
      item({ id: 'a', title: 'Morning Sync', durationSeconds: 300 }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    const row = wrapper.find('.meeting-item');
    expect(row.find('.mi-title').text()).toBe('Morning Sync');
    expect(row.find('.mi-sub').text()).toContain('min');
  });

  // Between the recorder pill's checkmark and the notes actually landing, the
  // row is the only place the sidebar can say a meeting is still being worked
  // on rather than permanently empty (#315).
  describe('processing rows', () => {
    async function mountWithRows(rows: Record<string, unknown>[]) {
      listMeetings.mockResolvedValue(rows);
      const wrapper = mountWithDetailStub();
      await flushPromises();
      return wrapper;
    }

    it('shows a spinner and "Processing…" while a local recording transcribes', async () => {
      const wrapper = await mountWithRows([item({ id: 'a', status: 'transcribing' })]);
      const row = wrapper.get('.meeting-item');
      expect(row.find('.mi-sub--processing').text()).toContain('Processing');
      expect(row.find('.mi-spinner').exists()).toBe(true);
    });

    it('keeps showing "Processing…" while a transcribed local recording awaits its notes', async () => {
      const wrapper = await mountWithRows([
        item({
          id: 'a',
          timestamp: new Date(Date.now() - 60_000).toISOString(),
          status: 'done',
          files: { hasAudio: true, hasTranscript: true, hasNote: false },
        }),
      ]);
      expect(wrapper.get('.meeting-item').find('.mi-sub--processing').exists()).toBe(true);
    });

    // Notes generation runs for minutes. A recording that ended long ago and
    // still has no note is stuck, not busy — a real vault had 18 such rows from
    // three months back, every one of them spinning forever.
    it('stops inferring "Processing…" for a recording that ended long ago', async () => {
      const wrapper = await mountWithRows([
        item({
          id: 'a',
          timestamp: new Date(Date.now() - 3 * 60 * 60 * 1000).toISOString(),
          status: 'done',
          files: { hasAudio: true, hasTranscript: true, hasNote: false },
        }),
      ]);
      expect(wrapper.get('.meeting-item').find('.mi-sub--processing').exists()).toBe(false);
    });

    // A missing note only implies "still generating" while the list can't say
    // otherwise. Once it reports the notes settled without one — nothing was
    // said, or generation failed — a spinning row would be a lie, and it would
    // make every content-ready report from the open detail reload the list.
    it.each(['empty-transcript', 'failed'])(
      'shows the normal sub-line for a recent recording whose notes settled as %s',
      async (notesStatus) => {
        const wrapper = await mountWithRows([
          item({
            id: 'a',
            timestamp: new Date(Date.now() - 60_000).toISOString(),
            status: 'done',
            files: { hasAudio: true, hasTranscript: true, hasNote: false, notesStatus },
          }),
        ]);
        expect(wrapper.get('.meeting-item').find('.mi-sub--processing').exists()).toBe(false);
      }
    );

    // The window is measured from the end, so a long recording isn't already
    // stale the moment it stops.
    it('measures the window from the end of a long recording, not its start', async () => {
      const wrapper = await mountWithRows([
        item({
          id: 'a',
          timestamp: new Date(Date.now() - 3 * 60 * 60 * 1000).toISOString(),
          durationSeconds: 3 * 60 * 60 - 60,
          status: 'done',
          files: { hasAudio: true, hasTranscript: true, hasNote: false },
        }),
      ]);
      expect(wrapper.get('.meeting-item').find('.mi-sub--processing').exists()).toBe(true);
    });

    // An age bound must never suppress a live state the backend reports directly.
    it('still shows "Processing…" for an old recording that is actively transcribing', async () => {
      const wrapper = await mountWithRows([
        item({
          id: 'a',
          timestamp: new Date(Date.now() - 3 * 60 * 60 * 1000).toISOString(),
          status: 'transcribing',
        }),
      ]);
      expect(wrapper.get('.meeting-item').find('.mi-sub--processing').exists()).toBe(true);
    });

    it('shows the normal sub-line once a local recording has its notes', async () => {
      const wrapper = await mountWithRows([
        item({ id: 'a', status: 'done', files: { hasAudio: true, hasTranscript: true, hasNote: true } }),
      ]);
      const row = wrapper.get('.meeting-item');
      expect(row.find('.mi-sub--processing').exists()).toBe(false);
      expect(row.find('.mi-sub').text()).toContain('min');
    });

    // A failed transcription is reported by the detail panel's chip with a Retry;
    // a row that kept spinning forever would be a lie.
    it('shows the normal sub-line for a failed local recording', async () => {
      const wrapper = await mountWithRows([
        item({ id: 'a', status: 'failed', files: { hasAudio: true, hasTranscript: false, hasNote: false } }),
      ]);
      expect(wrapper.get('.meeting-item').find('.mi-sub--processing').exists()).toBe(false);
    });

    it('marks the recorded cloud meeting as processing when the upload succeeds', async () => {
      backendId.mockReturnValue('ariso');
      const wrapper = await mountWithRows([item({ id: '42', status: undefined, files: undefined })]);

      const strip = wrapper.findComponent({ name: 'RecorderStrip' });
      strip.vm.$emit('recording-change', '42');
      strip.vm.$emit('recording-phase', 'success');
      await flushPromises();

      expect(markUploaded).toHaveBeenCalledWith('42');
      expect(wrapper.get('.meeting-item').find('.mi-sub--processing').text()).toContain('Processing');
    });

    it('does not track a local recording as a cloud upload', async () => {
      const wrapper = await mountWithRows([item({ id: 'a' })]);

      const strip = wrapper.findComponent({ name: 'RecorderStrip' });
      strip.vm.$emit('recording-change', 'a');
      strip.vm.$emit('recording-phase', 'success');
      await flushPromises();

      expect(markUploaded).not.toHaveBeenCalled();
    });

    it('reloads the list and drops the indicator once the cloud meeting has content', async () => {
      backendId.mockReturnValue('ariso');
      const wrapper = await mountWithRows([item({ id: '42', status: undefined, files: undefined })]);
      const strip = wrapper.findComponent({ name: 'RecorderStrip' });
      strip.vm.$emit('recording-change', '42');
      strip.vm.$emit('recording-phase', 'success');
      await flushPromises();
      listMeetings.mockClear();

      resolveProcessing('42');
      await flushPromises();

      expect(listMeetings).toHaveBeenCalled();
      expect(wrapper.get('.meeting-item').find('.mi-sub--processing').exists()).toBe(false);
    });

    // The detail panel owns the only live poll of a local recording's pipeline,
    // so it is what tells the list its row went stale.
    it('reloads the list when the open local recording reports its content is ready', async () => {
      const wrapper = await mountWithRows([item({ id: 'a', status: 'transcribing' })]);
      await wrapper.get('.meeting-item').trigger('click');
      await flushPromises();
      listMeetings.mockClear();

      wrapper.findComponent({ name: 'MeetingDetailView' }).vm.$emit('content-ready', { id: 'a' });
      await flushPromises();

      expect(listMeetings).toHaveBeenCalled();
    });

    it('does not reload when a row that was never processing reports content ready', async () => {
      const wrapper = await mountWithRows([item({ id: 'a', status: 'done' })]);
      await wrapper.get('.meeting-item').trigger('click');
      await flushPromises();
      listMeetings.mockClear();

      wrapper.findComponent({ name: 'MeetingDetailView' }).vm.$emit('content-ready', { id: 'a' });
      await flushPromises();

      expect(listMeetings).not.toHaveBeenCalled();
    });

    // The detail panel performs the delete; the list has to stop showing the row.
    describe('a deleted local note', () => {
      it('drops the row and clears the selection, then re-syncs from disk', async () => {
        const wrapper = await mountWithRows([item({ id: 'a' }), item({ id: 'b' })]);
        await wrapper.get('.meeting-item').trigger('click');
        await flushPromises();
        listMeetings.mockClear();
        listMeetings.mockResolvedValue([item({ id: 'b' })]);

        wrapper.findComponent({ name: 'MeetingDetailView' }).vm.$emit('deleted', { id: 'a' });
        await flushPromises();

        const rows = wrapper.findAll('.meeting-item');
        expect(rows).toHaveLength(1);
        expect(rows[0].text()).not.toContain('a');
        // Selection cleared -> the detail pane is replaced by the Up Next card.
        expect(wrapper.findComponent({ name: 'MeetingDetailView' }).exists()).toBe(false);
        expect(listMeetings).toHaveBeenCalled();
      });

      it('drops the row immediately, without waiting on the list reload', async () => {
        const wrapper = await mountWithRows([item({ id: 'a' }), item({ id: 'b' })]);
        await wrapper.get('.meeting-item').trigger('click');
        await flushPromises();
        // A reload that never settles: the row must still be gone from the UI.
        listMeetings.mockReturnValue(new Promise(() => {}));

        wrapper.findComponent({ name: 'MeetingDetailView' }).vm.$emit('deleted', { id: 'a' });
        await nextTick();

        expect(wrapper.findAll('.meeting-item')).toHaveLength(1);
      });

      it('does not autosave the notes of the recording it just deleted', async () => {
        const wrapper = await mountWithRows([item({ id: 'a' })]);
        await wrapper.get('.meeting-item').trigger('click');
        await flushPromises();
        saveNotesNow.mockClear();

        wrapper.findComponent({ name: 'MeetingDetailView' }).vm.$emit('deleted', { id: 'a' });
        await flushPromises();

        // The folder is gone; the usual pre-selection-change flush must be skipped.
        expect(saveNotesNow).not.toHaveBeenCalled();
      });
    });
  });

  it('strikes through the title of a canceled meeting only', async () => {
    listMeetings.mockResolvedValue([
      item({ id: 'a', title: 'Dropped Sync', canceled: true }),
      item({ id: 'b', title: 'Live Sync' }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    const titles = wrapper.findAll('.mi-title');
    const canceled = titles.find((t) => t.text() === 'Dropped Sync')!;
    const live = titles.find((t) => t.text() === 'Live Sync')!;
    expect(canceled.classes()).toContain('mi-title--canceled');
    expect(live.classes()).not.toContain('mi-title--canceled');
  });

  it('opens the floating recorder window forcing a new recording when the detail pane is empty', async () => {
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.find('.add-btn').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('start_recording_window', { forceNew: true });
  });

  it('shows an initializing state when a recorder window exists before capture starts', async () => {
    listMeetings.mockResolvedValue([]);
    getAllWebviewWindows.mockResolvedValue([{ label: 'waveform' }]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    // The sidebar collapses for the recording session.
    expect(wrapper.find('.sidebar').exists()).toBe(false);
    const btn = wrapper.find('.add-btn');
    expect(btn.exists()).toBe(true);
    expect(btn.text()).toContain('Starting recording');
    expect(btn.attributes('disabled')).toBeDefined();
  });

  it('hides the sidebar and shows immediate feedback after clicking Start', async () => {
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.find('.sidebar').exists()).toBe(true);
    await wrapper.find('.add-btn').trigger('click');
    await flushPromises();
    // Sidebar collapses right away and the command cannot be launched twice.
    expect(wrapper.find('.sidebar').exists()).toBe(false);
    expect(wrapper.find('.add-btn').text()).toContain('Starting recording');
    expect(wrapper.find('.add-btn').attributes('disabled')).toBeDefined();
  });

  it('shows initializing feedback for a recording started from the native menu', async () => {
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();

    emitEvent('recording://state', true);
    await flushPromises();

    expect(wrapper.find('.add-btn').text()).toContain('Starting recording');
    expect(wrapper.find('.add-btn').attributes('disabled')).toBeDefined();
  });

  it('refreshes pending uploads as soon as a cloud upload fails', async () => {
    backendId.mockReturnValue('ariso');
    usesMeetingPicker.mockReturnValue(true);
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(listPendingUploads).toHaveBeenCalledTimes(1);

    emitEvent('recording://state', true);
    await flushPromises();
    expect(wrapper.find('.sidebar').exists()).toBe(false);

    emitEvent('recorder://state', {
      bars: [], durationSeconds: 3, isPaused: false, meetingId: null, phase: 'failed',
    });
    await flushPromises();

    expect(listPendingUploads).toHaveBeenCalledTimes(2);
    expect(wrapper.find('.sidebar').exists()).toBe(true);
    expect(wrapper.find('.status-label').text()).toContain('Upload failed');
  });

  it('keeps Start disabled when native stop fires before the recorder window closes', async () => {
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();

    emitEvent('recording://state', true);
    emitEvent('recorder://state', {
      bars: [], durationSeconds: 3, isPaused: false, meetingId: null, phase: 'uploading',
    });
    await flushPromises();
    expect(wrapper.find('.add-btn').attributes('disabled')).toBeDefined();

    // Native capture has stopped, but the waveform still owns upload/retry.
    emitEvent('recording://state', false);
    await flushPromises();
    expect(wrapper.find('.add-btn').attributes('disabled')).toBeDefined();

    emitEvent('recorder://state', {
      bars: [], durationSeconds: 3, isPaused: false, meetingId: null, phase: 'closed',
    });
    await flushPromises();
    expect(wrapper.find('.add-btn').attributes('disabled')).toBeUndefined();
  });

  // Regression: stopping a recording from the in-library strip leaves the
  // window focused, so no focus event fires to reset the internal `recording`
  // flag. The Start button must NOT stay stuck disabled — disabling is driven
  // by the docked strip's presence, not by that flag.
  it('keeps the Start button usable after a recording ends without a refocus', async () => {
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Standup' })]);
    const wrapper = mountWithDetailStub();
    await flushPromises();
    await wrapper.find('.add-btn').trigger('click');
    await flushPromises();

    // Recording runs, then stops — the strip relays a 'recording' then 'closed'
    // heartbeat. No window 'focus' event is dispatched (the strip was in-window).
    emitEvent('recorder://state', localRecorderState());
    await flushPromises();
    emitEvent('recorder://state', localRecorderState({ phase: 'closed' }));
    await flushPromises();

    const btn = wrapper.find('.add-btn');
    expect(btn.attributes('disabled')).toBeUndefined();
    invoke.mockClear();
    await btn.trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('start_recording_window', {});
  });

  it('reloads meetings when the window regains focus (recorder finished)', async () => {
    listMeetings.mockResolvedValue([]);
    mount(LibraryView);
    await flushPromises();
    expect(listMeetings).toHaveBeenCalledTimes(1);

    window.dispatchEvent(new Event('focus'));
    await flushPromises();
    expect(listMeetings).toHaveBeenCalledTimes(2);
  });

  it('does not re-select a meeting on window focus once the user is on the Up Next view', async () => {
    listMeetings.mockResolvedValue([
      item({ id: 'a', title: 'Standup' }),
      item({ id: 'b', title: 'Planning' }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    // Initial mount lands on the first meeting.
    expect(wrapper.find('.meeting-item').attributes('aria-pressed')).toBe('true');

    // User closes the detail → the Up Next greeting/card view is shown.
    await wrapper.find('.btn-close').trigger('click');
    await flushPromises();
    expect(wrapper.find('.up-next').exists()).toBe(true);
    expect(wrapper.findAll('.meeting-item').every((r) => r.attributes('aria-pressed') === 'false')).toBe(true);

    // Regaining focus (e.g. switching back to the window, or clicking to move it)
    // reloads the list but must NOT yank the user back into a meeting.
    window.dispatchEvent(new Event('focus'));
    await flushPromises();
    expect(listMeetings).toHaveBeenCalledTimes(2);
    expect(wrapper.find('.up-next').exists()).toBe(true);
    expect(wrapper.findAll('.meeting-item').every((r) => r.attributes('aria-pressed') === 'false')).toBe(true);
  });

  it('closes the open meeting and reloads when the backend changes', async () => {
    listMeetings.mockResolvedValue([
      item({ id: 'a', title: 'Standup' }),
      item({ id: 'b', title: 'Planning' }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    // A meeting from the previous backend is open in the detail pane.
    expect(wrapper.find('.meeting-item').attributes('aria-pressed')).toBe('true');
    expect(wrapper.find('.up-next').exists()).toBe(false);

    // Switching backends (from the Settings window) must close that meeting,
    // returning the detail to the neutral Up Next state, and reload the list.
    emitEvent('backend://changed', {});
    await flushPromises();

    expect(listMeetings).toHaveBeenCalledTimes(2);
    expect(wrapper.find('.up-next').exists()).toBe(true);
    expect(wrapper.findAll('.meeting-item').every((r) => r.attributes('aria-pressed') === 'false')).toBe(true);
  });

  it('shows a distinct error message (not the empty state) when loading fails', async () => {
    listMeetings.mockRejectedValue(new Error('boom'));
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.text()).toContain('Could not load meetings');
    expect(wrapper.text()).not.toContain('No meetings yet');
  });

  function todayAt(hour: number): string {
    const d = new Date();
    return new Date(d.getFullYear(), d.getMonth(), d.getDate(), hour, 0, 0).toISOString();
  }

  it("Today tab filters the list to today's meetings and moves the active class", async () => {
    listMeetings.mockResolvedValue([
      item({ id: 'today', title: 'Today Standup', timestamp: todayAt(9) }),
      item({ id: 'old', title: 'Old Sync', timestamp: '2020-01-02T10:00:00Z' }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.findAll('.meeting-item')).toHaveLength(2);
    expect(wrapper.get('button[title="Meetings"]').classes()).toContain('nav-tab--active');

    await wrapper.get('button[title="Today"]').trigger('click');
    expect(wrapper.findAll('.meeting-item')).toHaveLength(1);
    expect(wrapper.text()).toContain('Today Standup');
    expect(wrapper.text()).not.toContain('Old Sync');
    expect(wrapper.get('button[title="Today"]').classes()).toContain('nav-tab--active');
    expect(wrapper.get('button[title="Meetings"]').classes()).not.toContain('nav-tab--active');
  });

  it('Meetings tab hides meetings scheduled beyond today', async () => {
    const inAWeek = new Date();
    inAWeek.setDate(inAWeek.getDate() + 7);
    listMeetings.mockResolvedValue([
      item({ id: 'today', title: 'Today Standup', timestamp: todayAt(9) }),
      item({ id: 'old', title: 'Old Sync', timestamp: '2020-01-02T10:00:00Z' }),
      item({ id: 'moved', title: 'Rescheduled Sync', timestamp: inAWeek.toISOString() }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.findAll('.meeting-item')).toHaveLength(2);
    expect(wrapper.text()).not.toContain('Rescheduled Sync');
  });

  it('Meetings tab shows a past-specific hint when only future meetings exist', async () => {
    const inAWeek = new Date();
    inAWeek.setDate(inAWeek.getDate() + 7);
    listMeetings.mockResolvedValue([
      item({ id: 'moved', title: 'Rescheduled Sync', timestamp: inAWeek.toISOString() }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.findAll('.meeting-item')).toHaveLength(0);
    expect(wrapper.text()).toContain('No past meetings.');
  });

  it('Today tab shows the empty hint when there are no meetings today', async () => {
    listMeetings.mockResolvedValue([item({ id: 'old', title: 'Old Sync', timestamp: '2020-01-02T10:00:00Z' })]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.get('button[title="Today"]').trigger('click');
    expect(wrapper.findAll('.meeting-item')).toHaveLength(0);
    expect(wrapper.text()).toContain('No meetings today.');
  });

  it('start-recording button opens the meeting picker for picker backends', async () => {
    backendId.mockReturnValue('ariso');
    usesMeetingPicker.mockReturnValue(true);
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.get('.add-btn').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('open_meeting_picker', {});
  });

  it('always opens the picker from the Meetings view, even with a meeting selected — defaulted to it', async () => {
    backendId.mockReturnValue('ariso');
    usesMeetingPicker.mockReturnValue(true);
    listMeetings.mockResolvedValue([
      item({ id: '42', title: 'Daily Plan', files: undefined }),
      item({ id: '7', title: 'Other Sync', files: undefined }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    // Meetings is the default view; the first row auto-selects (id 42), which
    // becomes the picker's default meeting.
    await wrapper.get('.add-btn').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('open_meeting_picker', { defaultMeetingId: 42 });
    expect(invoke).not.toHaveBeenCalledWith('start_recording_window', { meetingId: 42 });
  });

  it('Today view opens the picker defaulted to a deliberately selected today meeting', async () => {
    backendId.mockReturnValue('ariso');
    usesMeetingPicker.mockReturnValue(true);
    const start = new Date(Date.now() - 30 * 60_000).toISOString();
    const end = new Date(Date.now() + 30 * 60_000).toISOString();
    const earlierToday = new Date(new Date().setHours(7, 0, 0, 0)).toISOString();
    listMeetings.mockResolvedValue([
      item({ id: 'old', title: 'Old Sync', timestamp: '2020-01-02T10:00:00Z', files: undefined }),
      item({ id: '99', title: 'Live Standup', timestamp: start, endTimestamp: end, files: undefined }),
      item({ id: '50', title: 'Pick Me', timestamp: earlierToday, files: undefined }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.get('button[title="Today"]').trigger('click');
    await flushPromises();
    // Deliberately select a today meeting (id 50).
    const target = wrapper.findAll('.meeting-item').find((r) => r.text().includes('Pick Me'))!;
    await target.trigger('click');
    await flushPromises();
    await wrapper.get('.add-btn').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('open_meeting_picker', { defaultMeetingId: 50 });
    expect(invoke).not.toHaveBeenCalledWith('start_recording_window', { meetingId: 50 });
  });

  it('falls back to the meeting picker when the selected scheduled meeting id is not numeric', async () => {
    backendId.mockReturnValue('ariso');
    usesMeetingPicker.mockReturnValue(true);
    listMeetings.mockResolvedValue([item({ id: 'local-draft', title: 'Draft', files: undefined })]);
    const wrapper = mount(LibraryView);
    await flushPromises();

    await wrapper.get('.add-btn').trigger('click');
    await flushPromises();

    // The shown meeting (auto-selected) has a non-numeric id, so the picker
    // command — which only takes an id — gets none: the picker opens with no
    // default rather than a bogus/undefined one.
    expect(invoke).toHaveBeenCalledWith('open_meeting_picker', {});
    expect(invoke).not.toHaveBeenCalledWith('start_recording_window', {
      meetingId: expect.any(Number),
    });
  });

  it('marks the recording meeting with a red dot in the sidebar list', async () => {
    listMeetings.mockResolvedValue([
      item({ id: '42', title: 'Daily Plan' }),
      item({ id: '7', title: 'Other Sync' }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.find('.mi-rec-dot').exists()).toBe(false);

    // The recorder strip relays recorder://state to the library.
    emitEvent('recorder://state', {
      bars: [0, 0, 0],
      durationSeconds: 1,
      isPaused: false,
      meetingId: 42,
      phase: 'recording',
    });
    await flushPromises();
    const rows = wrapper.findAll('.meeting-item');
    expect(rows[0].find('.mi-rec-dot').exists()).toBe(true);
    expect(rows[1].find('.mi-rec-dot').exists()).toBe(false);
  });

  it('hides the red dot once the recording stops, even if the failed pill lingers', async () => {
    listMeetings.mockResolvedValue([
      item({ id: '42', title: 'Daily Plan' }),
      item({ id: '7', title: 'Other Sync' }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();

    emitEvent('recorder://state', {
      bars: [0, 0, 0],
      durationSeconds: 1,
      isPaused: false,
      meetingId: 42,
      phase: 'recording',
    });
    await flushPromises();
    expect(wrapper.findAll('.meeting-item')[0].find('.mi-rec-dot').exists()).toBe(true);

    // Upload failed: the pill stays open and keeps heartbeating 'failed', but the
    // recording has stopped — the row must no longer pulse.
    emitEvent('recorder://state', {
      bars: [0, 0, 0],
      durationSeconds: 1,
      isPaused: false,
      meetingId: 42,
      phase: 'failed',
    });
    await flushPromises();
    expect(wrapper.findAll('.meeting-item')[0].find('.mi-rec-dot').exists()).toBe(false);
  });

  it('selects the picked meeting in the detail panel when a recording starts', async () => {
    listMeetings.mockResolvedValue([item({ id: '42', title: 'Picked Sync' })]);
    const wrapper = mountWithDetailStub();
    await flushPromises();
    expect(wrapper.find('.up-next').exists()).toBe(false);

    emitEvent('recording://started', { meetingId: 42 });
    await flushPromises();
    expect(wrapper.find('.up-next').exists()).toBe(false);
    // The recording transition also collapses the sidebar immediately.
    expect(wrapper.find('.sidebar').exists()).toBe(false);
  });

  it('leaves the detail panel unchanged when a recording starts without a meeting', async () => {
    listMeetings.mockResolvedValue([item({ id: '42', title: 'Picked Sync' })]);
    const wrapper = mountWithDetailStub();
    await flushPromises();
    expect(wrapper.find('.up-next').exists()).toBe(false);

    emitEvent('recording://started', { meetingId: null });
    await flushPromises();
    expect(wrapper.find('.up-next').exists()).toBe(false);
    expect(wrapper.find('.sidebar').exists()).toBe(false);
  });

  it('reloads the meeting list when the picked meeting is not loaded yet', async () => {
    listMeetings
      .mockResolvedValueOnce([])
      .mockResolvedValue([item({ id: '42', title: 'Picked Sync' })]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(listMeetings).toHaveBeenCalledTimes(1);

    emitEvent('recording://started', { meetingId: 42 });
    await flushPromises();
    expect(listMeetings).toHaveBeenCalledTimes(2);
    expect(wrapper.find('.up-next').exists()).toBe(false);
  });

  // A local recording has no list row until finalize; the library synthesizes
  // one under the recording's deterministic id so the red dot, selection, and
  // the embedded strip have a home.
  function localRecorderState(over: Record<string, unknown> = {}) {
    return {
      bars: [0, 0, 0],
      durationSeconds: 1,
      isPaused: false,
      meetingId: null,
      localRecordingId: '2026-06-02T14-30-05Z',
      phase: 'recording',
      ...over,
    };
  }

  it('synthesizes a red-dot row for an in-progress local recording and selects it', async () => {
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Standup' })]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.findAll('.meeting-item')).toHaveLength(1);

    emitEvent('recorder://state', localRecorderState());
    await flushPromises();

    const rows = wrapper.findAll('.meeting-item');
    expect(rows).toHaveLength(2);
    // Within a date, rows sort earliest-first: Standup (10:00) then the
    // synthetic recording row (14:30).
    expect(rows[1].text()).toContain('Jun 2 @');
    expect(rows[1].find('.mi-rec-dot').exists()).toBe(true);
    expect(rows[1].attributes('aria-pressed')).toBe('true');
    // The embedded strip shows for the recorded meeting…
    expect(wrapper.find('.strip').exists()).toBe(true);
  });

  it('hides the recorder strip when another meeting is selected, keeping the red dot', async () => {
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Standup' })]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    emitEvent('recorder://state', localRecorderState());
    await flushPromises();
    expect(wrapper.find('.strip').exists()).toBe(true);

    // rows[0] is Standup (10:00); rows[1] is the recording (14:30). Click the
    // other meeting (Standup) to move selection off the recording.
    const rows = wrapper.findAll('.meeting-item');
    await rows[0].trigger('click');
    await flushPromises();

    expect(rows[0].attributes('aria-pressed')).toBe('true');
    expect(wrapper.find('.strip').exists()).toBe(false);
    expect(rows[1].find('.mi-rec-dot').exists()).toBe(true);
  });

  // Req 1: the floating recorder is docked in the detail pane → the titlebar
  // keeps the Start button but disables it so a second recording can't begin.
  it('disables the Start button while the recorder strip is docked in the detail pane', async () => {
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Standup' })]);
    const wrapper = mountWithDetailStub();
    await flushPromises();
    emitEvent('recorder://state', localRecorderState());
    await flushPromises();

    expect(wrapper.find('.strip').exists()).toBe(true);
    const btn = wrapper.find('.add-btn');
    expect(btn.exists()).toBe(true);
    expect(btn.text()).toContain('Start recording');
    expect(btn.attributes('disabled')).toBeDefined();
    expect(wrapper.find('.add-btn--recording').exists()).toBe(false);
  });

  // Req 2: recording continues but its meeting is no longer shown (the user
  // navigated to another meeting) → the Start button becomes a "Recording"
  // indicator.
  it('swaps the Start button for a "Recording" indicator when the recording is off-screen', async () => {
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Standup' })]);
    const wrapper = mountWithDetailStub();
    await flushPromises();
    emitEvent('recorder://state', localRecorderState());
    await flushPromises();

    // rows[0] is Standup (10:00); rows[1] is the recording (14:30). Move off the
    // recording onto Standup.
    const rows = wrapper.findAll('.meeting-item');
    await rows[0].trigger('click');
    await flushPromises();

    expect(wrapper.find('.strip').exists()).toBe(false);
    const pill = wrapper.find('.add-btn--recording');
    expect(pill.exists()).toBe(true);
    expect(pill.text()).toContain('In recording');
    // It renders the mini waveform, not the plain "Start recording" button.
    expect(pill.find('.rec-wave').exists()).toBe(true);
    expect(wrapper.find('.add-btn').text()).not.toContain('Start recording');
  });

  // Req 2: clicking the indicator re-docks the strip on the recording meeting.
  it('clicking the "Recording" indicator re-docks the strip on the recording meeting', async () => {
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Standup' })]);
    const wrapper = mountWithDetailStub();
    await flushPromises();
    emitEvent('recorder://state', localRecorderState());
    await flushPromises();
    await wrapper.findAll('.meeting-item')[0].trigger('click');
    await flushPromises();
    expect(wrapper.find('.add-btn--recording').exists()).toBe(true);
    expect(wrapper.find('.strip').exists()).toBe(false);

    await wrapper.find('.add-btn--recording').trigger('click');
    await flushPromises();

    expect(wrapper.find('.strip').exists()).toBe(true);
    expect(wrapper.find('.add-btn--recording').exists()).toBe(false);
    expect(wrapper.find('.add-btn').attributes('disabled')).toBeDefined();
  });

  // Regression (#318): a recording attached to no meeting at all (an Ariso
  // auto-recording that matched no calendar event stays that way for the whole
  // session) must never dock — the pill above a meeting reads as "this meeting
  // is being recorded", whichever meeting happens to be on screen.
  it('never docks an unattached recording, whichever meeting is on screen', async () => {
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Standup' }), item({ id: 'b', title: 'Planning' })]);
    const wrapper = mountWithDetailStub();
    await flushPromises();
    expect(wrapper.find('.detail-stub').attributes('data-meeting')).toBe('a');

    emitEvent('recorder://state', localRecorderState({ localRecordingId: null }));
    await flushPromises();
    expect(wrapper.find('.strip').exists()).toBe(false);
    expect(wrapper.find('.add-btn--recording').exists()).toBe(true);

    await wrapper.findAll('.meeting-item')[1].trigger('click');
    await flushPromises();
    expect(wrapper.find('.detail-stub').attributes('data-meeting')).toBe('b');
    expect(wrapper.find('.strip').exists()).toBe(false);
    expect(wrapper.find('.add-btn--recording').exists()).toBe(true);
  });

  // An unattached recording has no meeting to navigate back to, so the titlebar
  // indicator is a plain badge there — a button that silently does nothing on
  // click is worse than one that never invites the click.
  it('renders the "In recording" indicator as a badge when there is no meeting to return to', async () => {
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Standup' })]);
    const wrapper = mountWithDetailStub();
    await flushPromises();

    emitEvent('recorder://state', localRecorderState({ localRecordingId: null }));
    await flushPromises();
    const badge = wrapper.find('.add-btn--recording');
    expect(badge.exists()).toBe(true);
    expect(badge.text()).toContain('In recording');
    expect(badge.element.tagName).toBe('SPAN');

    // Once the recording resolves its id it has a meeting to return to, so the
    // indicator becomes a clickable button again. The resolved id synthesizes a
    // recording row and selects it, so step off it deliberately (by id, not by
    // row order) — on the recording's own row the strip docks and there is no
    // indicator to assert on.
    emitEvent('recorder://state', localRecorderState());
    await flushPromises();
    const standup = wrapper.findAll('.meeting-item').find((row) => row.text().includes('Standup'));
    await standup?.trigger('click');
    await flushPromises();
    expect(wrapper.find('.detail-stub').attributes('data-meeting')).toBe('a');
    expect(wrapper.find('.add-btn--recording').element.tagName).toBe('BUTTON');
  });

  // The docked strip and the titlebar indicator are two renderings of one rule
  // (LibraryView mirrors RecorderStrip's own visibility) — they must never both
  // show. An empty detail pane is where the two used to disagree.
  it('never shows the docked strip and the titlebar indicator at the same time', async () => {
    listMeetings.mockResolvedValue([]);
    const wrapper = mountWithDetailStub();
    await flushPromises();
    expect(wrapper.find('.detail-stub').exists()).toBe(false);

    emitEvent('recorder://state', localRecorderState({ localRecordingId: null }));
    await flushPromises();
    expect(wrapper.find('.strip').exists()).toBe(false);
    expect(wrapper.find('.add-btn--recording').exists()).toBe(true);
  });

  // Clicking the floating recorder pill emits `recording://reveal`; the open
  // library must jump back to the recording meeting (re-docking the strip).
  it('re-selects the recording meeting on a reveal event (recording pill clicked)', async () => {
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Standup' })]);
    const wrapper = mountWithDetailStub();
    await flushPromises();
    emitEvent('recorder://state', localRecorderState());
    await flushPromises();
    // Navigate off the recording (14:30) onto Standup (10:00).
    await wrapper.findAll('.meeting-item')[0].trigger('click');
    await flushPromises();
    expect(wrapper.find('.strip').exists()).toBe(false);

    emitEvent('recording://reveal', {});
    await flushPromises();

    expect(wrapper.find('.strip').exists()).toBe(true);
    expect(wrapper.find('.add-btn--recording').exists()).toBe(false);
  });

  // Req 2: closing the detail pane mid-recording also surfaces the indicator.
  it('shows the "Recording" indicator after the detail pane is closed mid-recording', async () => {
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Standup' })]);
    const wrapper = mountWithDetailStub();
    await flushPromises();
    emitEvent('recorder://state', localRecorderState());
    await flushPromises();
    expect(wrapper.find('.strip').exists()).toBe(true);

    await wrapper.find('.detail-stub .btn-close').trigger('click');
    await flushPromises();

    expect(wrapper.find('.strip').exists()).toBe(false);
    expect(wrapper.find('.add-btn--recording').exists()).toBe(true);
  });

  it('reloads when the recording closes so the finalized row replaces the synthetic one', async () => {
    listMeetings
      .mockResolvedValueOnce([item({ id: 'a', title: 'Standup' })])
      .mockResolvedValue([
        item({
          id: '2026-06-02T14-30-05Z',
          title: 'Recording 2026-06-02 14:30',
          timestamp: '2026-06-02T14:30:05Z',
          files: { hasAudio: true, hasNote: false, hasTranscript: true },
        }),
        item({ id: 'a', title: 'Standup' }),
      ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    emitEvent('recorder://state', localRecorderState());
    await flushPromises();
    expect(listMeetings).toHaveBeenCalledTimes(1);

    emitEvent('recorder://state', localRecorderState({ phase: 'closed' }));
    await flushPromises();

    expect(listMeetings).toHaveBeenCalledTimes(2);
    const rows = wrapper.findAll('.meeting-item');
    expect(rows).toHaveLength(2);
    // Earliest-first within the date keeps Standup (10:00) at rows[0] and the
    // finalized recording (14:30) at rows[1]; selection stays on the same id.
    expect(rows[1].attributes('aria-pressed')).toBe('true');
    expect(rows[1].find('.mi-rec-dot').exists()).toBe(false);
  });

  it('falls back to the first meeting when a discarded recording leaves no row', async () => {
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Standup' })]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    emitEvent('recorder://state', localRecorderState());
    await flushPromises();
    // The synthetic recording row (14:30) sorts after Standup (10:00) and holds
    // the selection at rows[1].
    expect(wrapper.findAll('.meeting-item')[1].attributes('aria-pressed')).toBe('true');

    emitEvent('recorder://state', localRecorderState({ phase: 'closed' }));
    await flushPromises();

    const rows = wrapper.findAll('.meeting-item');
    expect(rows).toHaveLength(1);
    expect(rows[0].attributes('aria-pressed')).toBe('true');
  });

  // An ad-hoc Ariso meeting (created via "Record a new meeting") isn't a
  // calendar-scheduled meeting, so listMeetings() never returns it. The library
  // pins it (fetching its metadata) so it stays in the sidebar after the
  // recording stops instead of vanishing on the post-recording reload.
  it('keeps an ad-hoc Ariso meeting in the list after its recording stops', async () => {
    backendId.mockReturnValue('ariso');
    usesMeetingPicker.mockReturnValue(true);
    // The calendar list never carries the ad-hoc meeting (id 77).
    listMeetings.mockResolvedValue([item({ id: '5', title: 'Calendar Sync', files: undefined })]);
    getMeetingDetail.mockImplementation((m) =>
      Promise.resolve({
        id: m.id,
        title: m.id === '77' ? 'My ad-hoc meeting' : m.title,
        startAt: '2026-06-02T14:30:00Z',
        participants: [],
        actionItems: [],
        isLocal: false,
      })
    );

    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.text()).not.toContain('My ad-hoc meeting');

    // Recording starts against the freshly created meeting 77.
    emitEvent('recorder://state', {
      bars: [0, 0, 0],
      durationSeconds: 1,
      isPaused: false,
      meetingId: 77,
      phase: 'recording',
    });
    await flushPromises();

    let adhoc = wrapper.findAll('.meeting-item').find((r) => r.text().includes('My ad-hoc meeting'));
    expect(adhoc).toBeTruthy();
    expect(adhoc!.find('.mi-rec-dot').exists()).toBe(true);

    // Recording stops — the strip clears, the library reloads (still no 77 from
    // the calendar), but the pinned row must remain.
    emitEvent('recorder://state', {
      bars: [0, 0, 0],
      durationSeconds: 1,
      isPaused: false,
      meetingId: 77,
      phase: 'closed',
    });
    await flushPromises();

    adhoc = wrapper.findAll('.meeting-item').find((r) => r.text().includes('My ad-hoc meeting'));
    expect(adhoc).toBeTruthy();
    expect(adhoc!.find('.mi-rec-dot').exists()).toBe(false);
  });

  it('unpins an ad-hoc meeting once the backend list starts returning it', async () => {
    backendId.mockReturnValue('ariso');
    usesMeetingPicker.mockReturnValue(true);
    // After the recording, a reload finally surfaces meeting 77 from the backend.
    listMeetings
      .mockResolvedValueOnce([item({ id: '5', title: 'Calendar Sync', files: undefined })])
      .mockResolvedValue([
        item({ id: '77', title: 'My ad-hoc meeting', timestamp: '2026-06-02T14:30:00Z', files: undefined }),
        item({ id: '5', title: 'Calendar Sync', files: undefined }),
      ]);
    getMeetingDetail.mockImplementation((m) =>
      Promise.resolve({
        id: m.id,
        title: m.id === '77' ? 'My ad-hoc meeting' : m.title,
        startAt: '2026-06-02T14:30:00Z',
        participants: [],
        actionItems: [],
        isLocal: false,
      })
    );

    const wrapper = mount(LibraryView);
    await flushPromises();

    emitEvent('recorder://state', {
      bars: [0, 0, 0], durationSeconds: 1, isPaused: false, meetingId: 77, phase: 'recording',
    });
    await flushPromises();
    emitEvent('recorder://state', {
      bars: [0, 0, 0], durationSeconds: 1, isPaused: false, meetingId: 77, phase: 'closed',
    });
    await flushPromises();

    // Exactly one row for 77 (the backend's), not a pinned duplicate.
    const adhocRows = wrapper.findAll('.meeting-item').filter((r) => r.text().includes('My ad-hoc meeting'));
    expect(adhocRows).toHaveLength(1);
  });

  it('start-recording button forces a new recording directly for local backend with an empty detail pane', async () => {
    usesMeetingPicker.mockReturnValue(false);
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.get('.add-btn').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('start_recording_window', { forceNew: true });
  });

  it('shows an actionable recorder startup failure and unlocks Start', async () => {
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();

    emitEvent('recording://state', true);
    emitEvent('recording://start-failed', {
      message: 'No microphone was found. Connect or enable a microphone, then try again.',
    });
    await flushPromises();

    expect(wrapper.get('.recording-start-error').attributes('role')).toBe('alert');
    expect(wrapper.get('.recording-start-error').text()).toContain('Connect or enable a microphone');
    expect(wrapper.get('.add-btn').attributes('disabled')).toBeUndefined();
  });

  // Starting a second recording while the first still holds the recorder pill:
  // the backend drops the queued request and reports which recording blocked it
  // and in what state, rather than leaving Start stuck forever (#320).
  it('names the meeting still uploading when a second recording is blocked', async () => {
    listMeetings.mockResolvedValue([item({ id: '42', title: 'Weekly Standup' })]);
    const wrapper = mount(LibraryView);
    await flushPromises();

    emitEvent('recording://state', true);
    emitEvent('recording://start-failed', { reason: 'uploading', meetingId: 42 });
    await flushPromises();

    const error = wrapper.get('.recording-start-error').text();
    expect(error).toContain('Weekly Standup');
    expect(error).toContain('still uploading');
    expect(wrapper.get('.add-btn').attributes('disabled')).toBeUndefined();
  });

  it('says the blocking meeting is still recording rather than uploading', async () => {
    listMeetings.mockResolvedValue([item({ id: '42', title: 'Weekly Standup' })]);
    const wrapper = mount(LibraryView);
    await flushPromises();

    emitEvent('recording://state', true);
    emitEvent('recording://start-failed', { reason: 'capturing', meetingId: 42 });
    await flushPromises();

    const error = wrapper.get('.recording-start-error').text();
    expect(error).toContain('Weekly Standup');
    expect(error).toContain('still recording');
    expect(error).not.toContain('uploading');
  });

  it('still explains a blocked start when the meeting is not in the list', async () => {
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();

    emitEvent('recording://state', true);
    emitEvent('recording://start-failed', { reason: 'uploading', meetingId: null });
    await flushPromises();

    expect(wrapper.get('.recording-start-error').text()).toContain('previous recording');
    expect(wrapper.get('.add-btn').attributes('disabled')).toBeUndefined();
  });

  it('renders the PendingUploads section inside the sidebar', async () => {
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView, {
      global: {
        stubs: {
          PendingUploads: { name: 'PendingUploads', template: '<div class="pending-stub" />' },
        },
      },
    });
    await flushPromises();
    expect(wrapper.find('.pending-stub').exists()).toBe(true);
  });

  it('notifies the recorder window when a sidebar pending upload succeeds', async () => {
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView, {
      global: {
        stubs: {
          PendingUploads: {
            name: 'PendingUploads',
            emits: ['uploaded'],
            template: '<div class="pending-stub" />',
          },
        },
      },
    });
    await flushPromises();
    emitAppEvent.mockClear();

    wrapper.findComponent({ name: 'PendingUploads' }).vm.$emit('uploaded');
    await flushPromises();

    // Broadcasts the stand-down so a still-open failed pill (mirrored into the
    // recorder strip) clears instead of lingering after a successful retry.
    expect(emitAppEvent).toHaveBeenCalledWith('pending-upload://succeeded');
  });

  it('confirms before recording an ariso auto-join meeting, and records only on confirm', async () => {
    backendId.mockReturnValue('ariso');
    usesMeetingPicker.mockReturnValue(true);
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();

    const upNext = wrapper.findComponent(UpNextCard);
    expect(upNext.exists()).toBe(true);

    // Emit "Start Meeting Early" for a flagged ariso meeting (numeric id).
    upNext.vm.$emit('start', item({ id: '42', autoJoinScheduled: true }));
    await flushPromises();

    // Dialog is shown; recording has NOT started yet.
    expect(wrapper.text()).toContain('Ari is scheduled to join this meeting');
    expect(invoke).not.toHaveBeenCalledWith('start_recording_window', { meetingId: 42 });

    // "Record anyway" proceeds.
    const buttons = wrapper.findAll('.ari-confirm__actions button');
    const recordBtn = buttons.find((b) => b.text() === 'Record anyway');
    expect(recordBtn).toBeTruthy();
    await recordBtn!.trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('start_recording_window', { meetingId: 42 });
  });

  it('records an unflagged ariso meeting immediately, no dialog', async () => {
    backendId.mockReturnValue('ariso');
    usesMeetingPicker.mockReturnValue(true);
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();

    const upNext = wrapper.findComponent(UpNextCard);
    upNext.vm.$emit('start', item({ id: '7', autoJoinScheduled: false }));
    await flushPromises();

    expect(wrapper.text()).not.toContain('Ari is scheduled to join this meeting');
    expect(invoke).toHaveBeenCalledWith('start_recording_window', { meetingId: 7 });
  });

  // Local, detail pane populated: Start keys on the shown meeting alone — no
  // New/Continue choice dialog. It fires the recorder directly, keeping the
  // 5-minute auto-append window (Rust decides append vs. new from that).
  it('local: with a meeting deliberately opened, Start records directly (no choice dialog)', async () => {
    backendId.mockReturnValue('local');
    usesMeetingPicker.mockReturnValue(false);
    listMeetings.mockResolvedValue([
      item({ id: '2026-06-02T10-00-00Z', title: 'Standup' }),
      item({ id: '2026-06-01T09-00-00Z', title: 'Older' }),
    ]);
    const wrapper = mountWithDetailStub();
    await flushPromises();

    // Deliberately select the first meeting (a user click, not the auto-select).
    const target = wrapper.findAll('.meeting-item').find((r) => r.text().includes('Standup'))!;
    await target.trigger('click');
    await flushPromises();

    await wrapper.get('.add-btn').trigger('click');
    await flushPromises();

    // No dialog — the recorder starts immediately, keeping the auto-append window.
    expect(wrapper.find('.rec-choice').exists()).toBe(false);
    expect(invoke).toHaveBeenCalledWith('start_recording_window', {});
  });

  it('ariso: deliberately opening a meeting then Start opens the picker defaulted to it', async () => {
    backendId.mockReturnValue('ariso');
    usesMeetingPicker.mockReturnValue(true);
    listMeetings.mockResolvedValue([
      item({ id: '42', title: 'Daily Plan', files: undefined }),
      item({ id: '7', title: 'Other Sync', files: undefined }),
    ]);
    const wrapper = mountWithDetailStub();
    await flushPromises();
    const target = wrapper.findAll('.meeting-item').find((r) => r.text().includes('Daily Plan'))!;
    await target.trigger('click');
    await flushPromises();
    await wrapper.get('.add-btn').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('open_meeting_picker', { defaultMeetingId: 42 });
  });
});

// Clicking a "Meeting prep ready" notification opens this window (or focuses
// it) and queues the prep id natively; the Library claims it and opens the
// prep's meeting with its Prep tab active.
describe('LibraryView meeting-prep notification', () => {
  function invokeWith(pendingPrepId: number | null) {
    invoke.mockImplementation((cmd: unknown) =>
      Promise.resolve(cmd === 'take_pending_meeting_prep' ? pendingPrepId : undefined)
    );
  }

  it('claims the queued prep on mount and opens that meeting on its Prep tab', async () => {
    backendId.mockReturnValue('ariso');
    invokeWith(5145);
    listMeetings.mockResolvedValue([
      item({ id: '1', title: 'Other', files: undefined }),
      item({ id: '45565', title: 'Standup', prepId: 5145, files: undefined }),
    ]);
    const wrapper = mountWithDetailStub();
    await flushPromises();

    expect(wrapper.find('.detail-stub').attributes('data-meeting')).toBe('45565');
    // The detail view needs the meeting the prep belongs to, so a request that
    // lands mid-load can't be answered by the previous meeting's pane.
    expect(openPrepTab).toHaveBeenCalledWith('45565');
    expect(openPrepTab).toHaveBeenCalledTimes(1);
  });

  it('falls back to the prep\'s meeting_id when no loaded row carries the prep id', async () => {
    backendId.mockReturnValue('ariso');
    invokeWith(5145);
    listMeetings.mockResolvedValue([
      item({ id: '45565', title: 'Standup', files: undefined }),
    ]);
    getMeetingPrep.mockResolvedValue({ content: '## P', meetingId: '45565' });
    const wrapper = mountWithDetailStub();
    await flushPromises();

    expect(getMeetingPrep).toHaveBeenCalledWith(5145);
    expect(wrapper.find('.detail-stub').attributes('data-meeting')).toBe('45565');
    expect(openPrepTab).toHaveBeenCalledTimes(1);
  });

  it('claims the prep on the event when the window was already open', async () => {
    backendId.mockReturnValue('ariso');
    invokeWith(null);
    listMeetings.mockResolvedValue([
      item({ id: '45565', title: 'Standup', prepId: 5145, files: undefined }),
    ]);
    const wrapper = mountWithDetailStub();
    await flushPromises();
    expect(openPrepTab).not.toHaveBeenCalled();

    invokeWith(5145);
    emitEvent('meeting-prep://open', {});
    await flushPromises();

    expect(wrapper.find('.detail-stub').attributes('data-meeting')).toBe('45565');
    expect(openPrepTab).toHaveBeenCalledTimes(1);
  });

  it('does nothing when nothing is queued', async () => {
    invokeWith(null);
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Standup' })]);
    mountWithDetailStub();
    await flushPromises();
    expect(openPrepTab).not.toHaveBeenCalled();
  });

  it('carries the prep id onto a loaded row that predates the prep', async () => {
    backendId.mockReturnValue('ariso');
    invokeWith(5145);
    listMeetings.mockResolvedValue([item({ id: '45565', title: 'Standup', files: undefined })]);
    getMeetingPrep.mockResolvedValue({ content: '## P', meetingId: '45565' });
    const wrapper = mountWithDetailStub();
    await flushPromises();

    // Without a prepId on the row the detail pane would render no Prep tab at
    // all — the notification already told us which prep this is.
    expect(wrapper.find('.detail-stub').attributes('data-prep')).toBe('5145');
  });

  it('pins and opens a prep whose meeting is outside the loaded list', async () => {
    backendId.mockReturnValue('ariso');
    invokeWith(5145);
    listMeetings.mockResolvedValue([item({ id: '1', title: 'Other', files: undefined })]);
    getMeetingPrep.mockResolvedValue({ content: '## P', meetingId: '99999' });
    getMeetingDetail.mockImplementation((meeting) =>
      Promise.resolve({
        id: meeting.id,
        title: 'Quarterly review',
        startAt: '2026-06-09T15:00:00Z',
        endAt: '2026-06-09T16:00:00Z',
        participants: [],
        actionItems: [],
        audioClips: [],
        isLocal: false,
      })
    );
    const wrapper = mountWithDetailStub();
    await flushPromises();

    expect(wrapper.find('.detail-stub').attributes('data-meeting')).toBe('99999');
    expect(wrapper.find('.detail-stub').attributes('data-prep')).toBe('5145');
    expect(openPrepTab).toHaveBeenCalledWith('99999');
  });

  // The tests above stub the detail pane; this one drives the real one, so the
  // hand-off (select the row → activate its Prep tab) is covered end to end.
  it('lands on the real detail view with the Prep tab showing the prep', async () => {
    backendId.mockReturnValue('ariso');
    invokeWith(5145);
    listMeetings.mockResolvedValue([
      item({ id: '1', title: 'Other', files: undefined }),
      item({ id: '45565', title: 'Standup', prepId: 5145, files: undefined }),
    ]);
    getMeetingDetail.mockImplementation((meeting) =>
      Promise.resolve({
        id: meeting.id,
        title: meeting.title,
        startAt: meeting.timestamp,
        participants: [],
        actionItems: [],
        audioClips: [],
        isLocal: false,
        digest: '## AI notes',
        prepId: meeting.prepId,
      })
    );
    getMeetingPrep.mockResolvedValue({ content: '## Talking points', meetingId: '45565' });

    const wrapper = mount(LibraryView);
    await flushPromises();

    expect(wrapper.find('.head-title').text()).toBe('Standup');
    expect(wrapper.find('.seg-btn--active').text()).toBe('Prep');
    expect(wrapper.find('.prep-pane').text()).toContain('Talking points');
  });

  it('highlights the prep\'s row in the sidebar, not just the detail pane', async () => {
    backendId.mockReturnValue('ariso');
    invokeWith(5145);
    // A prep lands shortly before its meeting starts. Pinned clock so "later
    // today" stays today in whatever timezone the suite runs in.
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.setSystemTime(new Date('2026-06-02T12:00:00Z'));
    listMeetings.mockResolvedValue([
      item({ id: '1', title: 'Yesterday standup', timestamp: '2026-06-01T10:00:00Z', files: undefined }),
      item({ id: '45565', title: 'Standup', timestamp: '2026-06-02T13:00:00Z', prepId: 5145, files: undefined }),
    ]);
    const wrapper = mountWithDetailStub();
    await flushPromises();

    const selected = wrapper.findAll('.meeting-item').filter((r) => r.classes('selected'));
    expect(selected).toHaveLength(1);
    expect(selected[0].text()).toContain('Standup');
    expect(wrapper.find('.detail-stub').attributes('data-meeting')).toBe('45565');
  });

  it('keeps the prep id on the selected row across a list refresh', async () => {
    backendId.mockReturnValue('ariso');
    invokeWith(5145);
    listMeetings.mockResolvedValue([item({ id: '45565', title: 'Standup', files: undefined })]);
    getMeetingPrep.mockResolvedValue({ content: '## P', meetingId: '45565' });
    const wrapper = mountWithDetailStub();
    await flushPromises();
    expect(wrapper.find('.detail-stub').attributes('data-prep')).toBe('5145');

    // Focusing the window (which a notification click does) refreshes the list;
    // the fresh row predates the prep and carries no prep_id.
    window.dispatchEvent(new Event('focus'));
    await flushPromises();

    expect(wrapper.find('.detail-stub').attributes('data-prep')).toBe('5145');
  });

  it('leaves the pane alone when the prep resolves to no reachable meeting', async () => {
    backendId.mockReturnValue('ariso');
    invokeWith(5145);
    listMeetings.mockResolvedValue([item({ id: '1', title: 'Other', files: undefined })]);
    getMeetingPrep.mockResolvedValue({ content: '## P', meetingId: '99999' });
    getMeetingDetail.mockRejectedValue(new Error('404'));
    const wrapper = mountWithDetailStub();
    await flushPromises();

    expect(openPrepTab).not.toHaveBeenCalled();
    // The auto-selected first row stays put — no meeting is yanked out from
    // under the user for a prep we can't place.
    expect(wrapper.find('.detail-stub').attributes('data-meeting')).toBe('1');
  });
});

describe('LibraryView Todo tab', () => {
  const daysAgo = (n: number) => {
    const d = new Date();
    d.setDate(d.getDate() - n);
    return d;
  };
  const todoButton = (wrapper: ReturnType<typeof mount>) =>
    wrapper.findAll('.nav-tab').find((b) => b.text().includes('Todo'))!;

  function actionItems(meeting: { id: string; title: string; timestamp: string }, ...items: string[]) {
    return [{ meeting, items: items.map((item) => ({ item })) }];
  }

  it('keeps the Todo tab disabled for a backend that has no action items', async () => {
    backendId.mockReturnValue('local');
    supportsActionItems.mockReturnValue(false);
    listMeetings.mockResolvedValue([]);

    const wrapper = mountWithDetailStub();
    await flushPromises();

    expect(todoButton(wrapper).attributes('disabled')).toBeDefined();
    expect(listActionItems).not.toHaveBeenCalled();
  });

  it('enables the Todo tab for a local backend that has vault tasks', async () => {
    backendId.mockReturnValue('local');
    supportsActionItems.mockReturnValue(true);
    listMeetings.mockResolvedValue([]);
    listActionItems.mockResolvedValue(
      actionItems(
        { id: '2026-06-02T14-30-05Z', title: 'Standup', timestamp: daysAgo(0).toISOString() },
        'Ship the RFC'
      )
    );

    const wrapper = mountWithDetailStub();
    await flushPromises();
    expect(todoButton(wrapper).attributes('disabled')).toBeUndefined();

    await todoButton(wrapper).trigger('click');
    await flushPromises();

    expect(wrapper.findAll('.todo-item')).toHaveLength(1);
    expect(wrapper.text()).toContain('Ship the RFC');
  });

  it('lists action items grouped by day when the tab is opened', async () => {
    backendId.mockReturnValue('ariso');
    supportsActionItems.mockReturnValue(true);
    listMeetings.mockResolvedValue([]);
    const today = daysAgo(0);
    const yesterday = daysAgo(1);
    listActionItems.mockResolvedValue([
      ...actionItems(
        { id: '9', title: 'Q3 Pricing Review', timestamp: today.toISOString() },
        'Send pricing deck',
        'Follow up with finance'
      ),
      ...actionItems(
        { id: '7', title: 'Platform Sync', timestamp: yesterday.toISOString() },
        'Draft migration plan'
      ),
    ]);

    const wrapper = mountWithDetailStub();
    await flushPromises();
    await todoButton(wrapper).trigger('click');
    await flushPromises();

    expect(listActionItems).toHaveBeenCalledTimes(1);

    const headers = wrapper.findAll('.group-label').map((h) => h.text());
    expect(headers[0]).toContain('TODAY');
    expect(headers[1]).toContain('YESTERDAY');

    const rows = wrapper.findAll('.todo-item');
    expect(rows).toHaveLength(3);
    expect(rows[0].text()).toContain('Send pricing deck');
    expect(rows[0].text()).toContain('Q3 Pricing Review');
    expect(rows[2].text()).toContain('Draft migration plan');
  });

  it('opens the source meeting in the detail pane when an action item is clicked', async () => {
    backendId.mockReturnValue('ariso');
    supportsActionItems.mockReturnValue(true);
    listMeetings.mockResolvedValue([]);
    listActionItems.mockResolvedValue(
      actionItems(
        { id: '9', title: 'Q3 Pricing Review', timestamp: daysAgo(0).toISOString() },
        'Send pricing deck'
      )
    );

    const wrapper = mountWithDetailStub();
    await flushPromises();
    await todoButton(wrapper).trigger('click');
    await flushPromises();
    await wrapper.findAll('.todo-item')[0].trigger('click');
    await flushPromises();

    expect(wrapper.find('.detail-stub').attributes('data-meeting')).toBe('9');
    expect(wrapper.findAll('.todo-item')[0].classes()).toContain('selected');
  });

  it('drops a todo ticked in the detail pane without flashing the loading state', async () => {
    backendId.mockReturnValue('local');
    supportsActionItems.mockReturnValue(true);
    listMeetings.mockResolvedValue([]);
    const meeting = { id: '2026-06-02T14-30-05Z', title: 'Standup', timestamp: daysAgo(0).toISOString() };
    listActionItems.mockResolvedValue(actionItems(meeting, 'Ship the RFC', 'Email legal'));

    const wrapper = mountWithDetailStub();
    await flushPromises();
    await todoButton(wrapper).trigger('click');
    await flushPromises();
    await wrapper.findAll('.todo-item')[0].trigger('click');
    await flushPromises();
    expect(wrapper.findAll('.todo-item')).toHaveLength(2);

    listActionItems.mockResolvedValue(actionItems(meeting, 'Email legal'));
    wrapper.findComponent({ name: 'MeetingDetailView' }).vm.$emit('tasks-changed', { id: meeting.id });
    await nextTick();
    expect(wrapper.text()).not.toContain('Loading…');
    await flushPromises();

    const rows = wrapper.findAll('.todo-item');
    expect(rows).toHaveLength(1);
    expect(rows[0].text()).toContain('Email legal');
    // The meeting stays open in the detail pane.
    expect(wrapper.find('.detail-stub').attributes('data-meeting')).toBe(meeting.id);
  });

  it('does not reload action items for a task ticked outside the Todo tab', async () => {
    backendId.mockReturnValue('local');
    supportsActionItems.mockReturnValue(true);
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Some meeting' })]);

    const wrapper = mountWithDetailStub();
    await flushPromises();
    await wrapper.findAll('.meeting-item')[0].trigger('click');
    await flushPromises();
    wrapper.findComponent({ name: 'MeetingDetailView' }).vm.$emit('tasks-changed', { id: 'a' });
    await flushPromises();

    expect(listActionItems).not.toHaveBeenCalled();
  });

  it('reports an error when the backend cannot load action items', async () => {
    backendId.mockReturnValue('ariso');
    supportsActionItems.mockReturnValue(true);
    listMeetings.mockResolvedValue([]);
    listActionItems.mockRejectedValue(new Error('offline'));

    const wrapper = mountWithDetailStub();
    await flushPromises();
    await todoButton(wrapper).trigger('click');
    await flushPromises();

    expect(wrapper.text()).toContain('Could not load action items.');
  });

  it('shows an empty state when the window has no action items', async () => {
    backendId.mockReturnValue('ariso');
    supportsActionItems.mockReturnValue(true);
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Some meeting' })]);
    listActionItems.mockResolvedValue([]);

    const wrapper = mountWithDetailStub();
    await flushPromises();
    await todoButton(wrapper).trigger('click');
    await flushPromises();

    expect(wrapper.text()).toContain('No action items.');
    expect(wrapper.findAll('.meeting-item')).toHaveLength(0);
  });

  it('falls back to the meetings list when the backend switches to one without action items', async () => {
    backendId.mockReturnValue('ariso');
    supportsActionItems.mockReturnValue(true);
    listMeetings.mockResolvedValue([]);
    listActionItems.mockResolvedValue(
      actionItems(
        { id: '9', title: 'Q3 Pricing Review', timestamp: daysAgo(0).toISOString() },
        'Send pricing deck'
      )
    );

    const wrapper = mountWithDetailStub();
    await flushPromises();
    await todoButton(wrapper).trigger('click');
    await flushPromises();
    expect(wrapper.findAll('.todo-item')).toHaveLength(1);

    backendId.mockReturnValue('local');
    supportsActionItems.mockReturnValue(false);
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Local recording' })]);
    emitEvent('backend://changed', null);
    await flushPromises();

    expect(wrapper.findAll('.todo-item')).toHaveLength(0);
    expect(wrapper.text()).toContain('Local recording');
    expect(todoButton(wrapper).classes()).not.toContain('nav-tab--active');
  });

  it('reloads the Todo tab when switching to a backend that also supports action items', async () => {
    backendId.mockReturnValue('ariso');
    supportsActionItems.mockReturnValue(true);
    listMeetings.mockResolvedValue([]);
    listActionItems.mockResolvedValue(
      actionItems(
        { id: '9', title: 'Q3 Pricing Review', timestamp: daysAgo(0).toISOString() },
        'Send pricing deck'
      )
    );

    const wrapper = mountWithDetailStub();
    await flushPromises();
    await todoButton(wrapper).trigger('click');
    await flushPromises();
    expect(wrapper.findAll('.todo-item')).toHaveLength(1);
    expect(wrapper.text()).toContain('Send pricing deck');

    backendId.mockReturnValue('local');
    supportsActionItems.mockReturnValue(true);
    listActionItems.mockResolvedValue(
      actionItems(
        { id: '2026-06-02T14-30-05Z', title: 'Standup', timestamp: daysAgo(0).toISOString() },
        'Ship the RFC'
      )
    );
    emitEvent('backend://changed', null);
    await flushPromises();

    expect(todoButton(wrapper).classes()).toContain('nav-tab--active');
    expect(wrapper.findAll('.todo-item')).toHaveLength(1);
    expect(wrapper.text()).toContain('Ship the RFC');
    expect(wrapper.text()).not.toContain('Send pricing deck');
  });

  it('drops a still-loading Todo request when the backend switches mid-load', async () => {
    backendId.mockReturnValue('ariso');
    supportsActionItems.mockReturnValue(true);
    listMeetings.mockResolvedValue([]);
    let resolveAriso!: (entries: ReturnType<typeof actionItems>) => void;
    listActionItems.mockReturnValueOnce(new Promise((resolve) => (resolveAriso = resolve)));

    const wrapper = mountWithDetailStub();
    await flushPromises();
    await todoButton(wrapper).trigger('click');
    await flushPromises();
    expect(wrapper.text()).toContain('Loading…');

    backendId.mockReturnValue('local');
    listActionItems.mockResolvedValue(
      actionItems(
        { id: '2026-06-02T14-30-05Z', title: 'Standup', timestamp: daysAgo(0).toISOString() },
        'Ship the RFC'
      )
    );
    emitEvent('backend://changed', null);
    await flushPromises();

    resolveAriso(
      actionItems(
        { id: '9', title: 'Q3 Pricing Review', timestamp: daysAgo(0).toISOString() },
        'Send pricing deck'
      )
    );
    await flushPromises();

    expect(listActionItems).toHaveBeenCalledTimes(2);
    expect(wrapper.text()).toContain('Ship the RFC');
    expect(wrapper.text()).not.toContain('Send pricing deck');
  });
});

describe('LibraryView organization branding', () => {
  const LOGO = 'data:image/png;base64,iVBORw0KGgo=';

  function mockOrg() {
    apiRequest.mockImplementation((_method: string, path: string) =>
      Promise.resolve(
        path === '/auth/me'
          ? { status: 200, data: { full_name: 'Ada Lovelace', email: 'ada@example.com' } }
          : path === '/organizations/info'
            ? { status: 200, data: { organization: { id: 1, name: 'Acme', logo_data: LOGO } } }
            : { status: 200, data: {} }
      )
    );
  }

  async function mountOn(backend: 'ariso' | 'local') {
    backendId.mockReturnValue(backend);
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    return wrapper;
  }

  function orgRequests() {
    return apiRequest.mock.calls.filter(([, path]) => path === '/organizations/info');
  }

  it('brands Up Next with the signed-in Ariso org', async () => {
    checkSession.mockResolvedValue({ sessionToken: 't' });
    mockOrg();
    const wrapper = await mountOn('ariso');

    expect(orgRequests()).toHaveLength(1);
    const upNext = wrapper.findComponent(UpNextCard);
    expect(upNext.props('orgName')).toBe('Acme');
    expect(upNext.props('orgLogo')).toBe(LOGO);
  });

  it('asks for no org while signed out', async () => {
    mockOrg();
    const wrapper = await mountOn('ariso');

    expect(orgRequests()).toHaveLength(0);
    expect(wrapper.findComponent(UpNextCard).props('orgName')).toBe('');
  });

  it('never asks for the org on Local', async () => {
    mockOrg();
    const wrapper = await mountOn('local');

    expect(orgRequests()).toHaveLength(0);
    expect(wrapper.findComponent(UpNextCard).props('orgName')).toBe('');
  });

  it('drops the org on a switch to Local and refetches on the way back', async () => {
    checkSession.mockResolvedValue({ sessionToken: 't' });
    mockOrg();
    const wrapper = await mountOn('ariso');
    expect(wrapper.findComponent(UpNextCard).props('orgName')).toBe('Acme');

    backendId.mockReturnValue('local');
    emitEvent('backend://changed', null);
    await flushPromises();
    expect(wrapper.findComponent(UpNextCard).props('orgName')).toBe('');
    expect(wrapper.findComponent(UpNextCard).props('orgLogo')).toBe('');

    backendId.mockReturnValue('ariso');
    emitEvent('backend://changed', null);
    await flushPromises();
    expect(orgRequests()).toHaveLength(2);
    expect(wrapper.findComponent(UpNextCard).props('orgName')).toBe('Acme');
  });

  it('drops the org when the session ends elsewhere', async () => {
    checkSession.mockResolvedValue({ sessionToken: 't' });
    mockOrg();
    const wrapper = await mountOn('ariso');
    expect(wrapper.findComponent(UpNextCard).props('orgName')).toBe('Acme');

    checkSession.mockResolvedValue(null);
    emitEvent('auth://changed', null);
    await flushPromises();
    expect(wrapper.findComponent(UpNextCard).props('orgName')).toBe('');
  });
});

describe('LibraryView backend indicator', () => {
  function mockProfile() {
    apiRequest.mockImplementation((_method: string, path: string) =>
      Promise.resolve(
        path === '/auth/me'
          ? { status: 200, data: { full_name: 'Ada Lovelace', email: 'ada@example.com' } }
          : { status: 200, data: {} }
      )
    );
  }

  async function mountOn(backend: 'ariso' | 'local') {
    backendId.mockReturnValue(backend);
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView, { attachTo: document.body });
    await flushPromises();
    return wrapper;
  }

  function pill(wrapper: ReturnType<typeof mount>) {
    return wrapper.get('.account-pill-wrap .account-pill');
  }

  function menuItem(wrapper: ReturnType<typeof mount>, label: string) {
    const item = wrapper
      .findAll('.backend-menu [role^="menuitem"]')
      .find((el) => el.text() === label);
    expect(item, `menu item "${label}"`).toBeDefined();
    return item!;
  }

  async function openMenu(wrapper: ReturnType<typeof mount>) {
    await pill(wrapper).trigger('click');
    await flushPromises();
  }

  it('sits in the titlebar, right after the sidebar toggle', async () => {
    const wrapper = await mountOn('local');

    const wrap = wrapper.get('.titlebar .account-pill-wrap').element;
    expect(wrap.previousElementSibling?.classList.contains('panel-toggle')).toBe(true);
    expect(wrapper.find('.sidebar .account-pill').exists()).toBe(false);
  });

  it('stays visible with the sidebar hidden', async () => {
    const wrapper = await mountOn('local');

    await wrapper.get('.panel-toggle').trigger('click');

    expect(wrapper.find('.sidebar').exists()).toBe(false);
    expect(pill(wrapper).text()).toBe('Local');
  });

  it('names the backend, with its icon after the name', async () => {
    const local = await mountOn('local');
    expect(pill(local).text()).toBe('Local');
    expect(pill(local).attributes('title')).toContain('Local mode');
    expect(pill(local).element.lastElementChild?.tagName.toLowerCase()).toBe('svg');
    local.unmount();

    const ariso = await mountOn('ariso');
    expect(pill(ariso).text()).toBe('ariso.ai');
    expect(pill(ariso).attributes('title')).toBe('ariso.ai — not signed in');
    expect(pill(ariso).element.lastElementChild?.tagName.toLowerCase()).toBe('svg');
  });

  it('carries the signed-in account in the tooltip', async () => {
    checkSession.mockResolvedValue({ sessionToken: 't' });
    mockProfile();
    const wrapper = await mountOn('ariso');

    expect(pill(wrapper).text()).toBe('ariso.ai');
    expect(pill(wrapper).attributes('title')).toBe('ariso.ai — ada@example.com');
  });

  it('opens a menu of ariso.ai, Local, and Settings, each with its icon', async () => {
    const wrapper = await mountOn('local');
    expect(pill(wrapper).attributes('aria-expanded')).toBe('false');

    await openMenu(wrapper);

    const menu = wrapper.get('.backend-menu');
    expect(menu.attributes('role')).toBe('menu');
    const items = menu.findAll('[role^="menuitem"]');
    expect(items.map((el) => el.text())).toEqual(['ariso.ai', 'Local', 'Settings']);
    for (const el of items) expect(el.find('svg').exists()).toBe(true);
    expect(pill(wrapper).attributes('aria-expanded')).toBe('true');
    // The active backend is checked, and focus lands on it.
    expect(menuItem(wrapper, 'Local').attributes('aria-checked')).toBe('true');
    expect(menuItem(wrapper, 'ariso.ai').attributes('aria-checked')).toBe('false');
    expect(document.activeElement).toBe(menuItem(wrapper, 'Local').element);
  });

  it('grays the ariso.ai option while not signed in, but keeps it clickable', async () => {
    const wrapper = await mountOn('ariso');
    await openMenu(wrapper);

    const ariso = menuItem(wrapper, 'ariso.ai');
    expect(ariso.classes()).toContain('backend-menu-item--signed-out');
    expect(ariso.attributes('disabled')).toBeUndefined();
    expect(ariso.attributes('aria-label')).toBe('ariso.ai, not signed in');
    expect(menuItem(wrapper, 'Local').classes()).not.toContain('backend-menu-item--signed-out');

    await ariso.trigger('click');
    await flushPromises();
    expect(wrapper.find('.sign-in-popover').exists()).toBe(true);
  });

  it('shows the ariso.ai option normally once signed in', async () => {
    checkSession.mockResolvedValue({ sessionToken: 't' });
    mockProfile();
    const wrapper = await mountOn('ariso');
    await openMenu(wrapper);

    const ariso = menuItem(wrapper, 'ariso.ai');
    expect(ariso.classes()).not.toContain('backend-menu-item--signed-out');
    expect(ariso.attributes('aria-label')).toBeUndefined();
  });

  it('grays the ariso.ai option on Local, which never checks the session', async () => {
    const wrapper = await mountOn('local');
    await openMenu(wrapper);

    expect(menuItem(wrapper, 'ariso.ai').classes()).toContain('backend-menu-item--signed-out');
    expect(checkSession).not.toHaveBeenCalled();
  });

  it('opening the menu on Local asks nothing of the network', async () => {
    const wrapper = await mountOn('local');
    await openMenu(wrapper);
    emitEvent('auth://changed', null);
    await flushPromises();

    expect(checkSession).not.toHaveBeenCalled();
    expect(apiRequest).not.toHaveBeenCalled();
  });

  it('moves through the menu with the arrow keys', async () => {
    const wrapper = await mountOn('local');
    await openMenu(wrapper);
    const menu = wrapper.get('.backend-menu');

    await menu.trigger('keydown', { key: 'ArrowDown' });
    expect(document.activeElement).toBe(menuItem(wrapper, 'Settings').element);
    await menu.trigger('keydown', { key: 'ArrowDown' });
    expect(document.activeElement).toBe(menuItem(wrapper, 'ariso.ai').element);
    await menu.trigger('keydown', { key: 'ArrowUp' });
    expect(document.activeElement).toBe(menuItem(wrapper, 'Settings').element);
  });

  it('Settings opens the Settings window', async () => {
    const wrapper = await mountOn('ariso');
    await openMenu(wrapper);

    await menuItem(wrapper, 'Settings').trigger('click');
    await flushPromises();

    expect(invoke).toHaveBeenCalledWith('create_settings_window', {});
    expect(wrapper.find('.backend-popover').exists()).toBe(false);
  });

  it('Local switches the backend and reloads against it', async () => {
    const wrapper = await mountOn('ariso');
    await openMenu(wrapper);
    listMeetings.mockClear();
    checkSession.mockClear();
    backendId.mockReturnValue('local');

    await menuItem(wrapper, 'Local').trigger('click');
    await flushPromises();

    expect(setBackendSetting).toHaveBeenCalledWith('local');
    expect(emitAppEvent).toHaveBeenCalledWith(
      'backend://changed',
      expect.objectContaining({ source: expect.any(String) })
    );
    expect(emitNotificationsSync).toHaveBeenCalled();
    expect(listMeetings).toHaveBeenCalled();
    expect(pill(wrapper).text()).toBe('Local');
    expect(checkSession).not.toHaveBeenCalled();
    expect(wrapper.find('.backend-popover').exists()).toBe(false);
    // Models are ready, so there's nothing for Settings to show.
    expect(invoke).not.toHaveBeenCalledWith('create_settings_window', {});
  });

  it('Local brings up Settings when the on-device models still need downloading', async () => {
    localModelStatus.mockResolvedValue({ state: 'not_downloaded' });
    const wrapper = await mountOn('ariso');
    await openMenu(wrapper);
    backendId.mockReturnValue('local');

    await menuItem(wrapper, 'Local').trigger('click');
    await flushPromises();

    expect(setBackendSetting).toHaveBeenCalledWith('local');
    expect(invoke).toHaveBeenCalledWith('create_settings_window', {});
  });

  it('Local does nothing when it is already the backend', async () => {
    const wrapper = await mountOn('local');
    await openMenu(wrapper);

    await menuItem(wrapper, 'Local').trigger('click');
    await flushPromises();

    expect(setBackendSetting).not.toHaveBeenCalled();
    expect(wrapper.find('.backend-popover').exists()).toBe(false);
  });

  it('ignores its own backend broadcast when it comes back', async () => {
    const wrapper = await mountOn('ariso');
    await openMenu(wrapper);
    backendId.mockReturnValue('local');
    await menuItem(wrapper, 'Local').trigger('click');
    await flushPromises();
    listMeetings.mockClear();

    const [, payload] = emitAppEvent.mock.calls.find((call) => call[0] === 'backend://changed')!;
    emitEvent('backend://changed', payload);
    await flushPromises();

    expect(listMeetings).not.toHaveBeenCalled();
  });

  it('ariso.ai while signed out shows the sign-in box', async () => {
    const wrapper = await mountOn('ariso');
    await openMenu(wrapper);

    await menuItem(wrapper, 'ariso.ai').trigger('click');
    await flushPromises();

    expect(setBackendSetting).not.toHaveBeenCalled();
    expect(wrapper.find('.backend-menu').exists()).toBe(false);
    const box = wrapper.get('.sign-in-popover');
    expect(box.attributes('role')).toBe('dialog');
    expect(box.text()).toContain('Sign in to ariso.ai');
    expect(box.get('.google-btn').text()).toContain('Sign in with Google');
    expect(box.get('.microsoft-btn').text()).toContain('Sign in with Microsoft');
    expect(document.activeElement).toBe(box.get('.google-btn').element);
  });

  it('ariso.ai while signed in just closes the menu', async () => {
    checkSession.mockResolvedValue({ sessionToken: 't' });
    mockProfile();
    const wrapper = await mountOn('ariso');
    await openMenu(wrapper);

    await menuItem(wrapper, 'ariso.ai').trigger('click');
    await flushPromises();

    expect(wrapper.find('.backend-popover').exists()).toBe(false);
    expect(document.activeElement).toBe(pill(wrapper).element);
  });

  it('ariso.ai from Local switches, then offers sign-in when there is no session', async () => {
    const wrapper = await mountOn('local');
    await openMenu(wrapper);
    backendId.mockReturnValue('ariso');

    await menuItem(wrapper, 'ariso.ai').trigger('click');
    await flushPromises();

    expect(setBackendSetting).toHaveBeenCalledWith('ariso');
    expect(pill(wrapper).text()).toBe('ariso.ai');
    expect(checkSession).toHaveBeenCalledTimes(1);
    expect(wrapper.find('.sign-in-popover').exists()).toBe(true);
  });

  it('ariso.ai from Local closes the menu when a session already exists', async () => {
    checkSession.mockResolvedValue({ sessionToken: 't' });
    mockProfile();
    const wrapper = await mountOn('local');
    await openMenu(wrapper);
    backendId.mockReturnValue('ariso');

    await menuItem(wrapper, 'ariso.ai').trigger('click');
    await flushPromises();

    expect(setBackendSetting).toHaveBeenCalledWith('ariso');
    expect(wrapper.find('.backend-popover').exists()).toBe(false);
    expect(pill(wrapper).attributes('title')).toBe('ariso.ai — ada@example.com');
  });

  it('cannot switch backend while recording', async () => {
    getAllWebviewWindows.mockResolvedValue([{ label: 'waveform' }]);
    const wrapper = await mountOn('ariso');
    await openMenu(wrapper);

    expect(menuItem(wrapper, 'ariso.ai').attributes('disabled')).toBeDefined();
    expect(menuItem(wrapper, 'Local').attributes('disabled')).toBeDefined();
    expect(menuItem(wrapper, 'Settings').attributes('disabled')).toBeUndefined();
    expect(wrapper.get('.backend-menu').text()).toContain("Backend can't be changed while recording.");
  });

  it('signs in from the sign-in box and closes it', async () => {
    const wrapper = await mountOn('ariso');
    await openMenu(wrapper);
    await menuItem(wrapper, 'ariso.ai').trigger('click');
    await flushPromises();

    microsoftSignIn.mockImplementation(() => {
      checkSession.mockResolvedValue({ sessionToken: 't' });
      return Promise.resolve({ success: true });
    });
    mockProfile();
    await wrapper.get('.sign-in-popover .microsoft-btn').trigger('click');
    await flushPromises();

    expect(microsoftSignIn).toHaveBeenCalledTimes(1);
    expect(googleSignIn).not.toHaveBeenCalled();
    expect(emitNotificationsSync).toHaveBeenCalled();
    expect(wrapper.find('.backend-popover').exists()).toBe(false);
    expect(pill(wrapper).attributes('title')).toBe('ariso.ai — ada@example.com');
  });

  it('shows the pending flow with a Cancel, and a failure inline', async () => {
    let resolveSignIn!: (r: SignInResult) => void;
    googleSignIn.mockReturnValue(new Promise((resolve) => (resolveSignIn = resolve)));
    const wrapper = await mountOn('ariso');
    await openMenu(wrapper);
    await menuItem(wrapper, 'ariso.ai').trigger('click');
    await flushPromises();
    await wrapper.get('.sign-in-popover .google-btn').trigger('click');
    await flushPromises();

    expect(wrapper.get('.sign-in-popover .google-btn').text()).toContain('Continue in your browser…');
    await wrapper.get('.sign-in-popover .sign-in-cancel').trigger('click');
    expect(cancelSignIn).toHaveBeenCalledTimes(1);

    resolveSignIn({ error: 'API returned 500' });
    await flushPromises();
    expect(wrapper.get('.sign-in-popover .sign-in-error').text()).toBe('API returned 500');
  });

  it('closes on Escape and returns focus to the pill', async () => {
    const wrapper = await mountOn('ariso');
    await openMenu(wrapper);

    await menuItem(wrapper, 'Local').trigger('keydown', { key: 'Escape' });

    expect(wrapper.find('.backend-popover').exists()).toBe(false);
    expect(document.activeElement).toBe(pill(wrapper).element);
  });

  it('closes on an outside click while idle', async () => {
    const wrapper = await mountOn('ariso');
    await openMenu(wrapper);

    wrapper.get('.backend-menu').element.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    await flushPromises();
    expect(wrapper.find('.backend-popover').exists()).toBe(true);

    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    await flushPromises();
    expect(wrapper.find('.backend-popover').exists()).toBe(false);
  });

  it('keeps the sign-in box open on an outside click while a flow is pending', async () => {
    googleSignIn.mockReturnValue(new Promise(() => {}));
    const wrapper = await mountOn('ariso');
    await openMenu(wrapper);
    await menuItem(wrapper, 'ariso.ai').trigger('click');
    await flushPromises();
    await wrapper.get('.sign-in-popover .google-btn').trigger('click');

    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    await flushPromises();
    expect(wrapper.find('.sign-in-popover').exists()).toBe(true);
  });

  it('follows an auth broadcast without a remount', async () => {
    const wrapper = await mountOn('ariso');
    await openMenu(wrapper);
    await menuItem(wrapper, 'ariso.ai').trigger('click');
    await flushPromises();
    expect(wrapper.find('.sign-in-popover').exists()).toBe(true);

    // Signed in from Settings or a tray row.
    checkSession.mockResolvedValue({ sessionToken: 't' });
    mockProfile();
    emitEvent('auth://changed', null);
    await flushPromises();
    expect(pill(wrapper).attributes('title')).toBe('ariso.ai — ada@example.com');
    expect(wrapper.find('.sign-in-popover').exists()).toBe(false);

    // Then signed out elsewhere.
    checkSession.mockResolvedValue(null);
    emitEvent('auth://changed', null);
    await flushPromises();
    expect(pill(wrapper).attributes('title')).toBe('ariso.ai — not signed in');
  });

  it('follows a backend switch made elsewhere, and asks nothing on Local', async () => {
    checkSession.mockResolvedValue({ sessionToken: 't' });
    mockProfile();
    const wrapper = await mountOn('ariso');

    backendId.mockReturnValue('local');
    checkSession.mockClear();
    apiRequest.mockClear();
    emitEvent('backend://changed', null);
    await flushPromises();
    expect(pill(wrapper).text()).toBe('Local');
    expect(checkSession).not.toHaveBeenCalled();
    expect(apiRequest).not.toHaveBeenCalled();

    // Back to Ariso: re-checked, not the stale signed-in state from before.
    checkSession.mockResolvedValue(null);
    backendId.mockReturnValue('ariso');
    emitEvent('backend://changed', null);
    await flushPromises();
    expect(checkSession).toHaveBeenCalledTimes(1);
    expect(pill(wrapper).attributes('title')).toBe('ariso.ai — not signed in');
  });

  it('closes the sign-in box and cancels its pending flow on a switch to Local', async () => {
    googleSignIn.mockReturnValue(new Promise(() => {}));
    const wrapper = await mountOn('ariso');
    await openMenu(wrapper);
    await menuItem(wrapper, 'ariso.ai').trigger('click');
    await flushPromises();
    await wrapper.get('.sign-in-popover .google-btn').trigger('click');
    await flushPromises();

    backendId.mockReturnValue('local');
    emitEvent('backend://changed', null);
    await flushPromises();

    expect(cancelSignIn).toHaveBeenCalledTimes(1);
    expect(wrapper.find('.sign-in-popover').exists()).toBe(false);
    expect(pill(wrapper).text()).toBe('Local');
  });

  it('does not cancel on a switch to Local when this window has no flow pending', async () => {
    const wrapper = await mountOn('ariso');

    backendId.mockReturnValue('local');
    emitEvent('backend://changed', null);
    await flushPromises();

    // Another window's flow is not this window's to cancel.
    expect(cancelSignIn).not.toHaveBeenCalled();
  });
});
