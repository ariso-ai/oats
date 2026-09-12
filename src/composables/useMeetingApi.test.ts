import { describe, it, expect, vi, beforeEach } from 'vitest';

const apiRequest = vi.fn();

vi.mock('../tauri', () => ({
  api: {
    request: (...a: unknown[]) => apiRequest(...a),
    putPresigned: vi.fn(),
  },
}));

import { useMeetingApi, isMeetingNotesReady } from './useMeetingApi';

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

describe('useMeetingApi.getMeetingPrep', () => {
  it('GETs /meeting-preps/{id} and resolves the content plus its meeting', async () => {
    apiRequest.mockResolvedValue({
      status: 200,
      data: { meetingPrep: { id: 4339, meeting_id: '45565', content: '## Open items' } },
    });
    await expect(useMeetingApi().getMeetingPrep(4339)).resolves.toEqual({
      content: '## Open items',
      meetingId: '45565',
    });
    expect(apiRequest).toHaveBeenCalledWith('GET', '/meeting-preps/4339');
  });

  it('accepts a numeric meeting_id', async () => {
    apiRequest.mockResolvedValue({
      status: 200,
      data: { meetingPrep: { id: 4339, meeting_id: 45565, content: '## Open items' } },
    });
    await expect(useMeetingApi().getMeetingPrep(4339)).resolves.toEqual({
      content: '## Open items',
      meetingId: '45565',
    });
  });

  it('resolves null on 404 (no prep)', async () => {
    apiRequest.mockResolvedValue({ status: 404, data: null });
    await expect(useMeetingApi().getMeetingPrep(4339)).resolves.toBeNull();
  });

  it('resolves null content when it is missing or blank', async () => {
    apiRequest.mockResolvedValue({ status: 200, data: { meetingPrep: { id: 4339, meeting_id: '1' } } });
    await expect(useMeetingApi().getMeetingPrep(4339)).resolves.toEqual({
      content: null,
      meetingId: '1',
    });
    apiRequest.mockResolvedValue({
      status: 200,
      data: { meetingPrep: { id: 4339, meeting_id: '1', content: '   ' } },
    });
    await expect(useMeetingApi().getMeetingPrep(4339)).resolves.toEqual({
      content: null,
      meetingId: '1',
    });
  });

  it('resolves null meetingId when meeting_id is missing or unusable', async () => {
    apiRequest.mockResolvedValue({ status: 200, data: { meetingPrep: { id: 4339, content: 'x' } } });
    await expect(useMeetingApi().getMeetingPrep(4339)).resolves.toEqual({
      content: 'x',
      meetingId: null,
    });
  });

  it('resolves null when the payload has no meetingPrep', async () => {
    apiRequest.mockResolvedValue({ status: 200, data: {} });
    await expect(useMeetingApi().getMeetingPrep(4339)).resolves.toBeNull();
  });

  it('throws the server error on other non-200 statuses', async () => {
    apiRequest.mockResolvedValue({ status: 500, data: { error: 'boom' } });
    await expect(useMeetingApi().getMeetingPrep(4339)).rejects.toThrow('boom');
  });
});

describe('uploadAudio presign request', () => {
  it('includes fileSize so the backend size validation does not reject it', async () => {
    apiRequest
      .mockResolvedValueOnce({
        status: 200,
        data: { meetingId: 9, presignedUrl: 'https://bucket.s3.amazonaws.com/rec.mp3?sig=x' },
      })
      .mockResolvedValueOnce({ status: 202, data: {} });
    const { api } = await import('../tauri');
    (api.putPresigned as ReturnType<typeof vi.fn>).mockResolvedValue(200);

    await useMeetingApi().uploadAudio(new Blob(['audio'], { type: 'audio/mpeg' }));

    expect(apiRequest).toHaveBeenCalledWith(
      'POST',
      '/desktop/meetings/audio/presign',
      expect.objectContaining({ fileSize: 5 })
    );
  });
});

describe('uploadAudio stage tagging', () => {
  // Turning a multi-MB mp3 into a number[] can throw (RangeError/OOM) before a
  // single byte reaches S3. That failure belongs to the s3-put leg, not to the
  // presign leg it would otherwise fall back to in reportUploadFailure.
  it('tags a failed audio-byte conversion as the s3-put stage', async () => {
    apiRequest.mockResolvedValue({
      status: 200,
      data: { meetingId: 9, presignedUrl: 'https://bucket.s3.amazonaws.com/rec.mp3?sig=x' },
    });
    const realResponse = globalThis.Response;
    globalThis.Response = class {
      constructor() {
        throw new RangeError('Array buffer allocation failed');
      }
    } as unknown as typeof Response;

    try {
      await expect(
        useMeetingApi().uploadAudio(new Blob(['audio'], { type: 'audio/mpeg' }))
      ).rejects.toMatchObject({ name: 'UploadStageError', stage: 's3-put' });
    } finally {
      globalThis.Response = realResponse;
    }
  });
});

