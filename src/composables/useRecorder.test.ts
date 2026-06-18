// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// lamejs does real MP3 work we don't need here; stub the encoder.
vi.mock('@breezystack/lamejs', () => ({
  default: {
    Mp3Encoder: class {
      encodeBuffer(): Int8Array {
        return new Int8Array(0);
      }
      flush(): Int8Array {
        return new Int8Array(0);
      }
    },
  },
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

import { useRecorder } from './useRecorder';

class FakeAudioContext {
  destination = {};
  createAnalyser() {
    return {
      frequencyBinCount: 0,
      getByteFrequencyData: () => {},
      connect: () => {},
      disconnect: () => {},
    };
  }
  createMediaStreamSource() {
    return { connect: () => {}, disconnect: () => {} };
  }
  createScriptProcessor() {
    return {
      connect: () => {},
      disconnect: () => {},
      onaudioprocess: null as unknown,
    };
  }
  createGain() {
    return { gain: { value: 0 }, connect: () => {}, disconnect: () => {} };
  }
  close() {}
}

beforeEach(() => {
  (globalThis as unknown as { AudioContext: unknown }).AudioContext =
    FakeAudioContext;
  Object.defineProperty(navigator, 'mediaDevices', {
    configurable: true,
    value: { getUserMedia: vi.fn(async () => ({ getTracks: () => [] })) },
  });
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('useRecorder duration', () => {
  it('tracks wall-clock elapsed time even when interval ticks are throttled', async () => {
    // Fake only the interval; drive Date.now() ourselves to simulate the OS
    // throttling the recorder window's timer in the background.
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
    const t0 = 1_000_000;
    const nowSpy = vi.spyOn(Date, 'now').mockReturnValue(t0);

    const rec = useRecorder();
    await rec.startRecording();

    // 25 minutes of real wall-clock time elapse, but the throttled interval
    // only fires once.
    nowSpy.mockReturnValue(t0 + 25 * 60 * 1000);
    vi.advanceTimersByTime(1000);

    expect(rec.durationSeconds.value).toBe(25 * 60);
  });

  it('excludes paused time from the elapsed duration', async () => {
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
    let now = 1_000_000;
    vi.spyOn(Date, 'now').mockImplementation(() => now);

    const rec = useRecorder();
    await rec.startRecording();

    now += 10_000; // 10s recorded
    rec.pauseRecording();
    now += 60_000; // 60s paused — must not count
    rec.resumeRecording();
    now += 5_000; // 5s more recorded
    vi.advanceTimersByTime(1000);

    expect(rec.durationSeconds.value).toBe(15);
  });

  it('freezes the displayed duration while paused', async () => {
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
    let now = 1_000_000;
    vi.spyOn(Date, 'now').mockImplementation(() => now);

    const rec = useRecorder();
    await rec.startRecording();

    now += 8_000;
    rec.pauseRecording();
    vi.advanceTimersByTime(1000);
    expect(rec.durationSeconds.value).toBe(8);

    now += 30_000; // still paused
    vi.advanceTimersByTime(1000);
    expect(rec.durationSeconds.value).toBe(8);
  });
});
