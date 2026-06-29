// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// lamejs does real MP3 work we don't need here; stub the encoder.
// Return a non-empty buffer from encodeBuffer so we can detect that the
// encoder was actually fed PCM data (blob.size > 0 after stop).
vi.mock('@breezystack/lamejs', () => ({
  default: {
    Mp3Encoder: class {
      encodeBuffer(): Int8Array {
        return new Int8Array([0x01]);
      }
      flush(): Int8Array {
        return new Int8Array(0);
      }
    },
  },
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

// Capture event listeners by name so tests can push synthetic events.
const listeners: Record<string, (e: { payload: string }) => void> = {};
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: string }) => void) => {
    listeners[name] = cb;
    return () => { delete listeners[name]; };
  }),
}));

// Mock ../tauri so startMicrophoneCapture / stopMicrophoneCapture are available
// without pulling in the Tauri plugin-store dependency.
vi.mock('../tauri', () => ({
  startMicrophoneCapture: vi.fn(async () => {}),
  stopMicrophoneCapture: vi.fn(async () => {}),
}));

import { useRecorder } from './useRecorder';
import { startMicrophoneCapture } from '../tauri';

type AudioProcCb = ((e: unknown) => void) | null;
let lastProcessor: { onaudioprocess: AudioProcCb } | null = null;

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
    const proc = {
      connect: () => {},
      disconnect: () => {},
      onaudioprocess: null as AudioProcCb,
    };
    lastProcessor = proc;
    return proc;
  }
  createGain() {
    return { gain: { value: 0 }, connect: () => {}, disconnect: () => {} };
  }
  close() {}
}

// Minimal AudioProcessingEvent for the mic-only path.
function fireAudioFrame(): void {
  const samples = new Float32Array(4096);
  lastProcessor?.onaudioprocess?.({
    inputBuffer: { length: 4096, getChannelData: () => samples },
    outputBuffer: { getChannelData: () => new Float32Array(4096) },
  });
}

// Encode n Int16 samples at a fixed peak value as the base64 payload that the
// native backend sends in each 'mic-audio-data' event.
function int16ToBase64(samples: Int16Array): string {
  const bytes = new Uint8Array(samples.buffer);
  let bin = '';
  for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]);
  return btoa(bin);
}

function pushMicFrame(peak: number, n = 4096): void {
  const s = new Int16Array(n).fill(peak);
  listeners['mic-audio-data']?.({ payload: int16ToBase64(s) });
}

beforeEach(() => {
  lastProcessor = null;
  // Clear captured listeners and mock call counts from previous tests.
  for (const k in listeners) delete listeners[k];
  vi.clearAllMocks();
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

  it('advances every second off the audio frame clock without a timer tick', async () => {
    // No fake timers: the interval never fires. The audio callback must keep
    // the display current on its own (it runs even while the window is hidden).
    let now = 1_000_000;
    vi.spyOn(Date, 'now').mockImplementation(() => now);

    const rec = useRecorder();
    await rec.startRecording();

    now += 1_000;
    fireAudioFrame();
    expect(rec.durationSeconds.value).toBe(1);

    now += 1_000;
    fireAudioFrame();
    expect(rec.durationSeconds.value).toBe(2);
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

describe('useRecorder mic native capture', () => {
  it('drains mic-audio-data events and encodes them as PCM', async () => {
    const rec = useRecorder();
    await rec.startRecording('mic');

    // The native backend was engaged, not getUserMedia.
    expect(startMicrophoneCapture).toHaveBeenCalledOnce();

    // Push a mic audio frame via the native backend event.
    pushMicFrame(8000);

    // Trigger onaudioprocess, which drains micAudioBuffer and feeds the encoder.
    fireAudioFrame();

    // Encoder was fed: blob is non-empty.
    const blob = await rec.stopRecording();
    expect(blob.size).toBeGreaterThan(0);
  });
});