describe('listActionItemsByDay', () => {
  it('asks the action-items endpoint for one local day and returns its groups', async () => {
    apiRequest.mockResolvedValue({
      status: 200,
      data: {
        actionItems: [
          {
            meetingId: 9,
            meetingTitle: 'Q3 Pricing Review',
            startAt: '2026-08-31T09:00:00Z',
            actionItems: [{ name: 'Dana', item: 'Send pricing deck' }],
          },
        ],
      },
    });

    const groups = await useMeetingApi().listActionItemsByDay('2026-08-31');

    expect(apiRequest).toHaveBeenCalledWith(
      'GET',
      '/meeting-notes/action-items?day=2026-08-31'
    );
    expect(groups).toEqual([
      {
        meetingId: 9,
        meetingTitle: 'Q3 Pricing Review',
        startAt: '2026-08-31T09:00:00Z',
        actionItems: [{ name: 'Dana', item: 'Send pricing deck' }],
      },
    ]);
  });

  it('resolves to an empty list when the day carries no action items', async () => {
    apiRequest.mockResolvedValue({ status: 200, data: {} });
    await expect(useMeetingApi().listActionItemsByDay('2026-08-31')).resolves.toEqual([]);
  });

  it('surfaces the server error for a rejected day', async () => {
    apiRequest.mockResolvedValue({ status: 400, data: { error: 'Invalid date format' } });
    await expect(useMeetingApi().listActionItemsByDay('nope')).rejects.toThrow(
      'Invalid date format'
    );
  });
});

describe('action-item follow-ups', () => {
  it('lists a meeting’s action-item follow-ups with their text and completion', async () => {
    apiRequest.mockResolvedValue({
      status: 200,
      data: {
        followUps: [
          { id: 3, raw: { description: 'Send pricing deck', completed: true } },
          { id: 4, searchable_json: { description: 'Book the venue' } },
          { id: 5, description: 'Draft the RFC', raw: {} },
          { id: 6, raw: {} },
        ],
      },
    });

    const followUps = await useMeetingApi().listFollowUpsBySource(9);

    expect(apiRequest).toHaveBeenCalledWith(
      'GET',
      '/follow-ups/by-source?source_id=9&source_type=action_item'
    );
    // A row with no description can't be tied to any item, so it is dropped.
    expect(followUps).toEqual([
      { id: 3, description: 'Send pricing deck', completed: true },
      { id: 4, description: 'Book the venue', completed: false },
      { id: 5, description: 'Draft the RFC', completed: false },
    ]);
  });

  it('surfaces a failed follow-up lookup', async () => {
    apiRequest.mockResolvedValue({ status: 500, data: { error: 'boom' } });
    await expect(useMeetingApi().listFollowUpsBySource(9)).rejects.toThrow('boom');
  });

  it('creates a follow-up tied to the action item the way the web app does', async () => {
    apiRequest.mockResolvedValue({
      status: 201,
      data: { followUp: { id: 11, raw: { description: 'Ship it', completed: false } } },
    });

    const created = await useMeetingApi().createActionItemFollowUp(9, 'Platform Sync', 'Ship it');

    expect(apiRequest).toHaveBeenCalledWith('POST', '/follow-ups', {
      description: 'Ship it',
      importance: 0.7,
      reasoning: 'Action item from meeting: Platform Sync',
      expiresAt: null,
      sourceId: '9',
      sourceType: 'action_item',
    });
    expect(created).toEqual({ id: 11, description: 'Ship it', completed: false });
  });

  it('marks a follow-up complete or incomplete', async () => {
    apiRequest.mockResolvedValue({ status: 200, data: {} });
    await useMeetingApi().setFollowUpCompleted(11, true);
    expect(apiRequest).toHaveBeenCalledWith('PATCH', '/follow-ups/11/complete', { completed: true });
  });

  it('surfaces a failed completion write', async () => {
    apiRequest.mockResolvedValue({ status: 404, data: { error: 'Follow-up not found' } });
    await expect(useMeetingApi().setFollowUpCompleted(11, false)).rejects.toThrow(
      'Follow-up not found'
    );
  });
});

describe('reassignActionItem', () => {
  it('patches the assignee and returns the server-resolved owner', async () => {
    apiRequest.mockResolvedValue({
      status: 200,
      data: { actionItem: { id: 'a/1', name: 'Dana', meetingParticipantId: 42 } },
    });

    const owner = await useMeetingApi().reassignActionItem('9', 'a/1', 42);

    expect(apiRequest).toHaveBeenCalledWith(
      'PATCH',
      '/meeting-notes/9/action-items/a%2F1/assignee',
      { meetingParticipantId: 42 }
    );
    expect(owner).toEqual({ name: 'Dana', meetingParticipantId: 42 });
  });

  it('sends null to unassign', async () => {
    apiRequest.mockResolvedValue({
      status: 200,
      data: { actionItem: { id: 'a1', name: 'Unassigned', meetingParticipantId: null } },
    });
    const owner = await useMeetingApi().reassignActionItem('9', 'a1', null);
    expect(apiRequest.mock.calls[0][2]).toEqual({ meetingParticipantId: null });
    expect(owner).toEqual({ name: 'Unassigned', meetingParticipantId: null });
  });

  it('surfaces a rejected reassignment', async () => {
    apiRequest.mockResolvedValue({ status: 403, data: { error: 'Not a meeting attendee' } });
    await expect(useMeetingApi().reassignActionItem('9', 'a1', 42)).rejects.toThrow(
      'Not a meeting attendee'
    );
  });
});

describe('isMeetingNotesReady', () => {
  it('is ready once a transcript exists', () => {
    expect(isMeetingNotesReady({ hasTranscript: true })).toBe(true);
  });

  it('is ready once a summary exists, even without a transcript', () => {
    expect(isMeetingNotesReady({ hasTranscript: false, summary: '{"digest":"x"}' })).toBe(true);
  });

  it('is not ready when neither is present', () => {
    expect(isMeetingNotesReady({})).toBe(false);
    expect(isMeetingNotesReady({ hasTranscript: false, summary: null })).toBe(false);
    expect(isMeetingNotesReady({ summary: '' })).toBe(false);
  });
});
