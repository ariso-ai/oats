import { describe, it, expect, vi, beforeEach } from 'vitest';

const list = vi.fn();
const combine = vi.fn();
const discardAudio = vi.fn();
const uploadAudio = vi.fn();
const reportUploadFailure = vi.fn(() => Promise.resolve());

vi.mock('./useDiagnostics', () => ({
  reportUploadFailure: (...a: unknown[]) => reportUploadFailure(...a),
}));
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

  it('reports an upload failure as a retry, with the batch size', async () => {
    combine.mockResolvedValue(new ArrayBuffer(4));
    uploadAudio.mockRejectedValue(new Error('offline'));
    await expect(combineAndUpload(items)).rejects.toThrow('offline');
    expect(reportUploadFailure).toHaveBeenCalledWith(
      expect.any(Error),
      expect.objectContaining({ attempt: 'retry', itemCount: 2, durationSeconds: 420 })
    );
  });

  it('reports a failing combine under the combine stage and never uploads', async () => {
    combine.mockRejectedValue(new Error('buffer missing'));
    await expect(combineAndUpload(items)).rejects.toThrow('buffer missing');
    expect(uploadAudio).not.toHaveBeenCalled();
    expect(reportUploadFailure).toHaveBeenCalledWith(
      expect.any(Error),
      expect.objectContaining({ attempt: 'retry', stage: 'combine', itemCount: 2 })
    );
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
