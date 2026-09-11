// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils';

const startRecording = vi.fn();
const stopRecording = vi.fn();
const getAnalyser = vi.fn(() => null);
const finalizeRecording = vi.fn();
const loadRecordingEnabled = vi.fn();
const closeWin = vi.fn(() => Promise.resolve());
const setIgnoreCursorEvents = vi.fn(() => Promise.resolve());
const showWin = vi.fn(() => Promise.resolve());
const invoke = vi.fn(() => Promise.resolve());
const backendKind = vi.hoisted(() => ({ value: 'local' as 'local' | 'ariso' }));

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
vi.mock('../composables/useBackend', () => ({
  getActiveBackend: () =>
    Promise.resolve({ id: backendKind.value, finalizeRecording: (...a: unknown[]) => finalizeRecording(...a) }),
}));
vi.mock('../composables/useRecordingPermissions', () => ({
  loadRecordingEnabled: () => loadRecordingEnabled(),
}));
const isSilenceDetectionEnabled = vi.fn(() => Promise.resolve(true));
vi.mock('../composables/useSilenceDetection', () => ({
  isSilenceDetectionEnabled: () => isSilenceDetectionEnabled(),
}));

const listScheduledMeetings = vi.fn(() => Promise.resolve([]));
vi.mock('../composables/useMeetingApi', () => ({
  useMeetingApi: () => ({ listScheduledMeetings: (...a: unknown[]) => listScheduledMeetings(...a) }),
}));

// vi.mock is hoisted before top-level consts, so shared mock handles that the
// factory closes over must live in vi.hoisted() to avoid TDZ errors.
const { discardPendingAudio, recordingIdForStart } = vi.hoisted(() => ({
  discardPendingAudio: vi.fn(() => Promise.resolve()),
  // Default: resolve to the sanitized start id (mirrors Rust sanitize_iso_to_id),
  // i.e. a fresh recording. Tests that exercise the append case override this.
  recordingIdForStart: vi.fn((createdAt: string) =>
    Promise.resolve(createdAt.split('.')[0].replace(/:/g, '-')),
  ),
}));
vi.mock('../tauri', () => ({
  pending: { discardAudio: (...a: unknown[]) => discardPendingAudio(...a) },
  local: { recordingIdForStart: (...a: [string]) => recordingIdForStart(...a) },
}));

import WaveformView from './WaveformView.vue';
import { SILENCE_PROMPT_MS, SILENCE_GRACE_MS } from '../composables/silenceWatch';

// Recorder views own native listeners and timers, so every test must exercise
// their unmount cleanup before another wrapper can observe those side effects.
enableAutoUnmount(afterEach);

/** The phase in the latest recorder://state broadcast: what the native pill
 *  and the Meetings strip render. */
function phases(): string[] {
  return emitEvent.mock.calls
    .filter(([name]) => name === 'recorder://state')
    .map(([, payload]) => (payload as { phase: string }).phase);
}
function lastPhase(): string | undefined {
  return phases().at(-1);
}

