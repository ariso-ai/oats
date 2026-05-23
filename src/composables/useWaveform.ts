import { ref, onUnmounted } from 'vue';

export function useWaveform() {
  const levels = ref<number[]>(new Array(32).fill(0));
  let analyser: AnalyserNode | null = null;
  let animationId: number | null = null;
  let dataArray: Uint8Array<ArrayBuffer> | null = null;

  function start(analyserNode: AnalyserNode): void {
    stop();
    analyser = analyserNode;
    analyser.fftSize = 64;
    dataArray = new Uint8Array(analyser.frequencyBinCount) as Uint8Array<ArrayBuffer>;
    tick();
  }

  function tick(): void {
    if (!analyser || !dataArray) return;
    analyser.getByteFrequencyData(dataArray);

    const newLevels: number[] = [];
    for (let i = 0; i < 32; i++) {
      newLevels.push(dataArray[i] / 255);
    }
    levels.value = newLevels;

    animationId = requestAnimationFrame(tick);
  }

  function stop(): void {
    if (animationId != null) {
      cancelAnimationFrame(animationId);
      animationId = null;
    }
    analyser = null;
    dataArray = null;
    levels.value = new Array(32).fill(0);
  }

  onUnmounted(stop);

  return { levels, start, stop };
}
