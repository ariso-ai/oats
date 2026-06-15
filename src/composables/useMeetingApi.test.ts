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
  vi.clearAllMocks();
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
