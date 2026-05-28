import { ref, type Ref } from 'vue';
import lamejs from '@breezystack/lamejs';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type RecordingMode = 'mic' | 'mic_and_system';

const hasTauri =
  typeof window !== 'undefined' &&
  ('__TAURI__' in window || '__TAURI_INTERNALS__' in window);

export function useRecorder() {
  const isRecording: Ref<boolean> = ref(false);
  const isPaused: Ref<boolean> = ref(false);
  const error: Ref<string | null> = ref(null);
  const durationSeconds: Ref<number> = ref(0);
  const startedAt: Ref<string | null> = ref(null);
  const systemAudioSupported: Ref<boolean> = ref(hasTauri);

  let audioContext: AudioContext | null = null;
  let micStream: MediaStream | null = null;
  let micSource: MediaStreamAudioSourceNode | null = null;
  let processor: ScriptProcessorNode | null = null;
  let analyserNode: AnalyserNode | null = null;
  let mp3Encoder: lamejs.Mp3Encoder | null = null;
  let mp3Chunks: Int8Array[] = [];
  let timerInterval: ReturnType<typeof setInterval> | null = null;

  // System audio state
  let systemAudioActive = false;
  let systemAudioUnlisten: UnlistenFn | null = null;
  // Ring buffer for incoming system audio (16kHz Int16 PCM)
  let systemAudioBuffer: Int16Array = new Int16Array(0);

  function getAnalyser(): AnalyserNode | null {
    return analyserNode;
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

    const useSystemAudio = mode === 'mic_and_system' && hasTauri;

    try {
      const debug =
        import.meta.env.DEV && import.meta.env.VITE_DEBUG_AUDIO === 'true';

      micStream = await navigator.mediaDevices.getUserMedia({
        audio: {
          channelCount: 1,
          echoCancellation: !debug,
          noiseSuppression: !debug,
          sampleRate: 44100,
        },
      });

      audioContext = new AudioContext({ sampleRate: 44100 });
      micSource = audioContext.createMediaStreamSource(micStream);

      // Analyser for waveform visualization
      analyserNode = audioContext.createAnalyser();
      micSource.connect(analyserNode);

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

        // Stereo encoder: ch0 = mic, ch1 = system audio
        mp3Encoder = new lamejs.Mp3Encoder(2, 44100, 128);
      } else {
        // Mono mic-only encoder
        mp3Encoder = new lamejs.Mp3Encoder(1, 44100, 128);
      }

      // ScriptProcessor to capture PCM samples
      processor = audioContext.createScriptProcessor(4096, 1, 1);
      processor.onaudioprocess = (e: AudioProcessingEvent) => {
        if (!isRecording.value || isPaused.value || !mp3Encoder) return;

        const samples = e.inputBuffer.getChannelData(0);
        const micInt16 = new Int16Array(samples.length);
        for (let i = 0; i < samples.length; i++) {
          const s = Math.max(-1, Math.min(1, samples[i]));
          micInt16[i] = s < 0 ? s * 0x8000 : s * 0x7fff;
        }

        let mp3buf: Int8Array;
        if (useSystemAudio) {
          // Drain system audio buffer, padded/trimmed to match mic frame size
          const sysRaw = drainSystemAudio(micInt16.length);
          const sysInt16 = new Int16Array(micInt16.length); // zero-filled
          if (sysRaw) {
            sysInt16.set(sysRaw.slice(0, micInt16.length));
          }
          mp3buf = mp3Encoder.encodeBuffer(micInt16, sysInt16);
        } else {
          mp3buf = mp3Encoder.encodeBuffer(micInt16);
        }

        if (mp3buf.length > 0) {
          mp3Chunks.push(new Int8Array(mp3buf));
        }
      };

      micSource.connect(processor);
      processor.connect(audioContext.destination);

      isRecording.value = true;
      isPaused.value = false;
      startedAt.value = new Date().toISOString();

      timerInterval = setInterval(() => {
        if (!isPaused.value) {
          durationSeconds.value++;
        }
      }, 1000);
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err);
      await cleanup();
      throw err;
    }
  }

  function pauseRecording(): void {
    if (!isRecording.value || isPaused.value) return;
    isPaused.value = true;
  }

  function resumeRecording(): void {
    if (!isRecording.value || !isPaused.value) return;
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
  }

  return {
    isRecording,
    isPaused,
    error,
    durationSeconds,
    startedAt,
    systemAudioSupported,
    getAnalyser,
    startRecording,
    pauseRecording,
    resumeRecording,
    stopRecording,
  };
}
