import { describe, it, expect } from 'vitest';
import { ONBOARDING_STEPS, nextStepIndex } from './onboarding';

describe('ONBOARDING_STEPS', () => {
  it('starts with the sign-in step', () => {
    expect(ONBOARDING_STEPS[0]).toBe('signin');
  });
});

describe('nextStepIndex', () => {
  it('returns null when on the last step (single-step flow finishes)', () => {
    expect(nextStepIndex(['signin'], 0)).toBeNull();
  });
  it('advances to the next index when more steps remain', () => {
    expect(nextStepIndex(['signin', 'permissions'], 0)).toBe(1);
  });
  it('returns null when on the last step of a multi-step flow', () => {
    expect(nextStepIndex(['signin', 'permissions'], 1)).toBeNull();
  });
});
