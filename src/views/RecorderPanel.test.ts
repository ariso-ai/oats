// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

const startRecording = vi.fn();
const stopRecording = vi.fn();
const pauseRecording = vi.fn();
const resumeRecording = vi.fn();
const getAnalyser = vi.fn(() => null);
const waveformStart = vi.fn();
const waveformStop = vi.fn();
const finalizeRecording = vi.fn();
const loadRecordingEnabled = vi.fn();

vi.mock('../composables/useRecorder', () => ({
  useRecorder: () => ({
    isPaused: { value: false },
    durationSeconds: { value: 0 },
    startedAt: { value: '2026-06-09T10:00:00Z' },
    getAnalyser,
    startRecording: (...a: unknown[]) => startRecording(...a),
    stopRecording: () => stopRecording(),
    pauseRecording: () => pauseRecording(),
    resumeRecording: () => resumeRecording(),
  }),
}));
vi.mock('../composables/useWaveform', () => ({
  useWaveform: () => ({
    levels: { value: new Array(32).fill(0) },
    start: (...a: unknown[]) => waveformStart(...a),
    stop: () => waveformStop(),
  }),
}));
vi.mock('../composables/useBackend', () => ({
  getActiveBackend: () =>
    Promise.resolve({ id: 'local', finalizeRecording: (...a: unknown[]) => finalizeRecording(...a) }),
}));
vi.mock('../composables/useRecordingPermissions', () => ({
  loadRecordingEnabled: () => loadRecordingEnabled(),
}));

import RecorderPanel from './RecorderPanel.vue';

beforeEach(() => {
  vi.clearAllMocks();
  loadRecordingEnabled.mockResolvedValue({ mic: true, systemAudio: false });
});
afterEach(() => vi.restoreAllMocks());

describe('RecorderPanel', () => {
  it('starts recording on mount with the derived mode', async () => {
    const wrapper = mount(RecorderPanel);
    await flushPromises();
    expect(startRecording).toHaveBeenCalledWith('mic');
  });

  it('emits done immediately when no recording source is enabled', async () => {
    loadRecordingEnabled.mockResolvedValue({ mic: false, systemAudio: false });
    const wrapper = mount(RecorderPanel);
    await flushPromises();
    expect(startRecording).not.toHaveBeenCalled();
    expect(wrapper.emitted('done')).toHaveLength(1);
  });

  it('stops, finalizes, shows success, and emits done on close', async () => {
    stopRecording.mockResolvedValue(new Blob([new Uint8Array([1, 2, 3])], { type: 'audio/mpeg' }));
    finalizeRecording.mockResolvedValue({ backend: 'local' });
    const wrapper = mount(RecorderPanel);
    await flushPromises();
    await wrapper.find('.stop-btn').trigger('click');
    await flushPromises();
    expect(stopRecording).toHaveBeenCalled();
    expect(finalizeRecording).toHaveBeenCalledTimes(1);
    expect(wrapper.text()).toContain('Transcription complete');
    await wrapper.find('.close-btn').trigger('click');
    expect(wrapper.emitted('done')).toHaveLength(1);
  });
});
