// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';

const invoke = vi.fn(() => Promise.resolve());
const listScheduledMeetings = vi.fn(() => Promise.resolve([] as unknown[]));
const createAudioMeeting = vi.fn(() => Promise.resolve({ meetingId: 77 }));
const getMeetingNotes = vi.fn(() => Promise.resolve({ id: 5, title: 'Past Sync', start_at: '2026-06-01T09:00:00Z' }));

vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock('../composables/useMeetingApi', () => ({
  useMeetingApi: () => ({
    listScheduledMeetings: (...a: unknown[]) => listScheduledMeetings(...a),
    createAudioMeeting: (...a: unknown[]) => createAudioMeeting(...a),
    getMeetingNotes: (...a: unknown[]) => getMeetingNotes(...a),
  }),
}));

import MeetingPickerView from './MeetingPickerView.vue';

function byText(wrapper: ReturnType<typeof mount>, text: string) {
  return wrapper.findAll('button').find((b) => b.text() === text);
}

enableAutoUnmount(afterEach);
beforeEach(() => {
  vi.clearAllMocks();
  listScheduledMeetings.mockResolvedValue([]);
  createAudioMeeting.mockResolvedValue({ meetingId: 77 });
});

describe('MeetingPickerView — record a new meeting', () => {
  it('opens the new-meeting prompt directly when there are no meetings today', async () => {
    // listScheduledMeetings defaults to [] in beforeEach.
    const wrapper = mount(MeetingPickerView);
    await flushPromises();
    expect(wrapper.text()).toContain('New meeting');
    expect(wrapper.text()).not.toContain('No meetings today.');
    expect(byText(wrapper, 'Record a new meeting')).toBeFalsy();
    expect(wrapper.find('input').exists()).toBe(true);
  });

  it('creates the meeting from the prompt and opens the recorder for it', async () => {
    const wrapper = mount(MeetingPickerView);
    await flushPromises();
    const input = wrapper.find('input');
    expect(input.exists()).toBe(true);
    await input.setValue('Sync with Sam');
    await byText(wrapper, 'Start recording')!.trigger('click');
    await flushPromises();
    expect(createAudioMeeting).toHaveBeenCalledWith('Sync with Sam');
    expect(invoke).toHaveBeenCalledWith('start_recording_window', { meetingId: 77 });
  });

  it('shows immediate feedback while the recorder window is opening', async () => {
    let resolveStart!: () => void;
    invoke.mockImplementationOnce(
      () => new Promise<void>((resolve) => { resolveStart = resolve; }),
    );
    const wrapper = mount(MeetingPickerView);
    await flushPromises();

    void byText(wrapper, 'Start recording')!.trigger('click');
    await flushPromises();

    expect(wrapper.text()).toContain('Starting recording');
    expect(wrapper.find('.btn-primary').attributes('disabled')).toBeDefined();
    resolveStart();
    await flushPromises();
  });

  it('keeps the title optional — an empty title still starts recording', async () => {
    const wrapper = mount(MeetingPickerView);
    await flushPromises();
    await byText(wrapper, 'Start recording')!.trigger('click');
    await flushPromises();
    expect(createAudioMeeting).toHaveBeenCalledWith('');
    expect(invoke).toHaveBeenCalledWith('start_recording_window', { meetingId: 77 });
  });

  it('surfaces an error and does not open the recorder when creation fails', async () => {
    createAudioMeeting.mockRejectedValueOnce(new Error('boom'));
    const wrapper = mount(MeetingPickerView);
    await flushPromises();
    await byText(wrapper, 'Start recording')!.trigger('click');
    await flushPromises();
    expect(invoke).not.toHaveBeenCalledWith('start_recording_window', { meetingId: 77 });
    expect(wrapper.text()).toContain('boom');
  });

  it('shows the meeting list with the new-meeting button when meetings exist', async () => {
    listScheduledMeetings.mockResolvedValue([
      { id: 1, title: 'Sync', start_at: new Date().toISOString() },
    ]);
    const wrapper = mount(MeetingPickerView);
    await flushPromises();
    expect(wrapper.text()).toContain('Select a meeting');
    expect(byText(wrapper, 'Record a new meeting')).toBeTruthy();
    expect(wrapper.find('input').exists()).toBe(false);
    // The button still opens the title prompt on demand.
    await byText(wrapper, 'Record a new meeting')!.trigger('click');
    await flushPromises();
    expect(wrapper.find('input').exists()).toBe(true);
  });

  it('ignores Escape in the empty state so the prompt cannot be dismissed into a dead-end', async () => {
    const wrapper = mount(MeetingPickerView);
    await flushPromises();
    expect(wrapper.find('input').exists()).toBe(true);
    await wrapper.find('input').trigger('keydown', { key: 'Escape' });
    await flushPromises();
    // Still showing the title input — Escape did not collapse it.
    expect(wrapper.find('input').exists()).toBe(true);
  });
});

