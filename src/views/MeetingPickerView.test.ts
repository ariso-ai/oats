// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';

const invoke = vi.fn(() => Promise.resolve());
const listScheduledMeetings = vi.fn(() => Promise.resolve([] as unknown[]));
const createAudioMeeting = vi.fn(() => Promise.resolve({ meetingId: 77 }));

vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock('../composables/useMeetingApi', () => ({
  useMeetingApi: () => ({
    listScheduledMeetings: (...a: unknown[]) => listScheduledMeetings(...a),
    createAudioMeeting: (...a: unknown[]) => createAudioMeeting(...a),
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
