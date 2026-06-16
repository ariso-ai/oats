import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { MeetingListItem } from './useBackend';

const apiRequest = vi.fn();
const readRecordingNote = vi.fn();
const writeRecordingNote = vi.fn();
const readRecordingNoteTitle = vi.fn();
const writeRecordingNoteTitle = vi.fn();

vi.mock('../tauri', () => ({
  api: { request: (...a: unknown[]) => apiRequest(...a) },
  local: {
    readRecordingNote: (...a: unknown[]) => readRecordingNote(...a),
    writeRecordingNote: (...a: unknown[]) => writeRecordingNote(...a),
    readRecordingNoteTitle: (...a: unknown[]) => readRecordingNoteTitle(...a),
    writeRecordingNoteTitle: (...a: unknown[]) => writeRecordingNoteTitle(...a),
  },
}));

import { useMeetingNotesPersistence } from './useMeetingNotesPersistence';

// A local meeting is identified by the presence of recording `files`; remote
// meetings have none.
const localMeeting = { id: 'rec-1', files: { hasTranscript: true } } as unknown as MeetingListItem;
const remoteMeeting = { id: '42' } as unknown as MeetingListItem;

beforeEach(() => {
  apiRequest.mockReset();
  readRecordingNote.mockReset();
  writeRecordingNote.mockReset();
  readRecordingNoteTitle.mockReset();
  writeRecordingNoteTitle.mockReset();
});

describe('local note persistence', () => {
  it('load reads body and title sidecar', async () => {
    readRecordingNote.mockResolvedValue('# Body');
    readRecordingNoteTitle.mockResolvedValue('Kickoff');
    const note = await useMeetingNotesPersistence().load(localMeeting);
    expect(readRecordingNote).toHaveBeenCalledWith('rec-1');
    expect(readRecordingNoteTitle).toHaveBeenCalledWith('rec-1');
    expect(note).toEqual({ content: '# Body', title: 'Kickoff' });
  });

  it('save writes body and title sidecar', async () => {
    writeRecordingNote.mockResolvedValue(undefined);
    writeRecordingNoteTitle.mockResolvedValue(undefined);
    await useMeetingNotesPersistence().save(localMeeting, { content: '# Body', title: 'Kickoff' });
    expect(writeRecordingNote).toHaveBeenCalledWith('rec-1', '# Body');
    expect(writeRecordingNoteTitle).toHaveBeenCalledWith('rec-1', 'Kickoff');
  });
});

describe('remote note persistence', () => {
  it('load returns content and title from the GET payload', async () => {
    apiRequest.mockResolvedValue({ status: 200, data: { content: 'hi', title: 'Sync' } });
    const note = await useMeetingNotesPersistence().load(remoteMeeting);
    expect(apiRequest).toHaveBeenCalledWith('GET', '/meeting-notes/42/individual-note');
    expect(note).toEqual({ content: 'hi', title: 'Sync' });
  });

  it('load defaults a missing title to empty string', async () => {
    apiRequest.mockResolvedValue({ status: 200, data: { content: 'hi' } });
    expect(await useMeetingNotesPersistence().load(remoteMeeting)).toEqual({ content: 'hi', title: '' });
  });

  it('load returns empty note on 404', async () => {
    apiRequest.mockResolvedValue({ status: 404, data: null });
    expect(await useMeetingNotesPersistence().load(remoteMeeting)).toEqual({ content: '', title: '' });
  });

  it('save PUTs content and title together', async () => {
    apiRequest.mockResolvedValue({ status: 200, data: {} });
    await useMeetingNotesPersistence().save(remoteMeeting, { content: 'hi', title: 'Sync' });
    expect(apiRequest).toHaveBeenCalledWith('PUT', '/meeting-notes/42/individual-note', {
      content: 'hi',
      title: 'Sync',
    });
  });
});
