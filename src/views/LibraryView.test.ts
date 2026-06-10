// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';

const listMeetings = vi.fn();
const openRecordingFile = vi.fn();
const readRecordingAudio = vi.fn();
const invoke = vi.fn(() => Promise.resolve());
const getAllWebviewWindows = vi.fn(() => Promise.resolve([] as { label: string }[]));

vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock('@tauri-apps/api/webviewWindow', () => ({
  getAllWebviewWindows: () => getAllWebviewWindows(),
}));
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
  getAllWebviewWindows.mockResolvedValue([]);
  invoke.mockResolvedValue(undefined);
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
    expect(wrapper.findAll('.recording-row')).toHaveLength(0);
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

  it('clicking a meeting item selects it (aria-pressed becomes true)', async () => {
    listMeetings.mockResolvedValue([
      item({ id: 'a', title: 'Standup' }),
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    const btn = wrapper.find('.meeting-item');
    expect(btn.attributes('aria-pressed')).toBe('false');
    await btn.trigger('click');
    expect(btn.attributes('aria-pressed')).toBe('true');
    expect(btn.classes()).toContain('selected');
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

  it('omits file controls for items without files (ariso meetings)', async () => {
    listMeetings.mockResolvedValue([
      { id: '7', title: 'Standup', timestamp: '2026-06-08T09:00:00Z' },
    ]);
    const wrapper = mount(LibraryView);
    await flushPromises();
    expect(wrapper.find('.row-controls').exists()).toBe(false);
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
});
