/**
 * Downsample an array of normalized levels (0–1) into `buckets` averaged bars.
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

/** How much the side bars track the center bar. Voice energy concentrates in
 *  the center's low-frequency bucket; without blending, the side bars (mid /
 *  high buckets) sit nearly dead during speech. */
const SIDE_FOLLOW = 0.5;

/**
 * Bucket levels, then arrange the bars so the lowest-frequency bucket — where
 * voice energy concentrates, making it the most reactive — sits in the
 * center, with later (quieter) buckets fanning out left/right. Each side bar
 * is blended toward the center's level so the whole waveform moves with
 * speech instead of only the middle bar. Renders the recorder's 3-bar
 * waveform.
 */
export function centerWeightedBars(levels: number[], buckets: number): number[] {
  const byFrequency = bucketLevels(levels, buckets);
  const out = new Array<number>(byFrequency.length).fill(0);
  const center = Math.floor(byFrequency.length / 2);
  let left = center - 1;
  let right = center + 1;
  byFrequency.forEach((v, i) => {
    if (i === 0) out[center] = v;
    else if ((i % 2 === 1 && left >= 0) || right >= out.length) out[left--] = v;
    else out[right++] = v;
  });
  for (let i = 0; i < out.length; i++) {
    if (i !== center) out[i] = out[i] * (1 - SIDE_FOLLOW) + out[center] * SIDE_FOLLOW;
  }
  return out;
}
