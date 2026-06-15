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
const setIgnoreCursorEvents = vi.fn(() => Promise.resolve());
const invoke = vi.fn(() => Promise.resolve());

vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
const eventHandlers: Record<string, (e: unknown) => void> = {};
const emitEvent = vi.fn(() => Promise.resolve());
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, cb: (e: unknown) => void) => {
    eventHandlers[name] = cb;
    return Promise.resolve(() => {});
  }),
  emit: (...a: unknown[]) => emitEvent(...a),
}));
vi.mock('@tauri-apps/api/webviewWindow', () => ({
  getCurrentWebviewWindow: () => ({
    close: closeWin,
    setIgnoreCursorEvents: (...a: unknown[]) => setIgnoreCursorEvents(...a),
  }),
}));
let routeQuery: Record<string, string> = {};
vi.mock('vue-router', () => ({ useRoute: () => ({ query: routeQuery }) }));
const recorderIsRecording = { value: true };
vi.mock('../composables/useRecorder', () => ({
  useRecorder: () => ({
    isRecording: recorderIsRecording,
    isPaused: { value: false },
    durationSeconds: { value: 5 },
    frameLevels: { value: new Array(32).fill(0.5) },
    lastSoundAt: { value: 0 },
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

const listScheduledMeetings = vi.fn(() => Promise.resolve([]));
vi.mock('../composables/useMeetingApi', () => ({
  useMeetingApi: () => ({ listScheduledMeetings: (...a: unknown[]) => listScheduledMeetings(...a) }),
}));

import WaveformView from './WaveformView.vue';

beforeEach(() => {
  vi.clearAllMocks();
  for (const k in eventHandlers) delete eventHandlers[k];
  routeQuery = {};
  recorderIsRecording.value = true;
  loadRecordingEnabled.mockResolvedValue({ mic: true, systemAudio: false });
});
afterEach(() => vi.restoreAllMocks());

describe('WaveformView vertical pill', () => {
  it('starts recording on mount and renders 3 waveform bars + 6 drag dots', async () => {
    const wrapper = mount(WaveformView);
    await flushPromises();
    expect(startRecording).toHaveBeenCalledWith('mic');
    expect(wrapper.findAll('.bar')).toHaveLength(3);
    expect(wrapper.findAll('.dot')).toHaveLength(6);
  });

  it('does not paint the pill when launched with pillHidden=1, but still records', async () => {
    routeQuery = { pillHidden: '1' };
    const wrapper = mount(WaveformView);
    await flushPromises();
    // Recording must start regardless — the window is born visible for getUserMedia.
    expect(startRecording).toHaveBeenCalledWith('mic');
    // Nothing painted: no flash while the meetings window owns the UI.
    expect(wrapper.find('.pill').exists()).toBe(false);
    // The empty transparent window must not swallow clicks underneath it.
    expect(setIgnoreCursorEvents).toHaveBeenCalledWith(true);
  });

  it('paints the pill when a pill-visible event reveals it (library minimized)', async () => {
    routeQuery = { pillHidden: '1' };
    const wrapper = mount(WaveformView);
    await flushPromises();
    expect(wrapper.find('.pill').exists()).toBe(false);

    await eventHandlers['recorder://pill-visible']?.({ payload: true });
    await flushPromises();
    expect(wrapper.find('.pill').exists()).toBe(true);
    expect(setIgnoreCursorEvents).toHaveBeenLastCalledWith(false);
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
    vi.setSystemTime(0); // keep Date.now() at epoch so silence backstop (lastSoundAt=0) never trips
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

  it('auto-stops after the silence timeout elapses', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(16 * 60_000); // now well past lastSoundAt (0) + 15min
    finalizeRecording.mockResolvedValue({ backend: 'local' });
    const wrapper = mount(WaveformView);
    await flushPromises();
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    await vi.advanceTimersByTimeAsync(1_100);
    await flushPromises();
    expect(stopRecording).toHaveBeenCalled();
    vi.useRealTimers();
    wrapper.unmount();
  });

  it('does not broadcast a recording phase before capture has started', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    // getUserMedia still pending: the recorder reports not-recording.
    recorderIsRecording.value = false;
    mount(WaveformView);
    await vi.runOnlyPendingTimersAsync();
    emitEvent.mockClear();
    await vi.advanceTimersByTimeAsync(2_100); // heartbeats fire
    const states = emitEvent.mock.calls.filter(([name]) => name === 'recorder://state');
    expect(states).toHaveLength(0);
    vi.useRealTimers();
  });

  it('broadcasts recorder://state through the stop flow, ending with closed', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    routeQuery = { meetingId: '42' };
    stopRecording.mockResolvedValue(new Blob([new Uint8Array([1, 2, 3])], { type: 'audio/mpeg' }));
    finalizeRecording.mockResolvedValue({ backend: 'local' });
    const wrapper = mount(WaveformView);
    await vi.runOnlyPendingTimersAsync();
    await wrapper.find('.stop-btn').trigger('click');
    await vi.runOnlyPendingTimersAsync();

    const states = emitEvent.mock.calls
      .filter(([name]) => name === 'recorder://state')
      .map(([, payload]) => payload as { phase: string; meetingId: number | null });
    const phases = states.map((s) => s.phase);
    expect(phases).toContain('uploading');
    expect(phases).toContain('success');
    expect(states[0]?.meetingId).toBe(42);

    await vi.advanceTimersByTimeAsync(2000);
    const last = emitEvent.mock.calls.filter(([name]) => name === 'recorder://state').at(-1);
    expect((last?.[1] as { phase: string }).phase).toBe('closed');
    vi.useRealTimers();
  });

  it('broadcasts the deterministic local recording id for the local backend', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    mount(WaveformView);
    await vi.runOnlyPendingTimersAsync();
    await vi.advanceTimersByTimeAsync(1_100); // heartbeat

    const state = emitEvent.mock.calls
      .filter(([name]) => name === 'recorder://state')
      .map(([, payload]) => payload as { localRecordingId: string | null })
      .at(-1);
    // Mirrors Rust sanitize_iso_to_id over the mocked startedAt.
    expect(state?.localRecordingId).toBe('2026-06-09T10-00-00Z');
    vi.useRealTimers();
  });

  it('auto mode with no calendar match shows the confirm overlay', async () => {
    routeQuery = { auto: '1' };
    listScheduledMeetings.mockResolvedValue([]);
    const wrapper = mount(WaveformView);
    await flushPromises();
    expect(wrapper.find('.confirm').exists()).toBe(true);
    expect(wrapper.find('.keep-btn').exists()).toBe(true);
    routeQuery = {};
    wrapper.unmount();
  });

  it('discards (does not upload) when stopped while the confirm overlay is unanswered', async () => {
    routeQuery = { auto: '1' };
    listScheduledMeetings.mockResolvedValue([]);
    const wrapper = mount(WaveformView);
    await flushPromises();
    // Confirm overlay should be showing (local backend, no match).
    expect(wrapper.find('.confirm').exists()).toBe(true);
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    // A native mic-off stop arrives before the user answers.
    await eventHandlers['auto-record://stop']?.({});
    await flushPromises();
    // Must NOT have uploaded; must have stopped + closed.
    expect(finalizeRecording).not.toHaveBeenCalled();
    expect(stopRecording).toHaveBeenCalled();
    routeQuery = {};
    wrapper.unmount();
  });
});