beforeEach(() => {
  vi.clearAllMocks();
  for (const k in eventHandlers) delete eventHandlers[k];
  routeQuery = {};
  backendKind.value = 'local';
  recorderIsRecording.value = true;
  recorderIsPaused.value = false;
  recorderDuration.value = 5;
  recorderStartedAt.value = '2026-06-09T10:00:00Z';
  loadRecordingEnabled.mockResolvedValue({ mic: true, systemAudio: false });
  isSilenceDetectionEnabled.mockResolvedValue(true);
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('WaveformView recorder host', () => {
  it('starts recording on mount and keeps the window open', async () => {
    mount(WaveformView);
    await flushPromises();
    expect(startRecording).toHaveBeenCalledWith('mic');
    expect(emitEvent).not.toHaveBeenCalledWith('recording://start-failed', expect.anything());
    expect(closeWin).not.toHaveBeenCalled();
  });

  it('reports an actionable startup failure before closing the recorder window', async () => {
    startRecording.mockRejectedValueOnce(new DOMException('', 'NotFoundError'));

    mount(WaveformView);
    await flushPromises();

    expect(emitEvent).toHaveBeenCalledWith('recording://start-failed', {
      message: 'No microphone was found. Connect or enable a microphone, then try again.',
    });
    expect(closeWin).toHaveBeenCalled();
  });

  it('paints nothing and lets clicks through while it records', async () => {
    const wrapper = mount(WaveformView);
    await flushPromises();
    expect(startRecording).toHaveBeenCalledWith('mic');
    // The pill is drawn natively; this window must not paint or swallow clicks.
    expect(wrapper.element.nodeType).toBe(Node.COMMENT_NODE);
    expect(setIgnoreCursorEvents).toHaveBeenCalledWith(true);
  });

  it('shows the recorder window before starting capture so getUserMedia can resolve', async () => {
    // Webviews don't reliably resolve getUserMedia for a window that isn't on
    // screen, so it is shown before capture starts (the watcher re-hides it).
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
    await eventHandlers['tray://stop-recording']?.({});
    await flushPromises();
    expect(lastPhase()).toBe('failed');

    showWin.mockClear();
    startRecording.mockClear();
    eventHandlers['recorder://continue-recording']?.({ payload: undefined });
    await flushPromises();

    // The window was hidden during the failed state; resuming must re-show it
    // before getUserMedia, or capture hangs and the strip/pill never appear.
    expect(showWin).toHaveBeenCalled();
    expect(showWin.mock.invocationCallOrder[0]).toBeLessThan(
      startRecording.mock.invocationCallOrder[0],
    );
  });

  it('stops, finalizes, shows ✓, and auto-closes on success', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0); // keep Date.now() at epoch so silence backstop (lastSoundAt=0) never trips
    stopRecording.mockResolvedValue(new Blob([new Uint8Array([1, 2, 3])], { type: 'audio/mpeg' }));
    finalizeRecording.mockResolvedValue({ backend: 'local' });
    const wrapper = mount(WaveformView);
    await vi.runOnlyPendingTimersAsync();
    await eventHandlers['tray://stop-recording']?.({});
    await vi.runOnlyPendingTimersAsync();
    expect(finalizeRecording).toHaveBeenCalledTimes(1);
    expect(phases()).toContain('success');
    await vi.advanceTimersByTimeAsync(2000);
    expect(closeWin).toHaveBeenCalled();
    vi.useRealTimers();
  });

  it('shows ✗ and stays open on finalize failure', async () => {
    stopRecording.mockResolvedValue(new Blob([new Uint8Array([1, 2, 3])], { type: 'audio/mpeg' }));
    finalizeRecording.mockRejectedValue(new Error('boom'));
    const wrapper = mount(WaveformView);
    await flushPromises();
    await eventHandlers['tray://stop-recording']?.({});
    await flushPromises();
    expect(lastPhase()).toBe('failed');
    expect(closeWin).not.toHaveBeenCalled();
  });

  it('never broadcasts a success phase when finalize fails', async () => {
    stopRecording.mockResolvedValue(new Blob([new Uint8Array([1, 2, 3])], { type: 'audio/mpeg' }));
    finalizeRecording.mockRejectedValue(new Error('offline'));
    const wrapper = mount(WaveformView);
    await flushPromises();
    await eventHandlers['tray://stop-recording']?.({});
    await flushPromises();

    const phases = emitEvent.mock.calls
      .filter(([name]) => name === 'recorder://state')
      .map(([, payload]) => (payload as { phase: string }).phase);
    expect(phases).toContain('failed');
    expect(phases).not.toContain('success');
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
    expect(invoke).toHaveBeenCalledWith('show_silence_prompt', {});
    expect(stopRecording).not.toHaveBeenCalled();
    vi.useRealTimers();
    wrapper.unmount();
  });

  it('never shows the silence prompt when silence detection is disabled', async () => {
    isSilenceDetectionEnabled.mockResolvedValue(false);
    vi.useFakeTimers();
    vi.setSystemTime(SILENCE_PROMPT_MS + 1_000); // well past the 10-min threshold
    finalizeRecording.mockResolvedValue({ backend: 'local' });
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    const wrapper = mount(WaveformView);
    await flushPromises();
    invoke.mockClear();
    // Advance well past both the prompt and grace windows — the timer must not
    // even be running, so nothing is prompted and the recording is never stopped.
    await vi.advanceTimersByTimeAsync(SILENCE_PROMPT_MS + SILENCE_GRACE_MS + 2_000);
    await flushPromises();
    expect(invoke.mock.calls.some(([cmd]) => cmd === 'show_silence_prompt')).toBe(false);
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
    expect(invoke).toHaveBeenCalledWith('show_silence_prompt', {});
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
    expect(invoke).toHaveBeenCalledWith('show_silence_prompt', {});
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
    expect(invoke).toHaveBeenCalledWith('show_silence_prompt', {});
    // User taps "Stop now": must trigger stopRecording immediately.
    await eventHandlers['silence-prompt://stop']?.({});
    await flushPromises();
    expect(stopRecording).toHaveBeenCalled();
    vi.useRealTimers();
    wrapper.unmount();
  });

  it('broadcasts starting, but not recording, before capture has started', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    // getUserMedia still pending: the recorder reports not-recording.
    recorderIsRecording.value = false;
    mount(WaveformView);
    await vi.runOnlyPendingTimersAsync();
    emitEvent.mockClear();
    await vi.advanceTimersByTimeAsync(2_100); // heartbeats fire
    const phases = emitEvent.mock.calls
      .filter(([name]) => name === 'recorder://state')
      .map(([, payload]) => (payload as { phase: string }).phase);
    expect(phases).toContain('starting');
    expect(phases).not.toContain('recording');
    vi.useRealTimers();
  });

  it('broadcasts the meeting created by an unattached cloud upload', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    backendKind.value = 'ariso';
    stopRecording.mockResolvedValue(new Blob([new Uint8Array([1, 2, 3])], { type: 'audio/mpeg' }));
    finalizeRecording.mockResolvedValue({ backend: 'ariso', meetingId: 77 });

    const wrapper = mount(WaveformView);
    await vi.runOnlyPendingTimersAsync();
    await eventHandlers['tray://stop-recording']?.({});
    await vi.runOnlyPendingTimersAsync();

    const success = emitEvent.mock.calls
      .filter(([name, payload]) =>
        name === 'recorder://state' && (payload as { phase: string }).phase === 'success')
      .at(-1)?.[1] as { meetingId: number | null } | undefined;
    expect(success?.meetingId).toBe(77);
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
    await eventHandlers['tray://stop-recording']?.({});
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

  it('broadcasts the append-target id when resuming the recent recording', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    // Rust resolves this session to an EXISTING recording (append within window),
    // so the strip must dock to that recording's row, not a new-session id.
    recordingIdForStart.mockResolvedValueOnce('2026-06-02T10-00-00Z');
    mount(WaveformView);
    await vi.runOnlyPendingTimersAsync();
    await vi.advanceTimersByTimeAsync(1_100); // heartbeat

    const state = emitEvent.mock.calls
      .filter(([name]) => name === 'recorder://state')
      .map(([, payload]) => payload as { localRecordingId: string | null })
      .at(-1);
    // Second arg is forceNew, false here — this route has no forceNew=1 query.
    expect(recordingIdForStart).toHaveBeenCalledWith('2026-06-09T10:00:00Z', false);
    expect(state?.localRecordingId).toBe('2026-06-02T10-00-00Z');
    vi.useRealTimers();
  });

  it('resolves a forced-new id when the route carries forceNew=1, and finalizes with forceNew', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    routeQuery = { forceNew: '1' };
    stopRecording.mockResolvedValue(new Blob([new Uint8Array([1, 2, 3])], { type: 'audio/mpeg' }));
    finalizeRecording.mockResolvedValue({ backend: 'local' });
    const wrapper = mount(WaveformView);
    await vi.runOnlyPendingTimersAsync();

    // The empty-detail Start button routes here with forceNew=1: recordingIdForStart
    // must be asked for a brand-new id, never an append target.
    expect(recordingIdForStart).toHaveBeenCalledWith('2026-06-09T10:00:00Z', true);

    await eventHandlers['tray://stop-recording']?.({});
    await vi.runOnlyPendingTimersAsync();

    const meta = finalizeRecording.mock.calls[0][1] as { forceNew?: boolean };
    expect(meta.forceNew).toBe(true);
    routeQuery = {};
    vi.useRealTimers();
  });

  it('auto mode records immediately with no confirmation step', async () => {
    // Confirmation happens before the window opens (the notification prompt).
    routeQuery = { auto: '1' };
    listScheduledMeetings.mockResolvedValue([]);
    const wrapper = mount(WaveformView);
    await flushPromises();
    expect(startRecording).toHaveBeenCalledWith('mic');
    expect(closeWin).not.toHaveBeenCalled();
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

  it('Resume clears the failed state, restarts recording, and keeps the blob', async () => {
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    finalizeRecording.mockRejectedValue(new Error('boom'));
    const wrapper = mount(WaveformView);
    await flushPromises();
    await eventHandlers['tray://stop-recording']?.({});
    await flushPromises();
    expect(lastPhase()).toBe('failed');

    startRecording.mockClear();
    eventHandlers['recorder://continue-recording']?.({ payload: undefined });
    await flushPromises();

    // Back in the live recording view, mic restarted, nothing discarded/closed.
    expect(startRecording).toHaveBeenCalledTimes(1);
    expect(lastPhase()).not.toBe('failed');
    expect(lastPhase()).toBe('recording');
    expect(discardPendingAudio).not.toHaveBeenCalled();
    expect(closeWin).not.toHaveBeenCalled();

    // After resume, isStopping was reset and the timed-out finalize abandoned,
    // so a subsequent stop is accepted and uploads again.
    stopRecording.mockResolvedValue(new Blob(['y'], { type: 'audio/mpeg' }));
    finalizeRecording.mockReset();
    finalizeRecording.mockResolvedValue({ backend: 'local' });
    await eventHandlers['tray://stop-recording']?.({});
    await flushPromises();
    expect(finalizeRecording).toHaveBeenCalled();
  });

  it('Resume re-broadcasts a recording phase so the strip leaves the failed state', async () => {
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    finalizeRecording.mockRejectedValue(new Error('boom'));
    const wrapper = mount(WaveformView);
    await flushPromises();
    await eventHandlers['tray://stop-recording']?.({});
    await flushPromises();

    emitEvent.mockClear();
    eventHandlers['recorder://continue-recording']?.({ payload: undefined });
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
    await eventHandlers['tray://stop-recording']?.({});
    await flushPromises();
    expect(lastPhase()).toBe('failed');
    const firstMeta = finalizeRecording.mock.calls[0][1] as {
      startAt: string | null;
      durationSeconds: number;
    };
    expect(firstMeta.durationSeconds).toBe(5);

    // Resume; second segment: 2 bytes, 7s.
    eventHandlers['recorder://continue-recording']?.({ payload: undefined });
    await flushPromises();
    recorderDuration.value = 7;
    stopRecording.mockResolvedValueOnce(
      new Blob([new Uint8Array([9, 9])], { type: 'audio/mpeg' }),
    );
    await eventHandlers['tray://stop-recording']?.({});
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
    expect(phases()).toContain('success');
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
    await eventHandlers['tray://stop-recording']?.({});
    await vi.runOnlyPendingTimersAsync();
    expect(lastPhase()).toBe('failed');

    eventHandlers['recorder://retry-upload']?.({ payload: undefined });
    await vi.runOnlyPendingTimersAsync();

    expect(finalizeRecording).toHaveBeenCalledTimes(2);
    // Same blob and meta on both attempts — retry must not re-derive anything.
    expect(finalizeRecording.mock.calls[1][0]).toBe(finalizeRecording.mock.calls[0][0]);
    expect(finalizeRecording.mock.calls[1][1]).toEqual(finalizeRecording.mock.calls[0][1]);
    expect(phases()).toContain('success');
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
    await eventHandlers['tray://stop-recording']?.({});
    await vi.runOnlyPendingTimersAsync();
    emitEvent.mockClear();

    eventHandlers['recorder://retry-upload']?.({ payload: undefined });
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
    await eventHandlers['tray://stop-recording']?.({});
    await flushPromises();

    eventHandlers['recorder://discard-recording']?.({ payload: undefined });
    await flushPromises();

    // Keyed by the recording's start timestamp (mocked recorder.startedAt).
    expect(discardPendingAudio).toHaveBeenCalledWith('2026-06-09T10:00:00Z');
    expect(closeWin).toHaveBeenCalled();
    const last = emitEvent.mock.calls.filter(([n]) => n === 'recorder://state').at(-1);
    expect((last?.[1] as { phase: string }).phase).toBe('closed');
  });

  it('closes the failed pill when the sidebar pending-upload retry succeeds', async () => {
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    finalizeRecording.mockRejectedValue(new Error('boom'));
    const wrapper = mount(WaveformView);
    await flushPromises();
    await eventHandlers['tray://stop-recording']?.({});
    await flushPromises();
    expect(lastPhase()).toBe('failed');

    // The Pending-uploads retry uploaded and discarded this same buffer: the
    // stale failed pill must go away (window closes → broadcasts 'closed'), and
    // the window must not re-discard or re-upload the already-gone buffer.
    emitEvent.mockClear();
    eventHandlers['pending-upload://succeeded']?.({ payload: undefined });
    await flushPromises();

    expect(closeWin).toHaveBeenCalled();
    const last = emitEvent.mock.calls.filter(([n]) => n === 'recorder://state').at(-1);
    expect((last?.[1] as { phase: string }).phase).toBe('closed');
    expect(discardPendingAudio).not.toHaveBeenCalled();
  });

  it('ignores a pending-upload success while still actively recording', async () => {
    const wrapper = mount(WaveformView);
    await flushPromises();

    eventHandlers['pending-upload://succeeded']?.({ payload: undefined });
    await flushPromises();

    expect(closeWin).not.toHaveBeenCalled();
    wrapper.unmount();
  });
});

