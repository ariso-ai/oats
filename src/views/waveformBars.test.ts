import { describe, it, expect } from 'vitest';
import { bucketLevels, centerWeightedBars } from './waveformBars';

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

describe('centerWeightedBars', () => {
  it('puts the lowest-frequency (most energetic) bucket in the center', () => {
    // 9 bins → 3 buckets averaging [1, 0.5, 0.25]; the hot low bucket lands
    // on the middle bar, and each side bar is a 50/50 blend of its own
    // bucket with the center: [0.75, 1, 0.625].
    const levels = [1, 1, 1, 0.5, 0.5, 0.5, 0.25, 0.25, 0.25];
    expect(centerWeightedBars(levels, 3)).toEqual([0.75, 1, 0.625]);
  });

  it('side bars follow the center even when their own buckets are silent', () => {
    // Speech-like input: all energy in the low bucket. The side bars must
    // still move (at half the center's level), not sit dead.
    const levels = [0.75, 0.75, 0.75, 0, 0, 0, 0, 0, 0];
    expect(centerWeightedBars(levels, 3)).toEqual([0.375, 0.75, 0.375]);
  });

  it('returns the requested number of bars', () => {
    expect(centerWeightedBars(new Array(32).fill(0.5), 3)).toHaveLength(3);
  });

  it('returns zeros for empty input', () => {
    expect(centerWeightedBars([], 3)).toEqual([0, 0, 0]);
  });
});
