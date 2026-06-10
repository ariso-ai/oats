// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

const startRecording = vi.fn();
const stopRecording = vi.fn();
const getAnalyser = vi.fn(() => null);
const waveformStart = vi.fn();
const waveformStop = vi.fn();
const finalizeRecording = vi.fn();
const loadRecordingEnabled = vi.fn();
const closeWin = vi.fn(() => Promise.resolve());
const invoke = vi.fn(() => Promise.resolve());

vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock('@tauri-apps/api/webviewWindow', () => ({
  getCurrentWebviewWindow: () => ({ close: closeWin }),
}));
vi.mock('vue-router', () => ({ useRoute: () => ({ query: {} }) }));
vi.mock('../composables/useRecorder', () => ({
  useRecorder: () => ({
    isRecording: { value: true },
    isPaused: { value: false },
    durationSeconds: { value: 5 },
    startedAt: { value: '2026-06-09T10:00:00Z' },
    getAnalyser,
    startRecording: (...a: unknown[]) => startRecording(...a),
    stopRecording: () => stopRecording(),
    pauseRecording: vi.fn(),
    resumeRecording: vi.fn(),
  }),
}));
vi.mock('../composables/useWaveform', () => ({
  useWaveform: () => ({
    levels: { value: new Array(32).fill(0.5) },
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

import WaveformView from './WaveformView.vue';

beforeEach(() => {
  vi.clearAllMocks();
  loadRecordingEnabled.mockResolvedValue({ mic: true, systemAudio: false });
});
afterEach(() => vi.restoreAllMocks());

describe('WaveformView vertical pill', () => {
  it('starts recording on mount and renders 5 waveform bars + 6 drag dots', async () => {
    const wrapper = mount(WaveformView);
    await flushPromises();
    expect(startRecording).toHaveBeenCalledWith('mic');
    expect(wrapper.findAll('.bar')).toHaveLength(5);
    expect(wrapper.findAll('.dot')).toHaveLength(6);
  });

  it('reveals timer/controls on hover (open class) while keeping the drag handle', async () => {
    const wrapper = mount(WaveformView);
    await flushPromises();
    expect(wrapper.find('.expanded-area').classes()).not.toContain('open');
    expect(wrapper.find('.drag-handle').exists()).toBe(true);

    await wrapper.find('.pill').trigger('mouseenter');
    await flushPromises();
    expect(wrapper.find('.expanded-area').classes()).toContain('open');
    expect(wrapper.find('.timer').text()).toBe('00:05');
    expect(wrapper.find('.stop-btn').exists()).toBe(true);
    expect(wrapper.find('.drag-handle').exists()).toBe(true);
  });

  it('collapses (removes open) on mouseleave', async () => {
    const wrapper = mount(WaveformView);
    await flushPromises();
    await wrapper.find('.pill').trigger('mouseenter');
    await flushPromises();
    expect(wrapper.find('.expanded-area').classes()).toContain('open');

    await wrapper.find('.pill').trigger('mouseleave');
    await flushPromises();
    expect(wrapper.find('.expanded-area').classes()).not.toContain('open');
  });

  it('opens the meetings window when the pill body is clicked', async () => {
    const wrapper = mount(WaveformView);
    await flushPromises();
    await wrapper.find('.pill').trigger('click');
    expect(invoke).toHaveBeenCalledWith('create_library_window');
  });

  it('clicking Stop does not open the meetings window', async () => {
    stopRecording.mockResolvedValue(new Blob([new Uint8Array([1, 2, 3])], { type: 'audio/mpeg' }));
    finalizeRecording.mockResolvedValue({ backend: 'local' });
    const wrapper = mount(WaveformView);
    await flushPromises();
    await wrapper.find('.pill').trigger('mouseenter');
    await flushPromises();
    invoke.mockClear();
    await wrapper.find('.stop-btn').trigger('click');
    await flushPromises();
    expect(invoke).not.toHaveBeenCalledWith('create_library_window');
  });

  it('stops, finalizes, shows ✓, and auto-closes on success', async () => {
    vi.useFakeTimers();
    stopRecording.mockResolvedValue(new Blob([new Uint8Array([1, 2, 3])], { type: 'audio/mpeg' }));
    finalizeRecording.mockResolvedValue({ backend: 'local' });
    const wrapper = mount(WaveformView);
    await vi.runOnlyPendingTimersAsync();
    await wrapper.find('.stop-btn').trigger('click');
    await vi.runOnlyPendingTimersAsync();
    expect(finalizeRecording).toHaveBeenCalledTimes(1);
    expect(wrapper.find('.status-icon.ok').exists()).toBe(true);
    await vi.advanceTimersByTimeAsync(2000);
    expect(closeWin).toHaveBeenCalled();
    vi.useRealTimers();
  });

  it('shows ✗ and stays open on finalize failure', async () => {
    stopRecording.mockResolvedValue(new Blob([new Uint8Array([1, 2, 3])], { type: 'audio/mpeg' }));
    finalizeRecording.mockRejectedValue(new Error('boom'));
    const wrapper = mount(WaveformView);
    await flushPromises();
    await wrapper.find('.stop-btn').trigger('click');
    await flushPromises();
    expect(wrapper.find('.status-icon.err').exists()).toBe(true);
    expect(closeWin).not.toHaveBeenCalled();
  });
});
