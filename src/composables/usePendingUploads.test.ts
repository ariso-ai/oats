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

import { mergedMeta, groupByMeetingId, combineAndUpload, discardAll } from './usePendingUploads';

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
  it('preserves the meetingId when the group carries one', () => {
    expect(mergedMeta([{ ...items[0], meetingId: 7 }])).toEqual({
      startAt: '2026-06-12T09:00:00Z',
      endAt: '2026-06-12T09:05:00Z',
      durationSeconds: 300,
      meetingId: 7,
    });
  });
  it('omits meetingId when no item in the group has one', () => {
    expect(mergedMeta(items)).not.toHaveProperty('meetingId');
  });
});

describe('groupByMeetingId', () => {
  it('keeps items with no meetingId together as one group', () => {
    expect(groupByMeetingId(items)).toEqual([items]);
  });

  it('splits items into one group per distinct meetingId, preserving order', () => {
    const a = { ...items[0], createdAt: 'a', meetingId: 5 };
    const b = { ...items[0], createdAt: 'b', meetingId: 5 };
    const c = { ...items[0], createdAt: 'c' };
    const d = { ...items[0], createdAt: 'd', meetingId: 9 };

    expect(groupByMeetingId([a, b, c, d])).toEqual([[a, b], [c], [d]]);
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

  it('uploads each meetingId group to its own meeting, preserving the id', async () => {
    const a = { ...items[0], createdAt: 'a', endAt: 'a-end', durationSeconds: 300, meetingId: 5 };
    const b = { ...items[0], createdAt: 'b', endAt: 'b-end', durationSeconds: 120, meetingId: 5 };
    const c = { ...items[0], createdAt: 'c', endAt: 'c-end', durationSeconds: 60, meetingId: undefined };
    const d = { ...items[0], createdAt: 'd', endAt: 'd-end', durationSeconds: 90, meetingId: 9 };

    combine.mockResolvedValue(new ArrayBuffer(4));
    uploadAudio.mockResolvedValue({ meetingId: 1 });
    discardAudio.mockResolvedValue(undefined);

    await combineAndUpload([a, b, c, d]);

    // One combine + upload per group.
    expect(combine.mock.calls).toEqual([[['a', 'b']], [['c']], [['d']]]);

    const metas = uploadAudio.mock.calls.map((call) => call[1]);
    expect(metas).toContainEqual(expect.objectContaining({ meetingId: 5, durationSeconds: 420 }));
    expect(metas).toContainEqual(expect.objectContaining({ meetingId: 9, durationSeconds: 90 }));
    // The id-less group creates a fresh meeting.
    const fresh = metas.find((m) => m.meetingId === undefined);
    expect(fresh).toBeDefined();
    expect(fresh).not.toHaveProperty('meetingId');

    // Every buffered key is discarded after its group uploads.
    for (const key of ['a', 'b', 'c', 'd']) {
      expect(discardAudio).toHaveBeenCalledWith(key);
    }
  });

  it('leaves a failed group buffered while still uploading and discarding the others', async () => {
    const a = { ...items[0], createdAt: 'a', durationSeconds: 300, meetingId: 5 };
    const d = { ...items[0], createdAt: 'd', durationSeconds: 90, meetingId: 9 };

    combine.mockResolvedValue(new ArrayBuffer(4));
    // Group with meetingId 9 fails to upload; meetingId 5 succeeds.
    uploadAudio.mockImplementation((_blob: unknown, meta: { meetingId?: number }) =>
      meta.meetingId === 9 ? Promise.reject(new Error('offline')) : Promise.resolve({ meetingId: 5 })
    );
    discardAudio.mockResolvedValue(undefined);

    await expect(combineAndUpload([a, d])).rejects.toThrow('offline');

    // The succeeded group is discarded; the failed group is left in place.
    expect(discardAudio).toHaveBeenCalledWith('a');
    expect(discardAudio).not.toHaveBeenCalledWith('d');
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
