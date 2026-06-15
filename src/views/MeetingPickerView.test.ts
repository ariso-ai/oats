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
  it('renders the renamed button and hides the title prompt by default', async () => {
    const wrapper = mount(MeetingPickerView);
    await flushPromises();
    expect(byText(wrapper, 'Record a new meeting')).toBeTruthy();
    expect(byText(wrapper, 'Record without meeting')).toBeFalsy();
    expect(wrapper.find('input').exists()).toBe(false);
  });

  it('prompts for a title, creates the meeting, then opens the recorder for it', async () => {
    const wrapper = mount(MeetingPickerView);
    await flushPromises();

    await byText(wrapper, 'Record a new meeting')!.trigger('click');
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

    await byText(wrapper, 'Record a new meeting')!.trigger('click');
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

    await byText(wrapper, 'Record a new meeting')!.trigger('click');
    await flushPromises();
    await byText(wrapper, 'Start recording')!.trigger('click');
    await flushPromises();

    expect(invoke).not.toHaveBeenCalledWith('start_recording_window', { meetingId: 77 });
    expect(wrapper.text()).toContain('boom');
  });
});
