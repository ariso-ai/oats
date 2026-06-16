// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';

const listMeetings = vi.fn();
const searchMeetings = vi.fn();
const getMeetingDetail = vi.fn();
const backendId = vi.fn(() => 'local');
const usesMeetingPicker = vi.fn(() => false);
const supportsSearch = vi.fn(() => false);
const openRecordingFile = vi.fn();
const readRecordingAudio = vi.fn();
const readRecordingNote = vi.fn();
const writeRecordingNote = vi.fn();
const invoke = vi.fn(() => Promise.resolve());
const getAllWebviewWindows = vi.fn(() => Promise.resolve([] as { label: string }[]));
const emitNotificationsSync = vi.fn(() => Promise.resolve());

vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock('@tauri-apps/api/webviewWindow', () => ({
  getAllWebviewWindows: () => getAllWebviewWindows(),
}));

// In-test event bus standing in for Tauri's app-wide events: `listen` records
// handlers, `emitEvent` drives them the way the Rust side would.
type EventHandler = (e: { payload: unknown }) => void;
const eventHandlers = new Map<string, EventHandler[]>();
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
        listMeetings: () => listMeetings(),
        searchMeetings: (query: string) => searchMeetings(query),
        getMeetingDetail: (meeting: unknown) => getMeetingDetail(meeting),
      }),
  };
});
vi.mock('../composables/useMeetingNotifications', () => ({
  emitNotificationsSync: () => emitNotificationsSync(),
}));
// RecordingAudioPlayer (rendered for local rows) and openNote/openTranscript go
// through ../tauri; keep those mocked so jsdom never touches real IPC.
// pending.list() is also mocked so the PendingUploads child never calls Tauri IPC.
vi.mock('../tauri', () => ({
  local: {
    openRecordingFile: (id: string, kind: string) => openRecordingFile(id, kind),
    readRecordingAudio: (id: string) => readRecordingAudio(id),
    readRecordingNote: (id: string) => readRecordingNote(id),
    writeRecordingNote: (id: string, markdown: string) => writeRecordingNote(id, markdown),
  },
  pending: {
    list: () => Promise.resolve([]),
  },
}));

import LibraryView from './LibraryView.vue';

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
      isLocal: true,
      durationSeconds: meeting.durationSeconds,
      hasTranscript: meeting.files?.hasTranscript ?? false,
    })
  );
  invoke.mockResolvedValue(undefined);
  readRecordingNote.mockResolvedValue('');
  writeRecordingNote.mockResolvedValue(undefined);
  backendId.mockReturnValue('local');
  usesMeetingPicker.mockReturnValue(false);
  supportsSearch.mockReturnValue(true);
  searchMeetings.mockResolvedValue([]);
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('LibraryView', () => {
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

  it('opens the floating recorder window when the add button is clicked', async () => {
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.find('.add-btn').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('start_recording_window', {});
  });

  it('hides the sidebar (and add button) while a recording (waveform window) is active', async () => {
    listMeetings.mockResolvedValue([]);
    getAllWebviewWindows.mockResolvedValue([{ label: 'waveform' }]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.find('.add-btn').exists()).toBe(false);
  });

  it('hides the sidebar immediately after clicking the add button', async () => {
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.find('.add-btn').exists()).toBe(true);
    await wrapper.find('.add-btn').trigger('click');
    await flushPromises();
    expect(wrapper.find('.add-btn').exists()).toBe(false);
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

  it('always opens the picker from the Meetings view, even with a meeting selected', async () => {
    backendId.mockReturnValue('ariso');
    usesMeetingPicker.mockReturnValue(true);
    listMeetings.mockResolvedValue([
      item({ id: '42', title: 'Daily Plan', files: undefined }),
      item({ id: '7', title: 'Other Sync', files: undefined }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    // Meetings is the default view; the first row auto-selects (id 42).
    await wrapper.get('.add-btn').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('open_meeting_picker', {});
    expect(invoke).not.toHaveBeenCalledWith('start_recording_window', { meetingId: 42 });
  });

  it('Today view records the in-progress meeting when nothing today is selected', async () => {
    backendId.mockReturnValue('ariso');
    usesMeetingPicker.mockReturnValue(true);
    const start = new Date(Date.now() - 30 * 60_000).toISOString();
    const end = new Date(Date.now() + 30 * 60_000).toISOString();
    listMeetings.mockResolvedValue([
      // meetings[0] auto-selects but is NOT today, so it can't override.
      item({ id: 'old', title: 'Old Sync', timestamp: '2020-01-02T10:00:00Z', files: undefined }),
      item({ id: '99', title: 'Live Standup', timestamp: start, endTimestamp: end, files: undefined }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.get('button[title="Today"]').trigger('click');
    await wrapper.get('.add-btn').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('start_recording_window', { meetingId: 99 });
    expect(invoke).not.toHaveBeenCalledWith('open_meeting_picker', {});
  });

  it('Today view opens the picker when no meeting is live and none is selected today', async () => {
    backendId.mockReturnValue('ariso');
    usesMeetingPicker.mockReturnValue(true);
    listMeetings.mockResolvedValue([
      item({ id: 'old', title: 'Old Sync', timestamp: '2020-01-02T10:00:00Z', files: undefined }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.get('button[title="Today"]').trigger('click');
    await wrapper.get('.add-btn').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('open_meeting_picker', {});
  });

  it('Today view records a deliberately selected today meeting (override beats the live one)', async () => {
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
    // Deliberately select the non-live today meeting (id 50).
    const target = wrapper.findAll('.meeting-item').find((r) => r.text().includes('Pick Me'))!;
    await target.trigger('click');
    await flushPromises();
    await wrapper.get('.add-btn').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('start_recording_window', { meetingId: 50 });
    expect(invoke).not.toHaveBeenCalledWith('start_recording_window', { meetingId: 99 });
  });

  it('falls back to the meeting picker when the selected scheduled meeting id is not numeric', async () => {
    backendId.mockReturnValue('ariso');
    usesMeetingPicker.mockReturnValue(true);
    listMeetings.mockResolvedValue([item({ id: 'local-draft', title: 'Draft', files: undefined })]);
    const wrapper = mount(LibraryView);
    await flushPromises();

    await wrapper.get('.add-btn').trigger('click');
    await flushPromises();

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
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.find('.up-next').exists()).toBe(false);

    emitEvent('recording://started', { meetingId: 42 });
    await flushPromises();
    expect(wrapper.find('.up-next').exists()).toBe(false);
    // The recording transition also collapses the sidebar immediately.
    expect(wrapper.find('.add-btn').exists()).toBe(false);
  });

  it('leaves the detail panel unchanged when a recording starts without a meeting', async () => {
    listMeetings.mockResolvedValue([item({ id: '42', title: 'Picked Sync' })]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.find('.up-next').exists()).toBe(false);

    emitEvent('recording://started', { meetingId: null });
    await flushPromises();
    expect(wrapper.find('.up-next').exists()).toBe(false);
    expect(wrapper.find('.add-btn').exists()).toBe(false);
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
    expect(rows[1].text()).toContain('Recording 2026-06-02');
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

  it('start-recording button opens the recorder directly for local backend', async () => {
    usesMeetingPicker.mockReturnValue(false);
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.get('.add-btn').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('start_recording_window', {});
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
});
