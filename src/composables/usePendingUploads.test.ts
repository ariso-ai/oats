import { describe, it, expect, vi, beforeEach } from 'vitest';

const list = vi.fn();
const combine = vi.fn();
const discardAudio = vi.fn();
const uploadAudio = vi.fn();

vi.mock('../tauri', () => ({
  pending: {
    list: (...a: unknown[]) => list(...a),
    combine: (...a: unknown[]) => combine(...a),
    discardAudio: (...a: unknown[]) => discardAudio(...a),
  },
}));
vi.mock('./useMeetingApi', () => ({
  useMeetingApi: () => ({ uploadAudio: (...a: unknown[]) => uploadAudio(...a) }),
}));

import { mergedMeta, combineAndUpload, discardAll } from './usePendingUploads';

const items = [
  { createdAt: '2026-06-12T09:00:00Z', startAt: '2026-06-12T09:00:00Z', endAt: '2026-06-12T09:05:00Z', durationSeconds: 300 },
  { createdAt: '2026-06-12T11:00:00Z', startAt: null, endAt: '2026-06-12T11:02:00Z', durationSeconds: 120 },
];

beforeEach(() => vi.clearAllMocks());

describe('mergedMeta', () => {
  it('takes earliest start, latest end, summed duration', () => {
    expect(mergedMeta(items)).toEqual({
      startAt: '2026-06-12T09:00:00Z',
      endAt: '2026-06-12T11:02:00Z',
      durationSeconds: 420,
    });
  });
  it('falls back to createdAt when the first item has no startAt', () => {
    expect(mergedMeta([items[1]]).startAt).toBe('2026-06-12T11:00:00Z');
  });
});

describe('combineAndUpload', () => {
  it('combines keys, uploads merged meta, then discards each key', async () => {
    combine.mockResolvedValue(new ArrayBuffer(4));
    uploadAudio.mockResolvedValue({ meetingId: 1 });
    discardAudio.mockResolvedValue(undefined);

    await combineAndUpload(items);

    expect(combine).toHaveBeenCalledWith(['2026-06-12T09:00:00Z', '2026-06-12T11:00:00Z']);
    const [blobArg, metaArg] = uploadAudio.mock.calls[0];
    expect(blobArg).toBeInstanceOf(Blob);
    expect(metaArg).toEqual({
      startAt: '2026-06-12T09:00:00Z',
      endAt: '2026-06-12T11:02:00Z',
      durationSeconds: 420,
    });
    expect(discardAudio).toHaveBeenCalledWith('2026-06-12T09:00:00Z');
    expect(discardAudio).toHaveBeenCalledWith('2026-06-12T11:00:00Z');
  });

  it('does not discard when the upload fails', async () => {
    combine.mockResolvedValue(new ArrayBuffer(4));
    uploadAudio.mockRejectedValue(new Error('offline'));
    await expect(combineAndUpload(items)).rejects.toThrow('offline');
    expect(discardAudio).not.toHaveBeenCalled();
  });

  it('is a no-op for an empty list', async () => {
    await combineAndUpload([]);
    expect(combine).not.toHaveBeenCalled();
    expect(uploadAudio).not.toHaveBeenCalled();
  });
});

describe('discardAll', () => {
  it('discards every item by key', async () => {
    discardAudio.mockResolvedValue(undefined);
    await discardAll(items);
    expect(discardAudio).toHaveBeenCalledTimes(2);
    expect(discardAudio).toHaveBeenCalledWith('2026-06-12T09:00:00Z');
    expect(discardAudio).toHaveBeenCalledWith('2026-06-12T11:00:00Z');
  });
});
