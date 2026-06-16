import { describe, it, expect, vi, beforeEach } from 'vitest';

const localFinalize = vi.fn();
const listRecordings = vi.fn();
const listMeetingsInWindow = vi.fn();
const searchMeetings = vi.fn();
const apiRequest = vi.fn();
const putPresigned = vi.fn();
const checkSession = vi.fn();
const modelStatus = vi.fn();
const getBackendSetting = vi.fn();
const uploadAudio = vi.fn();
const renameRecording = vi.fn();
const updateMeetingNotesTitle = vi.fn();
const bufferPendingAudio = vi.fn();
const discardPendingAudio = vi.fn();
const fetchMeetingAudio = vi.fn();
const readRecordingAudio = vi.fn();
const getMeetingNotes = vi.fn();

vi.mock('../tauri', () => ({
  local: {
    finalizeRecording: (...a: unknown[]) => localFinalize(...a),
    modelStatus: () => modelStatus(),
    listRecordings: () => listRecordings(),
    renameRecording: (...a: unknown[]) => renameRecording(...a),
    readRecordingAudio: (...a: unknown[]) => readRecordingAudio(...a),
  },
  auth: { checkSession: () => checkSession() },
  api: {
    request: (...a: unknown[]) => apiRequest(...a),
    putPresigned: (...a: unknown[]) => putPresigned(...a),
    fetchMeetingAudio: (...a: unknown[]) => fetchMeetingAudio(...a),
  },
  pending: {
    bufferAudio: (...a: unknown[]) => bufferPendingAudio(...a),
    discardAudio: (...a: unknown[]) => discardPendingAudio(...a),
  },
  getBackendSetting: () => getBackendSetting(),
}));

vi.mock('./useMeetingApi', () => ({
  useMeetingApi: () => ({
    uploadAudio: (...a: unknown[]) => uploadAudio(...a),
    listMeetingsInWindow: (...a: unknown[]) => listMeetingsInWindow(...a),
    searchMeetings: (...a: unknown[]) => searchMeetings(...a),
    updateMeetingNotesTitle: (...a: unknown[]) => updateMeetingNotesTitle(...a),
    getMeetingNotes: (...a: unknown[]) => getMeetingNotes(...a),
  }),
}));

import { ArisoBackend, LocalBackend, getActiveBackend, arisoMeetingWindow } from './useBackend';

beforeEach(() => {
  vi.clearAllMocks();
  getMeetingNotes.mockReset();
});

