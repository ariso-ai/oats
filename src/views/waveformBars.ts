/**
 * Downsample an array of normalized levels (0–1) into `buckets` averaged bars.
 * Used to render the 5-bar recorder waveform from the 32-bin audio analyser.
 */
export function bucketLevels(levels: number[], buckets: number): number[] {
  if (buckets <= 0) return [];
  if (levels.length === 0) return new Array(buckets).fill(0);
  const size = levels.length / buckets;
  const out: number[] = [];
  for (let b = 0; b < buckets; b++) {
    const start = Math.floor(b * size);
    const end = Math.max(Math.floor((b + 1) * size), start + 1);
    const slice = levels.slice(start, end);
    const sum = slice.reduce((acc, v) => acc + v, 0);
    out.push(slice.length ? sum / slice.length : 0);
  }
  return out;
}
