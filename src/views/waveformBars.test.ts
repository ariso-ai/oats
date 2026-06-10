import { describe, it, expect } from 'vitest';
import { bucketLevels } from './waveformBars';

describe('bucketLevels', () => {
  it('returns the requested number of bars', () => {
    expect(bucketLevels(new Array(32).fill(0), 5)).toHaveLength(5);
  });

  it('averages a uniform input to the same value', () => {
    expect(bucketLevels(new Array(32).fill(0.5), 5)).toEqual([0.5, 0.5, 0.5, 0.5, 0.5]);
  });

  it('averages each contiguous bucket', () => {
    // 10 values → 5 buckets of 2: [0,0][0,0][0,1][1,1][1,1]
    const levels = Array.from({ length: 10 }, (_, i) => (i < 5 ? 0 : 1));
    expect(bucketLevels(levels, 5)).toEqual([0, 0, 0.5, 1, 1]);
  });

  it('returns zeros for empty input', () => {
    expect(bucketLevels([], 5)).toEqual([0, 0, 0, 0, 0]);
  });
});