describe('LocalBackend', () => {
  it('declares no auth and no picker', () => {
    const b = new LocalBackend();
    expect(b.id).toBe('local');
    expect(b.needsAuth).toBe(false);
    expect(b.usesMeetingPicker).toBe(false);
    expect(b.supportsSearch).toBe(false);
  });

  it('isReady reflects model status', async () => {
    modelStatus.mockResolvedValue({ state: 'not_downloaded' });
    expect(await new LocalBackend().isReady()).toEqual({ ready: false, reason: 'model-missing' });
    modelStatus.mockResolvedValue({ state: 'unsupported' });
    expect(await new LocalBackend().isReady()).toEqual({ ready: false, reason: 'unsupported-platform' });
    modelStatus.mockResolvedValue({ state: 'ready', version: 'v3' });
    expect(await new LocalBackend().isReady()).toEqual({ ready: true });
  });

  it('finalizeRecording forwards bytes + derived title to the command', async () => {
    localFinalize.mockResolvedValue({ backend: 'local', id: 'X', title: 'T', status: 'done' });
    const blob = new Blob([new Uint8Array([1, 2, 3])], { type: 'audio/mpeg' });
    const res = await new LocalBackend().finalizeRecording(blob, {
      startAt: '2026-06-02T14:30:05.000Z',
      endAt: '2026-06-02T15:10:00.000Z',
      durationSeconds: 2400,
    });
    expect(res.backend).toBe('local');
    const [audioArg, titleArg, createdAtArg, durationArg] = localFinalize.mock.calls[0];
    expect(audioArg).toEqual([1, 2, 3]);
    expect(createdAtArg).toBe('2026-06-02T14:30:05.000Z');
    expect(durationArg).toBe(2400);
    // Title is a consistent local "YYYY-MM-DD HH:MM" (assert format, not a
    // timezone-specific value).
    expect(titleArg).toMatch(/^Recording \d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
  });

  it('renameMeeting forwards to the rename_local_recording bridge', async () => {
    renameRecording.mockResolvedValue(undefined);
    await new LocalBackend().renameMeeting('2026-06-02T14-30-05Z', 'New title');
    expect(renameRecording).toHaveBeenCalledWith('2026-06-02T14-30-05Z', 'New title');
  });

  it('does not fake local search results', async () => {
    await expect(new LocalBackend().searchMeetings('standup')).resolves.toEqual([]);
  });

  it('getMeetingAudio reads recording.mp3 when present, null otherwise', async () => {
    const b = new LocalBackend();
    const buf = new ArrayBuffer(4);
    readRecordingAudio.mockResolvedValue(buf);
    const withAudio = {
      id: 'a', title: 'T', timestamp: 't',
      files: { hasAudio: true, hasNote: false, hasTranscript: false },
    };
    expect(await b.getMeetingAudio(withAudio)).toBe(buf);
    expect(readRecordingAudio).toHaveBeenCalledWith('a');

    const withoutAudio = { ...withAudio, files: { ...withAudio.files, hasAudio: false } };
    expect(await b.getMeetingAudio(withoutAudio)).toBeNull();
    const noFiles = { id: 'b', title: 'T', timestamp: 't' };
    expect(await b.getMeetingAudio(noFiles)).toBeNull();
  });
});

describe('ArisoBackend', () => {
  it('declares auth + picker', () => {
    const b = new ArisoBackend();
    expect(b.id).toBe('ariso');
    expect(b.needsAuth).toBe(true);
    expect(b.usesMeetingPicker).toBe(true);
    expect(b.supportsSearch).toBe(true);
  });

  it('isReady reflects session', async () => {
    const b = new ArisoBackend();
    checkSession.mockResolvedValue({ sessionToken: 'tok' });
    expect(await b.isReady()).toEqual({ ready: true });
    checkSession.mockResolvedValue(null);
    expect(await b.isReady()).toEqual({ ready: false, reason: 'signed-out' });
  });

  it('finalizeRecording uploads via useMeetingApi and returns the meetingId', async () => {
    bufferPendingAudio.mockResolvedValue('2026-06-02T14-30-05Z');
    discardPendingAudio.mockResolvedValue(undefined);
    uploadAudio.mockResolvedValue({ meetingId: 7 });
    const blob = new Blob([new Uint8Array([9])], { type: 'audio/mpeg' });
    const res = await new ArisoBackend().finalizeRecording(blob, {
      startAt: '2026-06-02T14:30:05.000Z',
      endAt: '2026-06-02T15:10:00.000Z',
      durationSeconds: 2400,
      meetingId: 7,
    });
    expect(res).toEqual({ backend: 'ariso', meetingId: 7 });
    expect(uploadAudio).toHaveBeenCalledWith(blob, {
      startAt: '2026-06-02T14:30:05.000Z',
      endAt: '2026-06-02T15:10:00.000Z',
      meetingId: 7,
    });
  });

  it('renameMeeting forwards to the meeting-notes PATCH endpoint', async () => {
    updateMeetingNotesTitle.mockResolvedValue(undefined);
    await new ArisoBackend().renameMeeting('7', 'New title');
    expect(updateMeetingNotesTitle).toHaveBeenCalledWith('7', 'New title');
  });

  it('finalizeRecording buffers the audio before upload and discards after success', async () => {
    bufferPendingAudio.mockResolvedValue('2026-06-02T14-30-05Z');
    discardPendingAudio.mockResolvedValue(undefined);
    uploadAudio.mockResolvedValue({ meetingId: 7 });
    const blob = new Blob([new Uint8Array([9])], { type: 'audio/mpeg' });
    await new ArisoBackend().finalizeRecording(blob, {
      startAt: '2026-06-02T14:30:05.000Z',
      endAt: '2026-06-02T15:10:00.000Z',
      durationSeconds: 2400,
    });
    expect(bufferPendingAudio).toHaveBeenCalledWith([9], {
      createdAt: '2026-06-02T14:30:05.000Z',
      startAt: '2026-06-02T14:30:05.000Z',
      endAt: '2026-06-02T15:10:00.000Z',
      durationSeconds: 2400,
      meetingId: undefined,
    });
    expect(bufferPendingAudio.mock.invocationCallOrder[0]).toBeLessThan(
      uploadAudio.mock.invocationCallOrder[0]
    );
    expect(discardPendingAudio).toHaveBeenCalledWith('2026-06-02T14:30:05.000Z');
    expect(uploadAudio.mock.invocationCallOrder[0]).toBeLessThan(
      discardPendingAudio.mock.invocationCallOrder[0]
    );
  });

  it('finalizeRecording keys the buffer by endAt when startAt is null', async () => {
    bufferPendingAudio.mockResolvedValue('id');
    discardPendingAudio.mockResolvedValue(undefined);
    uploadAudio.mockResolvedValue({ meetingId: 7 });
    await new ArisoBackend().finalizeRecording(new Blob(['x']), {
      startAt: null,
      endAt: '2026-06-02T15:10:00.000Z',
      durationSeconds: 10,
    });
    expect(bufferPendingAudio.mock.calls[0][1]).toMatchObject({
      createdAt: '2026-06-02T15:10:00.000Z',
      startAt: null,
      endAt: '2026-06-02T15:10:00.000Z',
    });
    expect(discardPendingAudio).toHaveBeenCalledWith('2026-06-02T15:10:00.000Z');
  });

  it('finalizeRecording leaves the buffer in place when the upload fails', async () => {
    bufferPendingAudio.mockResolvedValue('id');
    uploadAudio.mockRejectedValue(new Error('S3 upload failed (500)'));
    await expect(
      new ArisoBackend().finalizeRecording(new Blob(['x']), {
        startAt: '2026-06-02T14:30:05.000Z',
        endAt: '2026-06-02T15:10:00.000Z',
        durationSeconds: 10,
      })
    ).rejects.toThrow('S3 upload failed');
    expect(bufferPendingAudio).toHaveBeenCalled();
    expect(discardPendingAudio).not.toHaveBeenCalled();
  });

  it('finalizeRecording still uploads when buffering itself fails', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    bufferPendingAudio.mockRejectedValue(new Error('disk full'));
    uploadAudio.mockResolvedValue({ meetingId: 7 });
    discardPendingAudio.mockResolvedValue(undefined);
    const res = await new ArisoBackend().finalizeRecording(new Blob(['x']), {
      startAt: '2026-06-02T14:30:05.000Z',
      endAt: '2026-06-02T15:10:00.000Z',
      durationSeconds: 10,
    });
    expect(res).toEqual({ backend: 'ariso', meetingId: 7 });
    expect(uploadAudio).toHaveBeenCalled();
  });

  it('finalizeRecording succeeds even when the post-upload discard fails', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    bufferPendingAudio.mockResolvedValue('id');
    uploadAudio.mockResolvedValue({ meetingId: 7 });
    discardPendingAudio.mockRejectedValue(new Error('locked'));
    const res = await new ArisoBackend().finalizeRecording(new Blob(['x']), {
      startAt: '2026-06-02T14:30:05.000Z',
      endAt: '2026-06-02T15:10:00.000Z',
      durationSeconds: 10,
    });
    expect(res).toEqual({ backend: 'ariso', meetingId: 7 });
  });

  it('getMeetingAudio returns bytes, maps 404 to null, and rethrows other errors', async () => {
    const b = new ArisoBackend();
    const item = { id: '7', title: 'T', timestamp: '2026-06-02T10:00:00Z' };
    const buf = new ArrayBuffer(4);
    fetchMeetingAudio.mockResolvedValue(buf);
    expect(await b.getMeetingAudio(item)).toBe(buf);
    expect(fetchMeetingAudio).toHaveBeenCalledWith('7');

    fetchMeetingAudio.mockRejectedValue('404: audio fetch failed');
    expect(await b.getMeetingAudio(item)).toBeNull();

    fetchMeetingAudio.mockRejectedValue('500: audio fetch failed');
    await expect(b.getMeetingAudio(item)).rejects.toBeTruthy();
  });

  it('maps share-gating fields and participant ids from getMeetingNotes', async () => {
    getMeetingNotes.mockResolvedValue({
      id: 7,
      title: 'Sync',
      start_at: '2026-06-01T10:00:00Z',
      visibility: 'workspace',
      short_code: 'abc123',
      public_share_expires_at: '2026-07-01T10:00:00Z',
      shareMeetingNotesToPublic: 'host_only',
      participants: [
        { id: 11, name: 'Ana', email: 'ana@x.com', role: 'host', self: true, avatar_url: 'u' },
      ],
      summary: '{}',
    });
    const backend = new ArisoBackend();
    const d = await backend.getMeetingDetail({ id: '7', title: 'Sync', timestamp: '2026-06-01T10:00:00Z' });
    expect(d.shortCode).toBe('abc123');
    expect(d.publicShareExpiresAt).toBe('2026-07-01T10:00:00Z');
    expect(d.shareMeetingNotesToPublic).toBe('host_only');
    expect(d.participants[0].id).toBe(11);
  });
});

