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
const showWin = vi.fn(() => Promise.resolve());
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
    show: showWin,
    setIgnoreCursorEvents: (...a: unknown[]) => setIgnoreCursorEvents(...a),
  }),
}));
let routeQuery: Record<string, string> = {};
vi.mock('vue-router', () => ({ useRoute: () => ({ query: routeQuery }) }));
const recorderIsRecording = { value: true };
const recorderIsPaused = { value: false };
const recorderDuration = { value: 5 };
const recorderStartedAt = { value: '2026-06-09T10:00:00Z' };
vi.mock('../composables/useRecorder', () => ({
  useRecorder: () => ({
    isRecording: recorderIsRecording,
    isPaused: recorderIsPaused,
    durationSeconds: recorderDuration,
    frameLevels: { value: new Array(32).fill(0.5) },
    lastSoundAt: { value: 0 },
    startedAt: recorderStartedAt,
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
import { SILENCE_PROMPT_MS, SILENCE_GRACE_MS } from '../composables/silenceWatch';

beforeEach(() => {
  vi.clearAllMocks();
  for (const k in eventHandlers) delete eventHandlers[k];
  routeQuery = {};
  recorderIsRecording.value = true;
  recorderIsPaused.value = false;
  recorderDuration.value = 5;
  recorderStartedAt.value = '2026-06-09T10:00:00Z';
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

  it('uses the white logo in the dark recorder pill', async () => {
    const wrapper = mount(WaveformView);
    await flushPromises();
    expect(wrapper.find('.logo').attributes('src')).toContain('oats-tray-white.svg');
  });

  it('adds tooltip titles to active recording controls', async () => {
    const wrapper = mount(WaveformView);
    await flushPromises();
    await wrapper.find('.pill').trigger('mouseenter');
    await flushPromises();

    const pause = wrapper.find('.pause-btn');
    const stop = wrapper.find('.stop-btn');
    expect(pause.attributes('title')).toBe('Pause recording');
    expect(pause.attributes('aria-label')).toBe('Pause recording');
    expect(stop.attributes('title')).toBe('Stop and save recording');
    expect(stop.attributes('aria-label')).toBe('Stop and save recording');
  });

  it('keeps the pause/resume tooltip in sync with paused state', async () => {
    recorderIsPaused.value = true;
    const wrapper = mount(WaveformView);
    await flushPromises();
    await wrapper.find('.pill').trigger('mouseenter');
    await flushPromises();

    const pause = wrapper.find('.pause-btn');
    expect(pause.attributes('title')).toBe('Resume recording');
    expect(pause.attributes('aria-label')).toBe('Resume recording');
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

  it('shows the recorder window before starting capture so getUserMedia can resolve', async () => {
    // WebKit never resolves getUserMedia for a window that isn't actually
    // visible. The pill is hidden behind the library's embedded strip, so it
    // must be shown before capture starts (the watcher re-hides it after).
    mount(WaveformView);
    await flushPromises();
    expect(showWin).toHaveBeenCalled();
    expect(showWin.mock.invocationCallOrder[0]).toBeLessThan(
      startRecording.mock.invocationCallOrder[0],
    );
  });

  it('Resume shows the (hidden) recorder window before restarting capture', async () => {
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    finalizeRecording.mockRejectedValue(new Error('boom'));
    const wrapper = mount(WaveformView);
    await flushPromises();
    await wrapper.find('.stop-btn').trigger('click');
    await flushPromises();
    expect(wrapper.find('.status-icon.err').exists()).toBe(true);

    showWin.mockClear();
    startRecording.mockClear();
    await wrapper.find('.resume-btn').trigger('click');
    await flushPromises();

    // The pill was hidden during the failed state; resuming must re-show it
    // before getUserMedia, or capture hangs and the strip/dot never appear.
    expect(showWin).toHaveBeenCalled();
    expect(showWin.mock.invocationCallOrder[0]).toBeLessThan(
      startRecording.mock.invocationCallOrder[0],
    );
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

  it('shows the silence prompt after 10 min of silence', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(SILENCE_PROMPT_MS + 1_000); // now past lastSoundAt (0) + 10 min
    finalizeRecording.mockResolvedValue({ backend: 'local' });
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    const wrapper = mount(WaveformView);
    await flushPromises();
    invoke.mockClear();
    // One loop tick: should show the prompt but NOT stop.
    await vi.advanceTimersByTimeAsync(1_100);
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('show_silence_prompt');
    expect(stopRecording).not.toHaveBeenCalled();
    vi.useRealTimers();
    wrapper.unmount();
  });

  it('auto-stops 60s after an unanswered silence prompt', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(SILENCE_PROMPT_MS + 1_000); // past 10 min silence threshold
    finalizeRecording.mockResolvedValue({ backend: 'local' });
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    const wrapper = mount(WaveformView);
    await flushPromises();
    invoke.mockClear();
    // First tick: shows the prompt (lastSoundAt stays 0, silence persists).
    await vi.advanceTimersByTimeAsync(1_100);
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('show_silence_prompt');
    expect(stopRecording).not.toHaveBeenCalled();
    // Advance past the 60s grace — still silent, prompt ignored → auto-stop.
    await vi.advanceTimersByTimeAsync(SILENCE_GRACE_MS + 1_000);
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('dismiss_silence_prompt');
    expect(stopRecording).toHaveBeenCalled();
    vi.useRealTimers();
    wrapper.unmount();
  });

  it('silence-prompt://keep reseeds the silence clock so auto-stop is deferred', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(SILENCE_PROMPT_MS + 1_000); // past 10-min silence threshold
    finalizeRecording.mockResolvedValue({ backend: 'local' });
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    const wrapper = mount(WaveformView);
    await flushPromises();
    invoke.mockClear();
    stopRecording.mockClear();
    // First tick: prompt fires (lastSoundAt is 0, silence window exceeded).
    await vi.advanceTimersByTimeAsync(1_100);
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('show_silence_prompt');
    // User taps "Keep recording": reseeds lastSoundAt to fake-now
    // (SILENCE_PROMPT_MS + 1_000 + 1_100ms). promptShownAt is also cleared.
    await eventHandlers['silence-prompt://keep']?.({});
    await flushPromises();
    // Advance past what would have been the 60s auto-stop grace. Since keep
    // reseeded lastSoundAt to ~now, silence hasn't accumulated for 10 min again
    // — so the silence watcher should NOT fire stop within this window.
    await vi.advanceTimersByTimeAsync(SILENCE_GRACE_MS + 1_000);
    await flushPromises();
    expect(stopRecording).not.toHaveBeenCalled();
    vi.useRealTimers();
    wrapper.unmount();
  });

  it('silence-prompt://stop immediately stops the recording', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(SILENCE_PROMPT_MS + 1_000); // past 10-min silence threshold
    finalizeRecording.mockResolvedValue({ backend: 'local' });
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    const wrapper = mount(WaveformView);
    await flushPromises();
    invoke.mockClear();
    stopRecording.mockClear();
    // Show prompt.
    await vi.advanceTimersByTimeAsync(1_100);
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('show_silence_prompt');
    // User taps "Stop now": must trigger stopRecording immediately.
    await eventHandlers['silence-prompt://stop']?.({});
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

  it('auto mode records immediately with no in-pill confirm overlay', async () => {
    // Confirmation now happens before the window opens (the notification
    // prompt), so the pill itself never shows a keep/discard overlay.
    routeQuery = { auto: '1' };
    listScheduledMeetings.mockResolvedValue([]);
    const wrapper = mount(WaveformView);
    await flushPromises();
    expect(wrapper.find('.confirm').exists()).toBe(false);
    expect(startRecording).toHaveBeenCalledWith('mic');
    expect(wrapper.findAll('.bar')).toHaveLength(3);
    routeQuery = {};
    wrapper.unmount();
  });

  it('discards (does not upload) a too-short auto recording on stop', async () => {
    // Duration defaults to 5s (< the 15s minimum), so an auto recording that
    // stops almost immediately is dropped rather than uploaded as a stub.
    routeQuery = { auto: '1' };
    listScheduledMeetings.mockResolvedValue([]);
    const wrapper = mount(WaveformView);
    await flushPromises();
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    // A native mic-off stop arrives while the recording is still under 15s.
    await eventHandlers['auto-record://stop']?.({});
    await flushPromises();
    // Must NOT have uploaded; must have stopped + closed.
    expect(finalizeRecording).not.toHaveBeenCalled();
    expect(stopRecording).toHaveBeenCalled();
    expect(closeWin).toHaveBeenCalled();
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

  it('adds tooltip titles to failed-upload controls', async () => {
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    finalizeRecording.mockRejectedValue(new Error('boom'));
    const wrapper = mount(WaveformView);
    await flushPromises();
    await wrapper.find('.stop-btn').trigger('click');
    await flushPromises();

    const retry = wrapper.find('.retry-btn');
    const resume = wrapper.find('.resume-btn');
    const dismiss = wrapper.find('.dismiss-btn');
    expect(retry.attributes('title')).toBe('Retry upload');
    expect(retry.attributes('aria-label')).toBe('Retry upload');
    expect(resume.attributes('title')).toBe('Continue recording');
    expect(resume.attributes('aria-label')).toBe('Continue recording');
    expect(dismiss.attributes('title')).toBe('Discard recording');
    expect(dismiss.attributes('aria-label')).toBe('Discard recording');
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

    // After resume, isStopping was reset and the timed-out finalize abandoned,
    // so a subsequent stop is accepted and uploads again.
    stopRecording.mockResolvedValue(new Blob(['y'], { type: 'audio/mpeg' }));
    finalizeRecording.mockReset();
    finalizeRecording.mockResolvedValue({ backend: 'local' });
    await wrapper.find('.stop-btn').trigger('click');
    await flushPromises();
    expect(finalizeRecording).toHaveBeenCalled();
  });

  it('Resume re-broadcasts a recording phase so the strip leaves the failed state', async () => {
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    finalizeRecording.mockRejectedValue(new Error('boom'));
    const wrapper = mount(WaveformView);
    await flushPromises();
    await wrapper.find('.stop-btn').trigger('click');
    await flushPromises();

    emitEvent.mockClear();
    await wrapper.find('.resume-btn').trigger('click');
    await flushPromises();

    const phases = emitEvent.mock.calls
      .filter(([name]) => name === 'recorder://state')
      .map(([, p]) => (p as { phase: string }).phase);
    expect(phases).toContain('recording');
    expect(phases).not.toContain('failed');
    wrapper.unmount();
  });

  it('stop after resume uploads the combined blob with original startAt and summed duration', async () => {
    finalizeRecording
      .mockRejectedValueOnce(new Error('boom'))   // first stop fails
      .mockResolvedValue({ backend: 'local' });    // combined upload succeeds
    // First segment: 3 bytes, 5s (defaults).
    stopRecording.mockResolvedValueOnce(
      new Blob([new Uint8Array([1, 2, 3])], { type: 'audio/mpeg' }),
    );
    const wrapper = mount(WaveformView);
    await flushPromises();
    await wrapper.find('.stop-btn').trigger('click');
    await flushPromises();
    expect(wrapper.find('.status-icon.err').exists()).toBe(true);
    const firstMeta = finalizeRecording.mock.calls[0][1] as {
      startAt: string | null;
      durationSeconds: number;
    };
    expect(firstMeta.durationSeconds).toBe(5);

    // Resume; second segment: 2 bytes, 7s.
    await wrapper.find('.resume-btn').trigger('click');
    await flushPromises();
    recorderDuration.value = 7;
    stopRecording.mockResolvedValueOnce(
      new Blob([new Uint8Array([9, 9])], { type: 'audio/mpeg' }),
    );
    await wrapper.find('.stop-btn').trigger('click');
    await flushPromises();

    expect(finalizeRecording).toHaveBeenCalledTimes(2);
    const combinedBlob = finalizeRecording.mock.calls[1][0] as Blob;
    const combinedMeta = finalizeRecording.mock.calls[1][1] as {
      startAt: string | null;
      durationSeconds: number;
    };
    expect(combinedBlob.size).toBe(5);               // 3 + 2 bytes concatenated
    expect(combinedMeta.startAt).toBe('2026-06-09T10:00:00Z'); // original start kept
    expect(combinedMeta.durationSeconds).toBe(12);   // 5 + 7 summed
    expect(wrapper.find('.status-icon.ok').exists()).toBe(true);
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
