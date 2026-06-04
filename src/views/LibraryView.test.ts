// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

const listRecordings = vi.fn();
const readRecordingAudio = vi.fn();
const openRecordingFile = vi.fn();
vi.mock('../tauri', () => ({
  local: {
    listRecordings: () => listRecordings(),
    readRecordingAudio: (id: string) => readRecordingAudio(id),
    openRecordingFile: (id: string, kind: string) => openRecordingFile(id, kind),
  },
}));

import LibraryView from './LibraryView.vue';

function rec(over: Record<string, unknown>) {
  return {
    id: 'x',
    title: 'T',
    createdAt: '2026-06-02T10:00:00Z',
    durationSeconds: 10,
    status: 'done',
    hasAudio: false,
    hasNote: false,
    hasTranscript: false,
    ...over,
  };
}

beforeEach(() => vi.clearAllMocks());

describe('LibraryView', () => {
  it('shows an empty state when there are no recordings', async () => {
    listRecordings.mockResolvedValue([]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.text()).toContain('No recordings yet');
    expect(wrapper.findAll('.recording-row')).toHaveLength(0);
  });

  it('renders a row per recording in the order returned', async () => {
    listRecordings.mockResolvedValue([
      { id: 'b', title: 'Second', createdAt: '2026-06-02T10:00:00Z', durationSeconds: 75, status: 'done' },
      { id: 'a', title: 'First', createdAt: '2026-06-01T10:00:00Z', durationSeconds: 3661, status: 'failed' },
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    const rows = wrapper.findAll('.recording-row');
    expect(rows).toHaveLength(2);
    expect(rows[0].text()).toContain('Second');
    expect(rows[0].text()).toContain('01:15'); // 75s
    expect(rows[1].text()).toContain('First');
    expect(rows[1].text()).toContain('61:01'); // 3661s
    expect(rows[1].text()).toContain('failed');
  });

  it('enables/disables Note and Transcript per file-presence flags', async () => {
    listRecordings.mockResolvedValue([
      rec({ id: 'a', hasNote: true, hasTranscript: false }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    const note = wrapper.find('.btn-note');
    const transcript = wrapper.find('.btn-transcript');
    expect((note.element as HTMLButtonElement).disabled).toBe(false);
    expect((transcript.element as HTMLButtonElement).disabled).toBe(true);
  });

  it('clicking an enabled Note button opens the note file', async () => {
    openRecordingFile.mockResolvedValue(undefined);
    listRecordings.mockResolvedValue([
      rec({ id: 'a', hasNote: true }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.find('.btn-note').trigger('click');
    expect(openRecordingFile).toHaveBeenCalledWith('a', 'note');
  });

  it('clicking an enabled Transcript button opens the transcript file', async () => {
    openRecordingFile.mockResolvedValue(undefined);
    listRecordings.mockResolvedValue([
      rec({ id: 'a', hasTranscript: true }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    await wrapper.find('.btn-transcript').trigger('click');
    expect(openRecordingFile).toHaveBeenCalledWith('a', 'transcript');
  });

  it('shows a disabled play control when hasAudio is false', async () => {
    listRecordings.mockResolvedValue([rec({ id: 'a', hasAudio: false })]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    const play = wrapper.find('.play-btn');
    expect((play.element as HTMLButtonElement).disabled).toBe(true);
    expect(play.text()).toContain('No audio');
  });

  it('shows an enabled play trigger and loads audio on click when hasAudio is true', async () => {
    vi.stubGlobal('URL', {
      createObjectURL: vi.fn(() => 'blob:x'),
      revokeObjectURL: vi.fn(),
    });
    readRecordingAudio.mockResolvedValue(new ArrayBuffer(8));
    listRecordings.mockResolvedValue([rec({ id: 'a', hasAudio: true })]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    const play = wrapper.find('.play-btn');
    expect((play.element as HTMLButtonElement).disabled).toBe(false);
    // Lazy: nothing fetched on mount.
    expect(readRecordingAudio).not.toHaveBeenCalled();
    await play.trigger('click');
    await flushPromises();
    expect(readRecordingAudio).toHaveBeenCalledWith('a');
    expect(wrapper.find('audio.audio-el').exists()).toBe(true);
    vi.unstubAllGlobals();
  });
});
