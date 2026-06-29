import { describe, it, expect, vi, beforeEach } from 'vitest';

const storeGet = vi.fn();
const storeSet = vi.fn(() => Promise.resolve());

vi.mock('@tauri-apps/plugin-store', () => ({
  load: () => Promise.resolve({ get: storeGet, set: storeSet }),
}));

import {
  isMeetingEndReminderEnabled,
  setMeetingEndReminderEnabled,
} from './useMeetingEndReminder';

beforeEach(() => vi.clearAllMocks());

describe('useMeetingEndReminder', () => {
  it('defaults to enabled when unset', async () => {
    storeGet.mockResolvedValue(undefined);
    expect(await isMeetingEndReminderEnabled()).toBe(true);
  });

  it('stays enabled when explicitly true', async () => {
    storeGet.mockResolvedValue(true);
    expect(await isMeetingEndReminderEnabled()).toBe(true);
  });

  it('is disabled only when explicitly false', async () => {
    storeGet.mockResolvedValue(false);
    expect(await isMeetingEndReminderEnabled()).toBe(false);
  });

  it('persists the flag on set', async () => {
    await setMeetingEndReminderEnabled(false);
    expect(storeSet).toHaveBeenCalledWith('meetingEndReminderEnabled', false);
  });
});
