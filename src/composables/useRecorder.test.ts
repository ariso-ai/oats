// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// Capture every Int16Array passed to encodeBuffer so tests can assert that
// real mic data reached the encoder.  Hoisted so the mock factory can close
// over it before any import resolves.
const encodeCalls = vi.hoisted((): Int16Array[] => []);
const platformCapabilities = vi.hoisted((): {
  value: {
    os: 'macos' | 'windows';
    systemAudio: { supported: boolean; settingsUrl: string | null };
  };
} => ({
  value: {
    os: 'macos',
    systemAudio: { supported: false, settingsUrl: null },
  },
}));
const invoke = vi.hoisted(() => vi.fn(async () => undefined));

// lamejs does real MP3 work we don't need here; stub the encoder.
// encodeBuffer records its argument and returns a non-empty buffer so that
// blob.size > 0 after stop (and so the recorded samples are inspectable).
vi.mock('@breezystack/lamejs', () => ({
  default: {
    Mp3Encoder: class {
      encodeBuffer(left: Int16Array): Int8Array {
        encodeCalls.push(new Int16Array(left));
        return new Int8Array([0x01]);
      }
      flush(): Int8Array {
        return new Int8Array(0);
      }
    },
  },
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

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
vi.mock('./usePlatformCapabilities', () => ({
  loadPlatformCapabilities: () => Promise.resolve(platformCapabilities.value),
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
  // Clear captured listeners, encoder call records, and mock call counts.
  for (const k in listeners) delete listeners[k];
  encodeCalls.length = 0;
  vi.clearAllMocks();
  platformCapabilities.value = {
    os: 'macos',
    systemAudio: { supported: false, settingsUrl: null },
  };
  Object.defineProperty(window, '__TAURI_INTERNALS__', {
    configurable: true,
    value: {},
  });
  Object.defineProperty(navigator, 'mediaDevices', {
    configurable: true,
    value: {
      getUserMedia: vi.fn(async () => ({
        getTracks: () => [{ stop: vi.fn() }],
      })),
    },
  });
  (globalThis as unknown as { AudioContext: unknown }).AudioContext =
    FakeAudioContext;
});

afterEach(() => {
  delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('useRecorder duration', () => {
  it('rejects system-only capture when it is unsupported', async () => {
    const getUserMedia = vi.mocked(navigator.mediaDevices.getUserMedia);
    const rec = useRecorder();

    await expect(rec.startRecording('system')).rejects.toThrow(
      'System audio recording is not supported',
    );

    expect(getUserMedia).not.toHaveBeenCalled();
    expect(rec.isRecording.value).toBe(false);
  });

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

    // Encoder was fed with the actual mic samples: at least one encodeBuffer
    // call must contain a non-zero value, proving the native event →
    // micAudioBuffer → drainMic → encodeBuffer path carried the 8000-valued
    // samples (not just zero-filled silence).
    expect(encodeCalls.some(buf => buf.some(v => v !== 0))).toBe(true);

    // Blob is also non-empty (encoder returned a byte).
    const blob = await rec.stopRecording();
    expect(blob.size).toBeGreaterThan(0);
  });

  it('uses webview microphone capture on Windows', async () => {
    platformCapabilities.value = {
      os: 'windows',
      systemAudio: { supported: false, settingsUrl: 'ms-settings:sound' },
    };
    const getUserMedia = vi.mocked(navigator.mediaDevices.getUserMedia);
    const rec = useRecorder();

    await rec.startRecording('mic');

    expect(getUserMedia).toHaveBeenCalledOnce();
    const constraints = getUserMedia.mock.calls[0][0];
    expect(constraints.audio).toMatchObject({
      echoCancellation: false,
      noiseSuppression: false,
      autoGainControl: false,
    });
    expect(constraints.audio).not.toHaveProperty('sampleRate');
    expect(constraints.audio).not.toHaveProperty('channelCount');
    expect(startMicrophoneCapture).not.toHaveBeenCalled();
    await rec.stopRecording();
  });

  it('initializes Windows microphone and system audio concurrently', async () => {
    platformCapabilities.value = {
      os: 'windows',
      systemAudio: { supported: true, settingsUrl: 'ms-settings:sound' },
    };
    let resolveMic!: (stream: MediaStream) => void;
    const getUserMedia = vi.fn(
      () => new Promise<MediaStream>((resolve) => { resolveMic = resolve; }),
    );
    Object.defineProperty(navigator, 'mediaDevices', {
      configurable: true,
      value: { getUserMedia },
    });
    const rec = useRecorder();

    const starting = rec.startRecording('mic_and_system');
    await Promise.resolve();
    await Promise.resolve();

    expect(getUserMedia).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledWith('start_system_audio_capture');

    resolveMic({ getTracks: () => [{ stop: vi.fn() }] } as unknown as MediaStream);
    await starting;
    await rec.stopRecording();
  });
});
