import { ref, type Ref } from 'vue';
import lamejs from '@breezystack/lamejs';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type { RecordingMode } from '../views/recordingSettings';
import type { RecordingMode } from '../views/recordingSettings';

const hasTauri =
  typeof window !== 'undefined' &&
  ('__TAURI__' in window || '__TAURI_INTERNALS__' in window);

// Peak (absolute Int16) below which a frame counts as "no sound activity".
// ~0.01 of full scale (32768) — above the noise floor, below real speech.
const SILENCE_LEVEL = 300;

export function useRecorder() {
  const isRecording: Ref<boolean> = ref(false);
  const isPaused: Ref<boolean> = ref(false);
  const error: Ref<string | null> = ref(null);
  const durationSeconds: Ref<number> = ref(0);
  const startedAt: Ref<string | null> = ref(null);
  const systemAudioSupported: Ref<boolean> = ref(hasTauri);
  // Wall-clock of the last frame that carried real sound, for the silence
  // backstop. Seeded on start and reset on resume so paused gaps don't count.
  const lastSoundAt: Ref<number> = ref(Date.now());
  // Analyser spectrum (0–1 per bin) sampled once per audio frame (~10/s).
  // Unlike a rAF-driven loop, this keeps updating while the window is hidden,
  // so it can drive waveform mirrors in other windows.
  const frameLevels: Ref<number[]> = ref([]);

  let audioContext: AudioContext | null = null;
  let micStream: MediaStream | null = null;
  let micSource: MediaStreamAudioSourceNode | null = null;
  let processor: ScriptProcessorNode | null = null;
  let analyserNode: AnalyserNode | null = null;
  // Silent sink used only in system-audio-only mode to keep the Web Audio graph
  // pulling (so the ScriptProcessor fires) without playing the captured audio.
  let systemGain: GainNode | null = null;
  let mp3Encoder: lamejs.Mp3Encoder | null = null;
  let mp3Chunks: Int8Array[] = [];
  let timerInterval: ReturnType<typeof setInterval> | null = null;
  // Wall-clock anchors for the elapsed-time display. The duration is derived
  // from these on each tick rather than incremented per tick, so a throttled
  // timer (the recorder window runs in the background while the user is in
  // their meeting app, and the OS slows its JS timers) can't lose real time.
  let recordingStartMs = 0;
  let pausedAccumMs = 0;
  let pausedAtMs: number | null = null;

  // System audio state
  let systemAudioActive = false;
  let systemAudioUnlisten: UnlistenFn | null = null;
  // Ring buffer for incoming system audio (16kHz Int16 PCM)
  let systemAudioBuffer: Int16Array = new Int16Array(0);

  function getAnalyser(): AnalyserNode | null {
    return analyserNode;
  }

  // Recompute the elapsed-time display from wall-clock, subtracting paused
  // gaps (including any in-progress pause). Called both from the timer and
  // from the audio frame callback — the latter fires ~10/s even while the
  // window is backgrounded, so the display ticks every second even when the
  // OS throttles the JS timer below 1 Hz.
  function recomputeDuration(): void {
    if (!recordingStartMs) return;
    const now = Date.now();
    const pausedSoFar =
      pausedAccumMs + (pausedAtMs !== null ? now - pausedAtMs : 0);
    durationSeconds.value = Math.max(
      0,
      Math.floor((now - recordingStartMs - pausedSoFar) / 1000)
    );
  }

  /**
   * Resample Int16 PCM from srcRate to dstRate using linear interpolation.
   */
  function resampleInt16(
    src: Int16Array,
    srcRate: number,
    dstRate: number
  ): Int16Array {
    if (srcRate === dstRate) return src;
    const ratio = srcRate / dstRate;
    const outLen = Math.round(src.length / ratio);
    const out = new Int16Array(outLen);
    for (let i = 0; i < outLen; i++) {
      const srcIdx = i * ratio;
      const lo = Math.floor(srcIdx);
      const hi = Math.min(lo + 1, src.length - 1);
      const frac = srcIdx - lo;
      out[i] = Math.round(src[lo] * (1 - frac) + src[hi] * frac);
    }
    return out;
  }

  /**
   * Drain system audio buffer and resample to match the requested sample count
   * at 44100Hz. Returns an Int16Array of exactly `sampleCount` samples, or
   * null if no system audio is available.
   */
  function drainSystemAudio(sampleCount: number): Int16Array | null {
    if (systemAudioBuffer.length === 0) return null;

    // How many 16kHz samples correspond to `sampleCount` at 44100Hz
    const needed16k = Math.round(sampleCount * (16000 / 44100));
    const available = Math.min(needed16k, systemAudioBuffer.length);

    // Take what we have
    const chunk = systemAudioBuffer.slice(0, available);
    systemAudioBuffer = systemAudioBuffer.slice(available);

    // Resample from 16kHz to 44100Hz
    return resampleInt16(chunk, 16000, 44100);
  }

  async function startRecording(mode?: RecordingMode): Promise<void> {
    error.value = null;
    durationSeconds.value = 0;
    startedAt.value = null;
    mp3Chunks = [];
    systemAudioBuffer = new Int16Array(0);

    const useSystemAudio = (mode === 'mic_and_system' || mode === 'system') && hasTauri;
    const useMic = mode !== 'system';
    // Outside Tauri, system-audio-only collapses to no usable input; fail fast
    // rather than building a silent graph and recording zeroes.
    if (!useMic && !useSystemAudio) {
      throw new Error('No recording source is available');
    }

    try {
      audioContext = new AudioContext({ sampleRate: 44100 });
      analyserNode = audioContext.createAnalyser();

      if (useMic) {
        // Capture the mic raw — no echo cancellation, noise suppression, or
        // auto gain control. Enabling any of these routes the mic through
        // macOS Voice-Processing I/O, whose AGC drives the *shared* physical
        // input device gain down. A video-conference app reading the same
        // microphone then captures a quieter signal, so the remote end hears
        // the user's voice drop while Oats records. We capture system audio on
        // a separate channel anyway, so we don't need echo cancellation to keep
        // the other participants out of the mic channel.
        micStream = await navigator.mediaDevices.getUserMedia({
          audio: {
            channelCount: 1,
            echoCancellation: false,
            noiseSuppression: false,
            autoGainControl: false,
            sampleRate: 44100,
          },
        });
        micSource = audioContext.createMediaStreamSource(micStream);
        // Analyser for waveform visualization (mic path)
        micSource.connect(analyserNode);
      }

      if (useSystemAudio) {
        // Start Tauri system audio capture and listen for PCM data events
        await invoke('start_system_audio_capture');
        systemAudioActive = true;

        const unlisten = await listen<string>('system-audio-data', (event) => {
          if (!isRecording.value || isPaused.value) return;

          // Decode base64 → raw bytes → Int16 PCM
          const b64 = event.payload;
          const binary = atob(b64);
          const bytes = new Uint8Array(binary.length);
          for (let i = 0; i < binary.length; i++) {
            bytes[i] = binary.charCodeAt(i);
          }
          const samples = new Int16Array(bytes.buffer);

          // Append to ring buffer
          const merged = new Int16Array(
            systemAudioBuffer.length + samples.length
          );
          merged.set(systemAudioBuffer);
          merged.set(samples, systemAudioBuffer.length);
          systemAudioBuffer = merged;
        });
        systemAudioUnlisten = unlisten;
      }

      // Stereo (ch0 mic, ch1 system) only when mixing both; otherwise mono.
      const channels = useMic && useSystemAudio ? 2 : 1;
      mp3Encoder = new lamejs.Mp3Encoder(channels, 44100, 128);

      // ScriptProcessor to capture PCM samples. Fires whenever it is connected
      // to the destination — even with no input (the system-audio-only case).
      processor = audioContext.createScriptProcessor(4096, 1, 1);
      processor.onaudioprocess = (e: AudioProcessingEvent) => {
        // Sample the analyser before the paused/stopped early-return so the
        // waveform mirrors keep moving (matching the local rAF display, which
        // also keeps reading the analyser while paused).
        if (analyserNode) {
          const bins = new Uint8Array(analyserNode.frequencyBinCount);
          analyserNode.getByteFrequencyData(bins);
          frameLevels.value = Array.from(bins, (v) => v / 255);
        }
        // Advance the elapsed-time display off the (un-throttled) audio clock
        // so it ticks every second even when the window's JS timer is slowed.
        recomputeDuration();
        if (!isRecording.value || isPaused.value || !mp3Encoder) return;
        const frame = e.inputBuffer.length;
        let drainPeakRef: Int16Array = new Int16Array(0);

        // Mic channel (silent zero when mic is disabled)
        const micInt16 = new Int16Array(frame);
        if (useMic) {
          const samples = e.inputBuffer.getChannelData(0);
          for (let i = 0; i < samples.length; i++) {
            const s = Math.max(-1, Math.min(1, samples[i]));
            micInt16[i] = s < 0 ? s * 0x8000 : s * 0x7fff;
          }
        }

        let mp3buf: Int8Array;
        if (useMic && useSystemAudio) {
          // Drain system audio buffer, padded/trimmed to match mic frame size
          const sysRaw = drainSystemAudio(micInt16.length);
          const sysInt16 = new Int16Array(micInt16.length); // zero-filled
          if (sysRaw) {
            sysInt16.set(sysRaw.slice(0, micInt16.length));
          }
          drainPeakRef = sysInt16;
          mp3buf = mp3Encoder.encodeBuffer(micInt16, sysInt16);
        } else if (useSystemAudio) {
          // System-audio-only: mono encode from the ring buffer, and feed the
          // analyser by writing the same samples to the processor output (which
          // routes through analyser → silent gain → destination).
          const sysRaw = drainSystemAudio(frame);
          const sysInt16 = new Int16Array(frame); // zero-filled
          if (sysRaw) {
            sysInt16.set(sysRaw.slice(0, frame));
          }
          drainPeakRef = sysInt16;
          mp3buf = mp3Encoder.encodeBuffer(sysInt16);
          const out = e.outputBuffer.getChannelData(0);
          for (let i = 0; i < frame; i++) out[i] = sysInt16[i] / 0x8000;
        } else {
          // Mic-only mono encode
          mp3buf = mp3Encoder.encodeBuffer(micInt16);
        }

        if (mp3buf.length > 0) {
          mp3Chunks.push(new Int8Array(mp3buf));
        }

        // Silence backstop: note the time if this frame carried real sound on
        // any active source (mic and/or system).
        let peak = 0;
        for (let i = 0; i < micInt16.length; i++) {
          const a = Math.abs(micInt16[i]);
          if (a > peak) peak = a;
        }
        if (useSystemAudio) {
          const sys = drainPeakRef;
          for (let i = 0; i < sys.length; i++) {
            const a = Math.abs(sys[i]);
            if (a > peak) peak = a;
          }
        }
        if (peak >= SILENCE_LEVEL) {
          lastSoundAt.value = Date.now();
        }
      };

      if (useMic) {
        micSource!.connect(processor);
        processor.connect(audioContext.destination);
      } else {
        // System-audio-only graph: processor → analyser → gain(0) → destination.
        // The zero gain keeps playback silent while the connection to the
        // destination keeps the processor firing.
        systemGain = audioContext.createGain();
        systemGain.gain.value = 0;
        processor.connect(analyserNode);
        analyserNode.connect(systemGain);
        systemGain.connect(audioContext.destination);
      }

      isRecording.value = true;
      isPaused.value = false;
      startedAt.value = new Date().toISOString();
      lastSoundAt.value = Date.now();
      recordingStartMs = Date.now();
      pausedAccumMs = 0;
      pausedAtMs = null;

      // Backstop tick in case audio frames stop flowing; the audio callback
      // drives the per-second updates while recording.
      timerInterval = setInterval(recomputeDuration, 1000);
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err);
      await cleanup();
      throw err;
    }
  }

  function pauseRecording(): void {
    if (!isRecording.value || isPaused.value) return;
    pausedAtMs = Date.now();
    isPaused.value = true;
  }

  function resumeRecording(): void {
    if (!isRecording.value || !isPaused.value) return;
    // Fold the just-ended pause into the running total so it's excluded from
    // the elapsed duration.
    if (pausedAtMs !== null) {
      pausedAccumMs += Date.now() - pausedAtMs;
      pausedAtMs = null;
    }
    // Reset so the paused interval never counts as silence.
    lastSoundAt.value = Date.now();
    isPaused.value = false;
  }

  async function stopRecording(): Promise<Blob> {
    if (!mp3Encoder) {
      await cleanup();
      return new Blob([], { type: 'audio/mpeg' });
    }

    const remaining = mp3Encoder.flush();
    if (remaining.length > 0) {
      mp3Chunks.push(new Int8Array(remaining));
    }

    const blob = new Blob(mp3Chunks, { type: 'audio/mpeg' });
    await cleanup();
    return blob;
  }

  async function cleanup(): Promise<void> {
    if (timerInterval) {
      clearInterval(timerInterval);
      timerInterval = null;
    }

    // Stop system audio capture
    if (systemAudioActive) {
      try {
        await invoke('stop_system_audio_capture');
      } catch {
        // Already stopped or not available
      }
      systemAudioActive = false;
    }
    if (systemAudioUnlisten) {
      systemAudioUnlisten();
      systemAudioUnlisten = null;
    }
    systemAudioBuffer = new Int16Array(0);

    if (processor && micSource) {
      try {
        micSource.disconnect(processor);
      } catch {
        // Already disconnected
      }
    }
    if (processor) {
      try {
        processor.disconnect();
      } catch {
        // Already disconnected
      }
    }
    if (analyserNode && micSource) {
      try {
        micSource.disconnect(analyserNode);
      } catch {
        // Already disconnected
      }
    }
    if (systemGain) {
      try {
        systemGain.disconnect();
      } catch {
        // Already disconnected
      }
      systemGain = null;
    }
    if (analyserNode) {
      try {
        analyserNode.disconnect();
      } catch {
        // Already disconnected
      }
    }
    processor = null;
    micSource = null;
    analyserNode = null;

    if (micStream) {
      micStream.getTracks().forEach((track) => track.stop());
    }
    micStream = null;

    if (audioContext) {
      try {
        audioContext.close();
      } catch {
        // Already closed
      }
    }
    audioContext = null;

    mp3Encoder = null;
    mp3Chunks = [];
    isRecording.value = false;
    isPaused.value = false;
    frameLevels.value = [];
  }

  return {
    isRecording,
    isPaused,
    error,
    durationSeconds,
    startedAt,
    lastSoundAt,
    frameLevels,
    systemAudioSupported,
    getAnalyser,
    startRecording,
    pauseRecording,
    resumeRecording,
    stopRecording,
  };
}
