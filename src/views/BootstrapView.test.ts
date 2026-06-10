// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

const isOnboarded = vi.fn();
const openOnboardingWindow = vi.fn();
const checkSession = vi.fn();
const setOnboarded = vi.fn();

vi.mock('../tauri', () => ({
  auth: { checkSession: () => checkSession() },
  isOnboarded: () => isOnboarded(),
  openOnboardingWindow: () => openOnboardingWindow(),
  setOnboarded: (v: boolean) => setOnboarded(v),
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
  checkSession.mockResolvedValue(null);
  openOnboardingWindow.mockResolvedValue(undefined);
  setOnboarded.mockResolvedValue(undefined);
});

describe('BootstrapView first-run trigger', () => {
  it('opens the onboarding window when not yet onboarded', async () => {
    isOnboarded.mockResolvedValue(false);
    mount(BootstrapView);
    await flushPromises();
    expect(openOnboardingWindow).toHaveBeenCalledTimes(1);
  });

  it('marks upgraded signed-in profiles onboarded without opening onboarding', async () => {
    isOnboarded.mockResolvedValue(false);
    checkSession.mockResolvedValue({ sessionToken: 'existing' });
    mount(BootstrapView);
    await flushPromises();
    expect(setOnboarded).toHaveBeenCalledWith(true);
    expect(openOnboardingWindow).not.toHaveBeenCalled();
  });

  it('does not open the onboarding window once onboarded', async () => {
    isOnboarded.mockResolvedValue(true);
    mount(BootstrapView);
    await flushPromises();
    expect(openOnboardingWindow).not.toHaveBeenCalled();
  });
});