describe('MeetingPickerView — the recommended meeting is the primary target', () => {
  it('features the happening-now meeting and keeps it primary after expanding', async () => {
    const now = Date.now();
    listScheduledMeetings.mockResolvedValue([
      { id: 1, title: 'Standup', start_at: new Date(now).toISOString() },
      { id: 2, title: 'Later Thing', start_at: new Date(now + 3 * 60 * 60_000).toISOString() },
    ]);
    const wrapper = mount(MeetingPickerView);
    await flushPromises();

    // Collapsed: just the pick, carrying the primary treatment.
    let rows = wrapper.findAll('.meeting-row');
    expect(rows).toHaveLength(1);
    expect(rows[0].classes()).toContain('meeting-row--featured');
    expect(wrapper.text()).toContain('Happening now');

    await byText(wrapper, 'View all ▾')!.trigger('click');
    await flushPromises();

    // Expanded: still primary, and the other rows stay quiet.
    rows = wrapper.findAll('.meeting-row');
    expect(rows).toHaveLength(2);
    const standup = rows.find((r) => r.text().includes('Standup'))!;
    const later = rows.find((r) => r.text().includes('Later Thing'))!;
    expect(standup.classes()).toContain('meeting-row--featured');
    expect(standup.find('.live-dot').exists()).toBe(true);
    expect(later.classes()).not.toContain('meeting-row--featured');
    expect(later.find('.meeting-badge').exists()).toBe(false);
  });

  it('badges the soonest meeting "Up next" when nothing is happening now', async () => {
    const now = Date.now();
    listScheduledMeetings.mockResolvedValue([
      { id: 1, title: 'Later Thing', start_at: new Date(now + 3 * 60 * 60_000).toISOString() },
      { id: 2, title: 'Soon Thing', start_at: new Date(now + 30 * 60_000).toISOString() },
    ]);
    const wrapper = mount(MeetingPickerView);
    await flushPromises();

    await byText(wrapper, 'View all ▾')!.trigger('click');
    await flushPromises();

    const rows = wrapper.findAll('.meeting-row');
    const soon = rows.find((r) => r.text().includes('Soon Thing'))!;
    expect(soon.classes()).toContain('meeting-row--featured');
    expect(soon.find('.meeting-badge').text()).toContain('Up next');
    // "Up next" is not live, so no pulsing dot.
    expect(soon.find('.live-dot').exists()).toBe(false);
  });

  it('does not fade an edge the list has not scrolled past', async () => {
    listScheduledMeetings.mockResolvedValue([
      { id: 1, title: 'Standup', start_at: new Date().toISOString() },
    ]);
    const wrapper = mount(MeetingPickerView);
    await flushPromises();

    await byText(wrapper, 'View all ▾')!.trigger('click');
    await flushPromises();

    // An unconditional top fade dissolved the first meeting's title into the
    // backdrop before the user scrolled at all.
    const list = wrapper.find('.meeting-list');
    expect(list.classes()).not.toContain('is-faded-top');
    expect(list.classes()).not.toContain('is-faded-bottom');
  });
});

describe('MeetingPickerView — Ari-join gate', () => {
  it('flagged meeting shows confirm dialog and only records on "Record anyway"', async () => {
    listScheduledMeetings.mockResolvedValue([
      {
        id: 42,
        title: 'Standup',
        start_at: new Date().toISOString(),
        auto_join_scheduled: true,
      },
    ]);
    const wrapper = mount(MeetingPickerView);
    await flushPromises();

    // Expand to see all meetings and click the flagged one.
    await byText(wrapper, 'View all ▾')!.trigger('click');
    await flushPromises();

    const meetingBtn = wrapper.findAll('.meeting-row').find((b) => b.text().includes('Standup'));
    expect(meetingBtn).toBeTruthy();
    await meetingBtn!.trigger('click');
    await flushPromises();

    // Dialog must be visible.
    expect(wrapper.text()).toContain('Ari is scheduled to join this meeting and take notes');
    // Recording must NOT have started yet.
    expect(invoke).not.toHaveBeenCalledWith('start_recording_window', { meetingId: 42 });

    // Click "Record anyway" → recording proceeds.
    const buttons = wrapper.findAll('.ari-confirm__actions button');
    const recordBtn = buttons.find((b) => b.text() === 'Record anyway');
    expect(recordBtn).toBeTruthy();
    await recordBtn!.trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('start_recording_window', { meetingId: 42 });
  });

  it('unflagged meeting records immediately with no dialog', async () => {
    listScheduledMeetings.mockResolvedValue([
      {
        id: 7,
        title: 'Planning',
        start_at: new Date().toISOString(),
        auto_join_scheduled: false,
      },
    ]);
    const wrapper = mount(MeetingPickerView);
    await flushPromises();

    await byText(wrapper, 'View all ▾')!.trigger('click');
    await flushPromises();

    const meetingBtn = wrapper.findAll('.meeting-row').find((b) => b.text().includes('Planning'));
    expect(meetingBtn).toBeTruthy();
    await meetingBtn!.trigger('click');
    await flushPromises();

    expect(wrapper.text()).not.toContain('Ari is scheduled to join this meeting');
    expect(invoke).toHaveBeenCalledWith('start_recording_window', { meetingId: 7 });
  });
});