describe('getActiveBackend', () => {
  it('returns LocalBackend when setting is local, else ArisoBackend', async () => {
    getBackendSetting.mockResolvedValue('local');
    expect((await getActiveBackend()).id).toBe('local');
    getBackendSetting.mockResolvedValue('ariso');
    expect((await getActiveBackend()).id).toBe('ariso');
  });

  it('defaults to Ariso when the setting read throws', async () => {
    getBackendSetting.mockRejectedValue(new Error('store unavailable'));
    expect((await getActiveBackend()).id).toBe('ariso');
  });
});

describe('arisoMeetingWindow', () => {
  it('spans 7 days back to 7 days forward, date-only', () => {
    // Local midday avoids any tz boundary ambiguity.
    const now = new Date(2026, 5, 9, 12, 0, 0); // 2026-06-09 local
    expect(arisoMeetingWindow(now)).toEqual({
      startDate: '2026-06-02',
      endDate: '2026-06-16',
    });
  });

  it('rolls back across a month/year boundary', () => {
    const now = new Date(2026, 0, 3, 12, 0, 0); // 2026-01-03 local
    expect(arisoMeetingWindow(now)).toEqual({
      startDate: '2025-12-27',
      endDate: '2026-01-10',
    });
  });
});

describe('LocalBackend.listMeetings', () => {
  it('maps recordings to list items with file affordances', async () => {
    listRecordings.mockResolvedValue([
      {
        id: 'a',
        title: 'First',
        createdAt: '2026-06-02T10:00:00Z',
        durationSeconds: 75,
        status: 'done',
        hasAudio: true,
        hasNote: false,
        hasTranscript: true,
      },
    ]);
    const items = await new LocalBackend().listMeetings();
    expect(items).toEqual([
      {
        id: 'a',
        title: 'First',
        timestamp: '2026-06-02T10:00:00Z',
        durationSeconds: 75,
        status: 'done',
        files: { hasAudio: true, hasNote: false, hasTranscript: true },
      },
    ]);
  });
});

