import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { MeetingListItem } from './useBackend';

const apiRequest = vi.fn();
const readRecordingNote = vi.fn();
const writeRecordingNote = vi.fn();

vi.mock('../tauri', () => ({
  api: {
    request: (...args: unknown[]) => apiRequest(...args),
  },
  local: {
    readRecordingNote: (id: string) => readRecordingNote(id),
    writeRecordingNote: (id: string, markdown: string) => writeRecordingNote(id, markdown),
  },
}));

import { useMeetingNotesPersistence } from './useMeetingNotesPersistence';

const localMeeting: MeetingListItem = {
  id: 'local-1',
  title: 'Local recording',
  timestamp: '2026-06-12T10:00:00Z',
  files: { hasAudio: true, hasNote: false, hasTranscript: false },
};

const remoteMeeting: MeetingListItem = {
  id: 'remote 42',
  title: 'Cloud meeting',
  timestamp: '2026-06-12T10:00:00Z',
};

describe('useMeetingNotesPersistence', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    apiRequest.mockResolvedValue({ status: 200, data: { content: 'cloud note' } });
    readRecordingNote.mockResolvedValue('local note');
    writeRecordingNote.mockResolvedValue(undefined);
  });

  it('uses local files for local recording notes', async () => {
    const persistence = useMeetingNotesPersistence();

    expect(persistence.modeFor(localMeeting)).toBe('local');
    expect(await persistence.load(localMeeting)).toBe('local note');
    await persistence.save(localMeeting, 'updated local');

    expect(readRecordingNote).toHaveBeenCalledWith('local-1');
    expect(writeRecordingNote).toHaveBeenCalledWith('local-1', 'updated local');
    expect(apiRequest).not.toHaveBeenCalled();
  });

  it('uses the backend personal-note endpoint for cloud meetings', async () => {
    const persistence = useMeetingNotesPersistence();

    expect(persistence.modeFor(remoteMeeting)).toBe('remote');
    expect(persistence.canEdit(remoteMeeting)).toBe(true);
    expect(await persistence.load(remoteMeeting)).toBe('cloud note');
    await persistence.save(remoteMeeting, 'updated cloud');

    expect(apiRequest).toHaveBeenNthCalledWith(
      1,
      'GET',
      '/meeting-notes/remote%2042/individual-note'
    );
    expect(apiRequest).toHaveBeenNthCalledWith(
      2,
      'PUT',
      '/meeting-notes/remote%2042/individual-note',
      { content: 'updated cloud' }
    );
    expect(readRecordingNote).not.toHaveBeenCalled();
    expect(writeRecordingNote).not.toHaveBeenCalled();
  });

  it('treats a missing cloud personal note as an empty editable note', async () => {
    apiRequest.mockResolvedValueOnce({ status: 404, data: { error: 'not found' } });
    const persistence = useMeetingNotesPersistence();

    await expect(persistence.load(remoteMeeting)).resolves.toBe('');
  });
});
