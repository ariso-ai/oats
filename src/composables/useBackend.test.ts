import { describe, it, expect, vi, beforeEach } from 'vitest';

const localFinalize = vi.fn();
const listRecordings = vi.fn();
const listMeetingsInWindow = vi.fn();
const apiRequest = vi.fn();
const putPresigned = vi.fn();
const checkSession = vi.fn();
const modelStatus = vi.fn();
const getBackendSetting = vi.fn();
const uploadAudio = vi.fn();

vi.mock('../tauri', () => ({
  local: {
    finalizeRecording: (...a: unknown[]) => localFinalize(...a),
    modelStatus: () => modelStatus(),
    listRecordings: () => listRecordings(),
  },
  auth: { checkSession: () => checkSession() },
  api: {
    request: (...a: unknown[]) => apiRequest(...a),
    putPresigned: (...a: unknown[]) => putPresigned(...a),
  },
  getBackendSetting: () => getBackendSetting(),
}));

vi.mock('./useMeetingApi', () => ({
  useMeetingApi: () => ({
    uploadAudio: (...a: unknown[]) => uploadAudio(...a),
    listMeetingsInWindow: (...a: unknown[]) => listMeetingsInWindow(...a),
  }),
}));

import { ArisoBackend, LocalBackend, getActiveBackend, arisoMeetingWindow } from './useBackend';

beforeEach(() => {
  vi.clearAllMocks();
});

describe('LocalBackend', () => {
  it('declares no auth and no picker', () => {
    const b = new LocalBackend();
    expect(b.id).toBe('local');
    expect(b.needsAuth).toBe(false);
    expect(b.usesMeetingPicker).toBe(false);
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
});

describe('ArisoBackend', () => {
  it('declares auth + picker', () => {
    const b = new ArisoBackend();
    expect(b.id).toBe('ariso');
    expect(b.needsAuth).toBe(true);
    expect(b.usesMeetingPicker).toBe(true);
  });

  it('isReady reflects session', async () => {
    const b = new ArisoBackend();
    checkSession.mockResolvedValue({ sessionToken: 'tok' });
    expect(await b.isReady()).toEqual({ ready: true });
    checkSession.mockResolvedValue(null);
    expect(await b.isReady()).toEqual({ ready: false, reason: 'signed-out' });
  });

  it('finalizeRecording uploads via useMeetingApi and returns the meetingId', async () => {
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
  it('spans 7 days back to 1 day forward, date-only', () => {
    // Local midday avoids any tz boundary ambiguity.
    const now = new Date(2026, 5, 9, 12, 0, 0); // 2026-06-09 local
    expect(arisoMeetingWindow(now)).toEqual({
      startDate: '2026-06-02',
      endDate: '2026-06-10',
    });
  });

  it('rolls back across a month/year boundary', () => {
    const now = new Date(2026, 0, 3, 12, 0, 0); // 2026-01-03 local
    expect(arisoMeetingWindow(now)).toEqual({
      startDate: '2025-12-27',
      endDate: '2026-01-04',
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