describe('ArisoBackend.listMeetings', () => {
  it('queries the date window and maps meetings (no file affordances)', async () => {
    listMeetingsInWindow.mockResolvedValue([
      { id: 7, title: 'Standup', start_at: '2026-06-08T09:00:00Z' },
      { id: 8, title: null, start_at: '2026-06-09T09:00:00Z' },
    ]);
    const items = await new ArisoBackend().listMeetings();
    expect(listMeetingsInWindow).toHaveBeenCalledTimes(1);
    expect(listMeetingsInWindow).toHaveBeenCalledWith(
      expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/),
      expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/)
    );
    expect(items).toEqual([
      { id: '7', title: 'Standup', timestamp: '2026-06-08T09:00:00Z' },
      { id: '8', title: 'Untitled meeting', timestamp: '2026-06-09T09:00:00Z' },
    ]);
  });
});

describe('ArisoBackend.searchMeetings', () => {
  it('maps remote search results to selectable list items', async () => {
    searchMeetings.mockResolvedValue([
      {
        id: 9,
        title: 'Pipeline Review',
        start_at: '2026-06-11T15:00:00Z',
        end_at: '2026-06-11T15:30:00Z',
        snippet: 'Discussed pipeline notes',
        matched_text: 'pipeline',
      },
    ]);

    const items = await new ArisoBackend().searchMeetings('pipeline');

    expect(searchMeetings).toHaveBeenCalledWith('pipeline');
    expect(items).toEqual([
      {
        id: '9',
        title: 'Pipeline Review',
        timestamp: '2026-06-11T15:00:00Z',
        endTimestamp: '2026-06-11T15:30:00Z',
        snippet: 'Discussed pipeline notes',
        matchedText: 'pipeline',
      },
    ]);
  });
});
