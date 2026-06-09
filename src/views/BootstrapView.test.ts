// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

const isOnboarded = vi.fn();
const openOnboardingWindow = vi.fn();

vi.mock('../tauri', () => ({
  isOnboarded: () => isOnboarded(),
  openOnboardingWindow: () => openOnboardingWindow(),
}));
// listen resolves to an unlisten fn; invoke is a no-op for these tests.
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => Promise.resolve()),
}));
vi.mock('../composables/useMeetingNotifications', () => ({
  SYNC_EVENT: 'meeting-notifications-sync',
}));

import BootstrapView from './BootstrapView.vue';

beforeEach(() => {
  vi.clearAllMocks();
  openOnboardingWindow.mockResolvedValue(undefined);
});

describe('BootstrapView first-run trigger', () => {
  it('opens the onboarding window when not yet onboarded', async () => {
    isOnboarded.mockResolvedValue(false);
    mount(BootstrapView);
    await flushPromises();
    expect(openOnboardingWindow).toHaveBeenCalledTimes(1);
  });

  it('does not open the onboarding window once onboarded', async () => {
    isOnboarded.mockResolvedValue(true);
    mount(BootstrapView);
    await flushPromises();
    expect(openOnboardingWindow).not.toHaveBeenCalled();
  });
});
