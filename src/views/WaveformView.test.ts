// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

const mockState = vi.hoisted(() => ({
  isPaused: { value: false },
  durationSeconds: { value: 0 },
  startedAt: { value: '2026-06-19T14:00:00.000Z' },
  levels: { value: [0.2, 0.4, 0.6] },
  startRecording: vi.fn(),
  stopRecording: vi.fn(),
  pauseRecording: vi.fn(),
  resumeRecording: vi.fn(),
  getAnalyser: vi.fn(),
  waveformStart: vi.fn(),
  waveformStop: vi.fn(),
  finalizeRecording: vi.fn(),
  listen: vi.fn(),
  close: vi.fn(),
}));

vi.mock('vue-router', () => ({
  useRoute: () => ({ query: {} }),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => mockState.listen(...args),
}));

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  getCurrentWebviewWindow: () => ({ close: mockState.close }),
}));

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(async () => ({
    get: vi.fn(async () => 'mic_and_system'),
  })),
}));

vi.mock('../composables/useRecorder', () => ({
  useRecorder: () => ({
    isPaused: mockState.isPaused,
    durationSeconds: mockState.durationSeconds,
    startedAt: mockState.startedAt,
    startRecording: mockState.startRecording,
    stopRecording: mockState.stopRecording,
    pauseRecording: mockState.pauseRecording,
    resumeRecording: mockState.resumeRecording,
    getAnalyser: mockState.getAnalyser,
  }),
}));

vi.mock('../composables/useWaveform', () => ({
  useWaveform: () => ({
    levels: mockState.levels,
    start: mockState.waveformStart,
    stop: mockState.waveformStop,
  }),
}));

vi.mock('../composables/useBackend', () => ({
  getActiveBackend: vi.fn(async () => ({
    id: 'ariso',
    finalizeRecording: mockState.finalizeRecording,
  })),
}));

import WaveformView from './WaveformView.vue';

// Mounts the mini recording window against mocked native APIs so button
// affordances can be tested without invoking the real Tauri recorder pipeline.
async function mountWaveformView() {
  const wrapper = mount(WaveformView);
  await flushPromises();
  return wrapper;
}

beforeEach(() => {
  mockState.isPaused.value = false;
  mockState.durationSeconds.value = 0;
  mockState.startedAt.value = '2026-06-19T14:00:00.000Z';
  mockState.startRecording.mockResolvedValue(undefined);
  mockState.stopRecording.mockResolvedValue(new Blob(['audio'], { type: 'audio/mpeg' }));
  mockState.pauseRecording.mockClear();
  mockState.resumeRecording.mockClear();
  mockState.getAnalyser.mockReturnValue(null);
  mockState.waveformStart.mockClear();
  mockState.waveformStop.mockClear();
  mockState.finalizeRecording.mockResolvedValue({ backend: 'ariso', meetingId: 1 });
  mockState.listen.mockResolvedValue(() => {});
  mockState.close.mockResolvedValue(undefined);
});

describe('WaveformView', () => {
  it('adds tooltip titles to the active recording controls', async () => {
    const wrapper = await mountWaveformView();

    const pauseResume = wrapper.find('.pause-resume-btn');
    const stop = wrapper.find('.stop-btn');
    expect(pauseResume.attributes('title')).toBe('Pause recording');
    expect(pauseResume.attributes('aria-label')).toBe('Pause recording');
    expect(stop.attributes('title')).toBe('Stop and save recording');
    expect(stop.attributes('aria-label')).toBe('Stop and save recording');
  });

  it('keeps the pause/resume tooltip in sync with paused state', async () => {
    mockState.isPaused.value = true;
    const wrapper = await mountWaveformView();

    const pauseResume = wrapper.find('.pause-resume-btn');
    expect(pauseResume.attributes('title')).toBe('Resume recording');
    expect(pauseResume.attributes('aria-label')).toBe('Resume recording');
  });
});