describe('MeetingPickerView — forced default from the Library', () => {
  it('features the default meeting from the hash and records it on click', async () => {
    listScheduledMeetings.mockResolvedValue([]);
    getMeetingNotes.mockResolvedValue({ id: 5, title: 'Past Sync', start_at: '2026-06-01T09:00:00Z' });
    const prevHash = window.location.hash;
    window.location.hash = '#/meeting-picker?defaultMeetingId=5';
    try {
      const wrapper = mount(MeetingPickerView);
      await flushPromises();
      expect(getMeetingNotes).toHaveBeenCalledWith(5);
      expect(wrapper.text()).toContain('Continue meeting');
      expect(wrapper.text()).toContain('Past Sync');
      await wrapper.find('.meeting-row').trigger('click');
      await flushPromises();
      expect(invoke).toHaveBeenCalledWith('start_recording_window', { meetingId: 5 });
    } finally {
      window.location.hash = prevHash;
    }
  });

  it('"View all" shows the full list instead of a blank screen, and "View less" returns to Continue meeting', async () => {
    listScheduledMeetings.mockResolvedValue([
      { id: 9, title: 'Other Meeting', start_at: new Date().toISOString() },
    ]);
    getMeetingNotes.mockResolvedValue({ id: 5, title: 'Past Sync', start_at: '2026-06-01T09:00:00Z' });
    const prevHash = window.location.hash;
    window.location.hash = '#/meeting-picker?defaultMeetingId=5';
    try {
      const wrapper = mount(MeetingPickerView);
      await flushPromises();
      expect(wrapper.text()).toContain('Continue meeting');

      await byText(wrapper, 'View all ▾')!.trigger('click');
      await flushPromises();
      expect(wrapper.find('.meeting-list').exists()).toBe(true);
      expect(wrapper.text()).toContain('Other Meeting');
      expect(wrapper.text()).not.toContain('Continue meeting');

      await byText(wrapper, 'View less ▴')!.trigger('click');
      await flushPromises();
      expect(wrapper.text()).toContain('Continue meeting');
      expect(wrapper.text()).toContain('Past Sync');
      expect(wrapper.find('.meeting-list').exists()).toBe(false);
    } finally {
      window.location.hash = prevHash;
    }
  });

  it('"View all" keeps the forced meeting when it is a past meeting outside today\'s list', async () => {
    listScheduledMeetings.mockResolvedValue([
      { id: 9, title: 'Other Meeting', start_at: new Date().toISOString() },
    ]);
    getMeetingNotes.mockResolvedValue({ id: 5, title: 'Past Sync', start_at: '2026-06-01T09:00:00Z' });
    const prevHash = window.location.hash;
    window.location.hash = '#/meeting-picker?defaultMeetingId=5';
    try {
      const wrapper = mount(MeetingPickerView);
      await flushPromises();

      await byText(wrapper, 'View all ▾')!.trigger('click');
      await flushPromises();

      // The forced "Continue" meeting (id 5) isn't in today's list, but expanding
      // must not drop it — it should still be choosable, just as a regular row.
      expect(wrapper.text()).toContain('Past Sync');
      expect(wrapper.text()).toContain('Other Meeting');
      const pastSyncRow = wrapper.findAll('.meeting-row').find((r) => r.text().includes('Past Sync'));
      expect(pastSyncRow).toBeTruthy();
      await pastSyncRow!.trigger('click');
      await flushPromises();
      expect(invoke).toHaveBeenCalledWith('start_recording_window', { meetingId: 5 });
    } finally {
      window.location.hash = prevHash;
    }
  });
});
