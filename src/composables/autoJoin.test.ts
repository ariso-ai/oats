import { describe, it, expect } from 'vitest';
import { arisoTruthy, shouldConfirmAriJoin } from './autoJoin';

describe('arisoTruthy', () => {
  it('is true for boolean true, non-zero numbers, and "true"/"1"', () => {
    expect(arisoTruthy(true)).toBe(true);
    expect(arisoTruthy(1)).toBe(true);
    expect(arisoTruthy(2)).toBe(true);
    expect(arisoTruthy('true')).toBe(true);
    expect(arisoTruthy('1')).toBe(true);
  });

  it('is false for false, 0, other strings, null, and undefined', () => {
    expect(arisoTruthy(false)).toBe(false);
    expect(arisoTruthy(0)).toBe(false);
    expect(arisoTruthy('false')).toBe(false);
    expect(arisoTruthy('0')).toBe(false);
    expect(arisoTruthy('yes')).toBe(false);
    expect(arisoTruthy(null)).toBe(false);
    expect(arisoTruthy(undefined)).toBe(false);
  });
});

describe('shouldConfirmAriJoin', () => {
  it('is true only for ariso backend with a truthy flag', () => {
    expect(shouldConfirmAriJoin('ariso', true)).toBe(true);
  });

  it('is false for ariso without the flag', () => {
    expect(shouldConfirmAriJoin('ariso', false)).toBe(false);
    expect(shouldConfirmAriJoin('ariso', undefined)).toBe(false);
  });

  it('is false for the local backend even when flagged', () => {
    expect(shouldConfirmAriJoin('local', true)).toBe(false);
  });
});
