import { describe, it, expect } from 'vitest';
import { useRecordingStartChoice } from './useRecordingStartChoice';

describe('useRecordingStartChoice', () => {
  it('opens on request and resolves to the chosen kind', async () => {
    const c = useRecordingStartChoice();
    const p = c.requestChoice('Standup');
    expect(c.open.value).toBe(true);
    expect(c.meetingTitle.value).toBe('Standup');
    c.choose('continue');
    await expect(p).resolves.toBe('continue');
    expect(c.open.value).toBe(false);
  });

  it('resolves to "new" and to null on cancel', async () => {
    const c = useRecordingStartChoice();
    const p1 = c.requestChoice('A');
    c.choose('new');
    await expect(p1).resolves.toBe('new');
    const p2 = c.requestChoice('B');
    c.cancel();
    await expect(p2).resolves.toBeNull();
  });

  it('supersedes an in-flight request with null', async () => {
    const c = useRecordingStartChoice();
    const p1 = c.requestChoice('A');
    const p2 = c.requestChoice('B');
    await expect(p1).resolves.toBeNull();
    c.choose('continue');
    await expect(p2).resolves.toBe('continue');
  });
});