// The native pill drives the failed-upload controls through events.
describe('WaveformView native pill controls', () => {
  async function mountFailedUpload() {
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    finalizeRecording.mockRejectedValueOnce(new Error('boom'));
    const wrapper = mount(WaveformView);
    await flushPromises();
    await eventHandlers['tray://stop-recording']?.({});
    await flushPromises();
    expect(finalizeRecording).toHaveBeenCalledTimes(1);
    expect(lastPhase()).toBe('failed');
    return wrapper;
  }

  it('recorder://retry-upload re-runs finalize with the held recording', async () => {
    await mountFailedUpload();
    finalizeRecording.mockResolvedValue({ backend: 'local' });
    emitEvent.mockClear();

    await eventHandlers['recorder://retry-upload']?.({ payload: undefined });
    await flushPromises();

    expect(finalizeRecording).toHaveBeenCalledTimes(2);
    expect(finalizeRecording.mock.calls[1][0]).toBe(finalizeRecording.mock.calls[0][0]);
    const phases = emitEvent.mock.calls
      .filter(([name]) => name === 'recorder://state')
      .map(([, p]) => (p as { phase: string }).phase);
    expect(phases).toContain('success');
  });

  it('recorder://continue-recording resumes capture and keeps the audio', async () => {
    await mountFailedUpload();
    startRecording.mockClear();

    await eventHandlers['recorder://continue-recording']?.({ payload: undefined });
    await flushPromises();

    expect(startRecording).toHaveBeenCalledTimes(1);
    expect(discardPendingAudio).not.toHaveBeenCalled();
    expect(closeWin).not.toHaveBeenCalled();
  });

  it('recorder://discard-recording discards the buffered audio and closes', async () => {
    await mountFailedUpload();

    await eventHandlers['recorder://discard-recording']?.({ payload: undefined });
    await flushPromises();

    expect(discardPendingAudio).toHaveBeenCalledWith('2026-06-09T10:00:00Z');
    expect(closeWin).toHaveBeenCalled();
  });

  it('ignores the failed-upload events while recording', async () => {
    mount(WaveformView);
    await flushPromises();
    startRecording.mockClear();

    await eventHandlers['recorder://retry-upload']?.({ payload: undefined });
    await eventHandlers['recorder://continue-recording']?.({ payload: undefined });
    await eventHandlers['recorder://discard-recording']?.({ payload: undefined });
    await flushPromises();

    expect(finalizeRecording).not.toHaveBeenCalled();
    expect(startRecording).not.toHaveBeenCalled();
    expect(discardPendingAudio).not.toHaveBeenCalled();
    expect(closeWin).not.toHaveBeenCalled();
  });
});

