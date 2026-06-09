// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

const listMeetings = vi.fn();
const openRecordingFile = vi.fn();
const readRecordingAudio = vi.fn();

vi.mock('../composables/useBackend', () => ({
  getActiveBackend: () => Promise.resolve({ id: 'local', listMeetings: () => listMeetings() }),
}));
// RecordingAudioPlayer (rendered for local rows) and openNote/openTranscript go
// through ../tauri; keep those mocked so jsdom never touches real IPC.
vi.mock('../tauri', () => ({
  local: {
    openRecordingFile: (id: string, kind: string) => openRecordingFile(id, kind),
    readRecordingAudio: (id: string) => readRecordingAudio(id),
  },
}));
// Stub the recorder so mounting it never starts a real AudioContext.
vi.mock('./RecorderPanel.vue', () => ({
  default: {
    name: 'RecorderPanel',
    emits: ['done'],
    template: '<div class="recorder-stub"><button class="stub-done" @click="$emit(\'done\')">done</button></div>',
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

beforeEach(() => vi.clearAllMocks());
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
    expect(wrapper.findAll('.recording-row')).toHaveLength(0);
  });

  it('renders a row per meeting in the order returned', async () => {
    listMeetings.mockResolvedValue([
      item({ id: 'b', title: 'Second', durationSeconds: 75, status: 'done' }),
      item({ id: 'a', title: 'First', durationSeconds: 3661, status: 'failed' }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    const rows = wrapper.findAll('.recording-row');
    expect(rows).toHaveLength(2);
    expect(rows[0].text()).toContain('Second');
    expect(rows[0].text()).toContain('01:15');
    expect(rows[1].text()).toContain('First');
    expect(rows[1].text()).toContain('61:01');
    expect(rows[1].text()).toContain('failed');
  });

  it('enables/disables Note and Transcript per file-presence flags', async () => {
    listMeetings.mockResolvedValue([
      item({ id: 'a', files: { hasAudio: false, hasNote: true, hasTranscript: false } }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect((wrapper.find('.btn-note').element as HTMLButtonElement).disabled).toBe(false);
    expect((wrapper.find('.btn-transcript').element as HTMLButtonElement).disabled).toBe(true);
  });

  it('clicking an enabled Note button opens the note file', async () => {
    openRecordingFile.mockResolvedValue(undefined);
    listMeetings.mockResolvedValue([
      item({ id: 'a', files: { hasAudio: false, hasNote: true, hasTranscript: false } }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.find('.btn-note').trigger('click');
    expect(openRecordingFile).toHaveBeenCalledWith('a', 'note');
  });

  it('omits file controls for items without files (ariso meetings)', async () => {
    listMeetings.mockResolvedValue([
      { id: '7', title: 'Standup', timestamp: '2026-06-08T09:00:00Z' },
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.find('.row-controls').exists()).toBe(false);
  });

  it('hides the recorder until Record is clicked, then hides + reloads on done', async () => {
    listMeetings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.find('.recorder-stub').exists()).toBe(false);
    expect(listMeetings).toHaveBeenCalledTimes(1);

    await wrapper.find('.record-btn').trigger('click');
    expect(wrapper.find('.recorder-stub').exists()).toBe(true);

    await wrapper.find('.stub-done').trigger('click');
    await flushPromises();
    expect(wrapper.find('.recorder-stub').exists()).toBe(false);
    expect(listMeetings).toHaveBeenCalledTimes(2); // reloaded after recording
  });
});
