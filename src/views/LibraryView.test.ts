// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';

const listMeetings = vi.fn();
const getMeetingDetail = vi.fn();
const backendId = vi.fn(() => 'local');
const usesMeetingPicker = vi.fn(() => false);
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
        listMeetings: () => listMeetings(),
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

  it('starts recording against the selected scheduled meeting id', async () => {
    backendId.mockReturnValue('ariso');
    usesMeetingPicker.mockReturnValue(true);
    listMeetings.mockResolvedValue([
      item({ id: '42', title: 'Daily Plan', files: undefined }),
      item({ id: '7', title: 'Other Sync', files: undefined }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();

    await wrapper.get('.add-btn').trigger('click');
    await flushPromises();

    expect(invoke).toHaveBeenCalledWith('start_recording_window', { meetingId: 42 });
    expect(invoke).not.toHaveBeenCalledWith('open_meeting_picker', {});
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
    expect(wrapper.find('.empty-card').exists()).toBe(false);

    emitEvent('recording://started', { meetingId: 42 });
    await flushPromises();
    expect(wrapper.find('.empty-card').exists()).toBe(false);
    // The recording transition also collapses the sidebar immediately.
    expect(wrapper.find('.add-btn').exists()).toBe(false);
  });

  it('leaves the detail panel unchanged when a recording starts without a meeting', async () => {
    listMeetings.mockResolvedValue([item({ id: '42', title: 'Picked Sync' })]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.find('.empty-card').exists()).toBe(false);

    emitEvent('recording://started', { meetingId: null });
    await flushPromises();
    expect(wrapper.find('.empty-card').exists()).toBe(false);
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
    expect(wrapper.find('.empty-card').exists()).toBe(false);
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
    // Synthetic row (14:30) sorts above Standup (10:00) on the same date.
    expect(rows[0].text()).toContain('Recording 2026-06-02');
    expect(rows[0].find('.mi-rec-dot').exists()).toBe(true);
    expect(rows[0].attributes('aria-pressed')).toBe('true');
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

    const rows = wrapper.findAll('.meeting-item');
    await rows[1].trigger('click');
    await flushPromises();

    expect(rows[1].attributes('aria-pressed')).toBe('true');
    expect(wrapper.find('.strip').exists()).toBe(false);
    expect(rows[0].find('.mi-rec-dot').exists()).toBe(true);
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
    // The finalized recording keeps the selection under the same id.
    expect(rows[0].attributes('aria-pressed')).toBe('true');
    expect(rows[0].find('.mi-rec-dot').exists()).toBe(false);
  });

  it('falls back to the first meeting when a discarded recording leaves no row', async () => {
    listMeetings.mockResolvedValue([item({ id: 'a', title: 'Standup' })]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    emitEvent('recorder://state', localRecorderState());
    await flushPromises();
    expect(wrapper.findAll('.meeting-item')[0].attributes('aria-pressed')).toBe('true');

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
