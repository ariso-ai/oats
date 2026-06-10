// Pure step-sequence logic for the first-run onboarding window. Extracted from
// OnboardingView so the flow control is unit-testable without a DOM, mirroring
// the settingsDownload.ts pattern.

export type OnboardingStep = 'signin';

/** Ordered list of onboarding steps. Append here to add future steps. */
export const ONBOARDING_STEPS: readonly OnboardingStep[] = ['signin'];

/**
 * Given the current step index, return the next index, or null if the flow is
 * complete (the current step is the last one).
 */
export function nextStepIndex(steps: readonly unknown[], current: number): number | null {
  return current + 1 < steps.length ? current + 1 : null;
}
