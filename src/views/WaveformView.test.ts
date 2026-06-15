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
  getCurrentWebviewWindow: () => ({ close: closeWin }),
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

const discardPendingAudio = vi.fn(() => Promise.resolve());
vi.mock('../tauri', () => ({
  pending: { discardAudio: (...a: unknown[]) => discardPendingAudio(...a) },
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

  it('never broadcasts a success phase when finalize fails', async () => {
    stopRecording.mockResolvedValue(new Blob([new Uint8Array([1, 2, 3])], { type: 'audio/mpeg' }));
    finalizeRecording.mockRejectedValue(new Error('offline'));
    const wrapper = mount(WaveformView);
    await flushPromises();
    await wrapper.find('.stop-btn').trigger('click');
    await flushPromises();

    const phases = emitEvent.mock.calls
      .filter(([name]) => name === 'recorder://state')
      .map(([, payload]) => (payload as { phase: string }).phase);
    expect(phases).toContain('failed');
    expect(phases).not.toContain('success');
    // And the pill shows the error state, not a "saved" ✓.
    expect(wrapper.find('.status-icon.ok').exists()).toBe(false);
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

  it('failed upload shows Retry, Resume, and Discard controls', async () => {
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    finalizeRecording.mockRejectedValue(new Error('boom'));
    const wrapper = mount(WaveformView);
    await flushPromises();
    await wrapper.find('.stop-btn').trigger('click');
    await flushPromises();
    expect(wrapper.find('.status-icon.err').exists()).toBe(true);
    expect(wrapper.find('.retry-btn').exists()).toBe(true);
    expect(wrapper.find('.resume-btn').exists()).toBe(true);
    expect(wrapper.find('.dismiss-btn').exists()).toBe(true);
  });

  it('Resume clears the failed state, restarts recording, and keeps the blob', async () => {
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    finalizeRecording.mockRejectedValue(new Error('boom'));
    const wrapper = mount(WaveformView);
    await flushPromises();
    await wrapper.find('.stop-btn').trigger('click');
    await flushPromises();
    expect(wrapper.find('.status-icon.err').exists()).toBe(true);

    startRecording.mockClear();
    await wrapper.find('.resume-btn').trigger('click');
    await flushPromises();

    // Back in the live recording view, mic restarted, nothing discarded/closed.
    expect(startRecording).toHaveBeenCalledTimes(1);
    expect(wrapper.find('.status-icon.err').exists()).toBe(false);
    expect(wrapper.find('.bars').exists()).toBe(true);
    expect(discardPendingAudio).not.toHaveBeenCalled();
    expect(closeWin).not.toHaveBeenCalled();
  });

  it('Retry re-runs finalize with the same blob and meta, then succeeds', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    finalizeRecording
      .mockRejectedValueOnce(new Error('boom'))
      .mockResolvedValue({ backend: 'local' });
    const wrapper = mount(WaveformView);
    await vi.runOnlyPendingTimersAsync();
    await wrapper.find('.stop-btn').trigger('click');
    await vi.runOnlyPendingTimersAsync();
    expect(wrapper.find('.retry-btn').exists()).toBe(true);

    await wrapper.find('.retry-btn').trigger('click');
    await vi.runOnlyPendingTimersAsync();

    expect(finalizeRecording).toHaveBeenCalledTimes(2);
    // Same blob and meta on both attempts — retry must not re-derive anything.
    expect(finalizeRecording.mock.calls[1][0]).toBe(finalizeRecording.mock.calls[0][0]);
    expect(finalizeRecording.mock.calls[1][1]).toEqual(finalizeRecording.mock.calls[0][1]);
    expect(wrapper.find('.status-icon.ok').exists()).toBe(true);
    await vi.advanceTimersByTimeAsync(2000);
    expect(closeWin).toHaveBeenCalled();
    vi.useRealTimers();
  });

  it('retry broadcasts uploading then success phases', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    finalizeRecording
      .mockRejectedValueOnce(new Error('boom'))
      .mockResolvedValue({ backend: 'local' });
    const wrapper = mount(WaveformView);
    await vi.runOnlyPendingTimersAsync();
    await wrapper.find('.stop-btn').trigger('click');
    await vi.runOnlyPendingTimersAsync();
    emitEvent.mockClear();

    await wrapper.find('.retry-btn').trigger('click');
    await vi.runOnlyPendingTimersAsync();

    const phases = emitEvent.mock.calls
      .filter(([name]) => name === 'recorder://state')
      .map(([, p]) => (p as { phase: string }).phase);
    expect(phases).toContain('uploading');
    expect(phases).toContain('success');
    vi.useRealTimers();
  });

  it('Dismiss discards the buffered audio and closes the window', async () => {
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    finalizeRecording.mockRejectedValue(new Error('boom'));
    const wrapper = mount(WaveformView);
    await flushPromises();
    await wrapper.find('.stop-btn').trigger('click');
    await flushPromises();

    await wrapper.find('.dismiss-btn').trigger('click');
    await flushPromises();

    // Keyed by the recording's start timestamp (mocked recorder.startedAt).
    expect(discardPendingAudio).toHaveBeenCalledWith('2026-06-09T10:00:00Z');
    expect(closeWin).toHaveBeenCalled();
    const last = emitEvent.mock.calls.filter(([n]) => n === 'recorder://state').at(-1);
    expect((last?.[1] as { phase: string }).phase).toBe('closed');
  });
});
