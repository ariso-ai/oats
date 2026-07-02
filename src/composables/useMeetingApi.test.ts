import { describe, it, expect, vi, beforeEach } from 'vitest';

const apiRequest = vi.fn();

vi.mock('../tauri', () => ({
  api: {
    request: (...a: unknown[]) => apiRequest(...a),
    putPresigned: vi.fn(),
  },
}));

import { useMeetingApi } from './useMeetingApi';

beforeEach(() => {
  apiRequest.mockReset();
});

describe('useMeetingApi.searchMeetings', () => {
  it('calls the shared meetings endpoint with q and limit, then sorts newest first', async () => {
    apiRequest.mockResolvedValue({
      status: 200,
      data: {
        meetings: [
          { id: 1, title: 'Older', start_at: '2026-06-01T09:00:00Z' },
          { id: 2, title: 'Newer', start_at: '2026-06-02T09:00:00Z', snippet: 'note hit' },
        ],
      },
    });

    const results = await useMeetingApi().searchMeetings(' note ', 7);

    expect(apiRequest).toHaveBeenCalledWith('GET', '/meetings?q=note&limit=7');
    expect(results.map((m) => m.id)).toEqual([2, 1]);
    expect(results[0].snippet).toBe('note hit');
  });

  it('does not call the backend for a blank query', async () => {
    await expect(useMeetingApi().searchMeetings('   ')).resolves.toEqual([]);
    expect(apiRequest).not.toHaveBeenCalled();
  });
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

describe('multi-clip', () => {
  it('deleteMeetingRecordingClip DELETEs the per-clip recording route', async () => {
    apiRequest.mockResolvedValue({ status: 200, data: {} });
    await useMeetingApi().deleteMeetingRecordingClip(42, 'clip-uuid');
    expect(apiRequest).toHaveBeenCalledWith(
      'DELETE',
      '/meeting-notes/42/recording/clip-uuid'
    );
  });

  it('getMeetingTranscript keeps transcript_id per chunk (legacy default)', async () => {
    apiRequest.mockResolvedValue({
      status: 200,
      data: {
        transcript: [
          { chunk_index: 0, start_ms: 0, content: 'a', transcript_id: 'clip-1' },
          { chunk_index: 1, start_ms: 10, content: 'b' },
        ],
      },
    });
    const chunks = await useMeetingApi().getMeetingTranscript(7);
    expect(chunks).toEqual([
      { chunk_index: 0, start_ms: 0, content: 'a', transcript_id: 'clip-1' },
      { chunk_index: 1, start_ms: 10, content: 'b', transcript_id: 'legacy' },
    ]);
  });
});
