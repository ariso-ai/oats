import { describe, it, expect, vi, beforeEach } from 'vitest';

const apiRequest = vi.fn();
vi.mock('../tauri', () => ({
  api: { request: (...a: unknown[]) => apiRequest(...a) },
}));

import { useMeetingApi } from './useMeetingApi';

beforeEach(() => {
  apiRequest.mockReset();
});

describe('share methods', () => {
  it('shareMeeting posts visibility + expiry and maps the response', async () => {
    apiRequest.mockResolvedValue({
      status: 200,
      data: { shareUrl: 'https://x/y', shortCode: 'abc', publicShareExpiresAt: '2026-07-01T00:00:00Z' },
    });
    const r = await useMeetingApi().shareMeeting('5', 'public', 30);
    expect(apiRequest).toHaveBeenCalledWith('POST', '/meeting-notes/5/share', {
      visibility: 'public',
      expiresInDays: 30,
    });
    expect(r).toEqual({ shareUrl: 'https://x/y', shortCode: 'abc', publicShareExpiresAt: '2026-07-01T00:00:00Z' });
  });

  it('shareMeeting omits expiry for non-public', async () => {
    apiRequest.mockResolvedValue({ status: 200, data: { shareUrl: 'u' } });
    await useMeetingApi().shareMeeting('5', 'workspace', 30);
    expect(apiRequest).toHaveBeenCalledWith('POST', '/meeting-notes/5/share', { visibility: 'workspace' });
  });

  it('listShareEmails returns emails and collapses errors to []', async () => {
    apiRequest.mockResolvedValue({ status: 200, data: { items: [{ email: 'a@x.com' }, { email: '' }, {}] } });
    expect(await useMeetingApi().listShareEmails('5')).toEqual(['a@x.com']);
    apiRequest.mockRejectedValue(new Error('boom'));
    expect(await useMeetingApi().listShareEmails('5')).toEqual([]);
  });

  it('sendShareEmail returns alreadyShared and throws server error', async () => {
    apiRequest.mockResolvedValue({ status: 200, data: { already_shared: true } });
    expect(await useMeetingApi().sendShareEmail('5', 'a@x.com')).toEqual({ alreadyShared: true });
    apiRequest.mockResolvedValue({ status: 400, data: { error: 'bad email' } });
    await expect(useMeetingApi().sendShareEmail('5', 'x')).rejects.toThrow('bad email');
  });

  it('unshareEmail encodes the email in the query string', async () => {
    apiRequest.mockResolvedValue({ status: 200, data: {} });
    await useMeetingApi().unshareEmail('5', 'a+b@x.com');
    expect(apiRequest).toHaveBeenCalledWith('DELETE', '/meeting-notes/5/share-email?email=a%2Bb%40x.com');
  });
});