// A pending upload used to block starting a new recording outright: the failed
// pill kept the one recorder window alive, and every entry point no-opped into
// focusing it (#313). The native side now asks the incumbent pill to stand down.
describe('WaveformView recorder://yield handshake', () => {
  it('yields the recorder window from a failed pill, leaving the buffer for the sidebar', async () => {
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    finalizeRecording.mockRejectedValue(new Error('boom'));
    const wrapper = mount(WaveformView);
    await flushPromises();
    await eventHandlers['tray://stop-recording']?.({});
    await flushPromises();
    expect(lastPhase()).toBe('failed');

    emitEvent.mockClear();
    eventHandlers['recorder://yield']?.({ payload: undefined });
    await flushPromises();

    expect(closeWin).toHaveBeenCalled();
    const last = emitEvent.mock.calls.filter(([n]) => n === 'recorder://state').at(-1);
    expect((last?.[1] as { phase: string }).phase).toBe('closed');
    // The audio stays on disk so Pending uploads can still retry it — this is a
    // handoff, not a discard.
    expect(discardPendingAudio).not.toHaveBeenCalled();
  });

  it('yields the recorder window from the success pill without waiting out its close timer', async () => {
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    finalizeRecording.mockResolvedValue({ backend: 'local' });
    const wrapper = mount(WaveformView);
    await flushPromises();
    await eventHandlers['tray://stop-recording']?.({});
    await flushPromises();

    eventHandlers['recorder://yield']?.({ payload: undefined });
    await flushPromises();

    expect(closeWin).toHaveBeenCalled();
    wrapper.unmount();
  });

  it('refuses to yield while still capturing', async () => {
    const wrapper = mount(WaveformView);
    await flushPromises();

    eventHandlers['recorder://yield']?.({ payload: undefined });
    await flushPromises();

    // Killing a live recording to make room for a new one would lose audio.
    expect(closeWin).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it('refuses to yield while an upload is in flight', async () => {
    stopRecording.mockResolvedValue(new Blob(['x'], { type: 'audio/mpeg' }));
    let settleUpload: (r: { backend: string }) => void = () => {};
    finalizeRecording.mockReturnValue(
      new Promise((resolve) => {
        settleUpload = resolve;
      }),
    );
    const wrapper = mount(WaveformView);
    await flushPromises();
    void eventHandlers['tray://stop-recording']?.({});
    await flushPromises();

    eventHandlers['recorder://yield']?.({ payload: undefined });
    await flushPromises();

    // Tearing the window down here can strand a buffer whose upload already
    // succeeded (the post-upload discard runs in this window), which the next
    // retry would then upload a second time.
    expect(closeWin).not.toHaveBeenCalled();

    settleUpload({ backend: 'local' });
    await flushPromises();
    wrapper.unmount();
  });
});
