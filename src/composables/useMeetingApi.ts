import { api } from '../tauri';

interface TranscriptSegment {
  speaker: number;
  text: string;
  start: number;
  end: number;
}

// A single line of a stored transcript: the speaker-attributed `content`
// (e.g. "Speaker 1: …"), its zero-based `chunk_index`, and `start_ms` offset
// from the start of the recording.
interface TranscriptChunk {
  chunk_index: number;
  start_ms: number;
  content: string;
}

interface Meeting {
  id: number;
  title: string | null;
  status: string | null;
  created_at: string;
  updated_at: string;
  metadata: Record<string, unknown> | null;
}

interface PaginatedResponse {
  meetings: Meeting[];
  pagination: {
    page: number;
    limit: number;
    total: number;
    totalPages: number;
  };
}

interface ScheduledMeeting {
  id: number;
  title: string | null;
  start_at: string;
}

interface MeetingNotesParticipant {
  name?: string;
  email?: string;
  role?: string;
  self?: boolean;
  avatar_url?: string | null;
}

// The `/meeting-notes/:id` payload. `summary` is either a JSON string or an
// already-parsed object holding digest/summary/actionItems/score/coaching.
interface MeetingNotes {
  id: number;
  title: string | null;
  start_at: string;
  end_at?: string;
  status?: string;
  visibility?: string;
  external?: boolean;
  summary?: string | Record<string, unknown> | null;
  participants?: MeetingNotesParticipant[];
  hasTranscript?: boolean;
  // Requester's personal note (authenticated view); null when none written.
  individual_note?: { content?: string | null; title?: string | null } | null;
}

interface ScheduledMeetingsResponse {
  meetings: ScheduledMeeting[];
}

// Normalize a raw `transcript` payload into ordered chunks, tolerating missing
// or mistyped fields. Returns null when there are no usable chunks so callers
// can treat it the same as an absent transcript.
function parseTranscriptChunks(raw: unknown): TranscriptChunk[] | null {
  if (!Array.isArray(raw)) return null;
  const chunks = raw
    .filter((c): c is Record<string, unknown> => !!c && typeof c === 'object')
    .map((c) => ({
      chunk_index: typeof c.chunk_index === 'number' ? c.chunk_index : 0,
      start_ms: typeof c.start_ms === 'number' ? c.start_ms : 0,
      content: typeof c.content === 'string' ? c.content.trim() : '',
    }))
    .filter((c) => c.content.length > 0);
  return chunks.length ? chunks : null;
}

function assertOk(res: { status: number; data: unknown }, expected: number, action: string): void {
  if (res.status !== expected) {
    const data = res.data as { error?: string } | null;
    throw new Error(data?.error || `Failed to ${action} (${res.status})`);
  }
}

export function useMeetingApi() {
  async function getDeepgramToken(): Promise<string> {
    const res = await api.request('POST', '/desktop/deepgram-token');
    assertOk(res, 200, 'get transcription token');
    const data = res.data as { token?: string };
    if (!data.token) {
      throw new Error('No token returned from server');
    }
    return data.token;
  }

  async function createMeeting(
    title?: string
  ): Promise<{ meeting: Meeting }> {
    const res = await api.request('POST', '/desktop/meetings', { title });
    assertOk(res, 201, 'create meeting');
    return res.data as { meeting: Meeting };
  }

  async function listMeetings(
    page = 1,
    limit = 20
  ): Promise<PaginatedResponse> {
    const res = await api.request(
      'GET',
      `/desktop/meetings?page=${page}&limit=${limit}`
    );
    assertOk(res, 200, 'list meetings');
    return res.data as PaginatedResponse;
  }

  async function listScheduledMeetings(
    startDate: Date,
    endDate: Date
  ): Promise<ScheduledMeeting[]> {
    const params = new URLSearchParams({
      startDate: startDate.toISOString(),
      endDate: endDate.toISOString(),
    });
    const res = await api.request('GET', `/meetings?${params.toString()}`);
    assertOk(res, 200, 'list scheduled meetings');
    const data = res.data as ScheduledMeetingsResponse | null;
    return [...(data?.meetings ?? [])].sort(
      (a, b) =>
        new Date(a.start_at).getTime() - new Date(b.start_at).getTime()
    );
  }

  async function listMeetingsInWindow(
    startDate: string,
    endDate: string
  ): Promise<ScheduledMeeting[]> {
    const params = new URLSearchParams({
      start_date: startDate,
      end_date: endDate,
    });
    const res = await api.request('GET', `/meetings?${params.toString()}`);
    assertOk(res, 200, 'list meetings in window');
    const data = res.data as ScheduledMeetingsResponse | null;
    // Descending: soonest / most-recent meetings sit at the top of the list.
    return [...(data?.meetings ?? [])].sort(
      (a, b) => new Date(b.start_at).getTime() - new Date(a.start_at).getTime()
    );
  }

  async function getMeeting(
    meetingId: number
  ): Promise<{ meeting: Meeting }> {
    const res = await api.request('GET', `/desktop/meetings/${meetingId}`);
    assertOk(res, 200, 'get meeting');
    return res.data as { meeting: Meeting };
  }

  // Full meeting-notes payload (title, participants, summary JSON, assessment,
  // coaching). This is the same endpoint the web meeting-notes page uses; the
  // desktop library renders its detail panel from it.
  async function getMeetingNotes(
    meetingId: number | string
  ): Promise<MeetingNotes> {
    const encodedMeetingId = encodeURIComponent(String(meetingId));
    const res = await api.request('GET', `/meeting-notes/${encodedMeetingId}`);
    assertOk(res, 200, 'get meeting notes');
    return res.data as MeetingNotes;
  }

  // Rename a meeting via the shared meeting-notes endpoint (the same one the web
  // app uses). Used by the desktop detail panel's inline title editing.
  async function updateMeetingNotesTitle(
    meetingId: number | string,
    title: string
  ): Promise<void> {
    const encodedMeetingId = encodeURIComponent(String(meetingId));
    const res = await api.request('PATCH', `/meeting-notes/${encodedMeetingId}`, { title });
    assertOk(res, 200, 'update meeting title');
  }

  // Fetch a meeting's stored transcript as an ordered list of chunks. Resolves
  // to null on 404 (no transcript stored yet) so callers can treat "absent"
  // distinctly from a real error, and also when the payload holds no usable
  // chunks.
  async function getMeetingTranscript(
    meetingId: number | string
  ): Promise<TranscriptChunk[] | null> {
    const encodedMeetingId = encodeURIComponent(String(meetingId));
    const res = await api.request('GET', `/meeting-notes/${encodedMeetingId}/transcript`);
    if (res.status === 404) return null;
    assertOk(res, 200, 'get transcript');
    const data = res.data as { transcript?: unknown } | null;
    return parseTranscriptChunks(data?.transcript);
  }

  // Fetch the requester's individual note. Resolves to null on 404 or when no
  // note content is stored.
  async function getMeetingIndividualNote(
    meetingId: number | string
  ): Promise<{ content: string; title: string | null } | null> {
    const encodedMeetingId = encodeURIComponent(String(meetingId));
    const res = await api.request('GET', `/meeting-notes/${encodedMeetingId}/individual-note`);
    if (res.status === 404) return null;
    assertOk(res, 200, 'get individual note');
    const data = res.data as
      | { content?: string | null; title?: string | null; note?: { content?: string | null; title?: string | null } }
      | null;
    const content =
      typeof data?.content === 'string' && data.content
        ? data.content
        : typeof data?.note?.content === 'string'
          ? data.note.content
          : null;
    if (!content) return null;
    const title =
      typeof data?.title === 'string'
        ? data.title
        : typeof data?.note?.title === 'string'
          ? data.note.title
          : null;
    return { content, title };
  }

  async function updateMeeting(
    meetingId: number,
    updates: { status?: string; title?: string }
  ): Promise<{ meeting: Meeting }> {
    const res = await api.request(
      'PATCH',
      `/desktop/meetings/${meetingId}`,
      updates
    );
    assertOk(res, 200, 'update meeting');
    return res.data as { meeting: Meeting };
  }

  async function endMeeting(
    meetingId: number
  ): Promise<{ meeting: Meeting }> {
    const res = await api.request(
      'POST',
      `/desktop/meetings/${meetingId}/end`
    );
    assertOk(res, 200, 'end meeting');
    return res.data as { meeting: Meeting };
  }

  async function saveTranscript(
    meetingId: number,
    segments: TranscriptSegment[],
    hostSpeakerId?: number | null
  ): Promise<void> {
    const speakerIds = [...new Set(segments.map((s) => s.speaker))];
    const participants = speakerIds.map((id) => ({
      id,
      name: `Speaker ${id + 1}`,
      is_host: hostSpeakerId != null ? id === hostSpeakerId : id === (segments[0]?.speaker ?? 0),
    }));

    const transcriptPayload = {
      participants,
      segments: segments.map((s) => ({
        participant_id: s.speaker,
        text: s.text,
        start_timestamp: new Date(s.start * 1000).toISOString(),
        end_timestamp: new Date(s.end * 1000).toISOString(),
      })),
    };

    const res = await api.request(
      'POST',
      `/meeting-notes/${meetingId}/transcript`,
      transcriptPayload
    );
    assertOk(res, 202, 'save transcript');
  }

  async function generateMeetingNotes(meetingId: number): Promise<void> {
    const res = await api.request(
      'POST',
      `/desktop/meetings/${meetingId}/generate-notes`
    );
    assertOk(res, 202, 'generate meeting notes');
  }

  async function updateParticipantNames(
    meetingId: number,
    participants: Array<{ participant_id: number; name: string }>
  ): Promise<void> {
    await api.request(
      'PATCH',
      `/desktop/meetings/${meetingId}/participants`,
      { participants }
    );
  }

  async function uploadAudio(
    audioBlob: Blob,
    options?: {
      title?: string;
      startAt?: string | null;
      endAt?: string;
      meetingId?: number;
    }
  ): Promise<{ meetingId: number }> {
    const metadata: Record<string, string> = {
      endAt: options?.endAt ?? new Date().toISOString(),
    };
    if (options?.startAt) metadata.startAt = options.startAt;

    const presignPath =
      options?.meetingId != null
        ? `/desktop/meetings/${options.meetingId}/audio/presign`
        : '/desktop/meetings/audio/presign';

    const presignRes = await api.request('POST', presignPath, {
      filename: 'recording.mp3',
      title: options?.title?.trim() || undefined,
      metadata,
    });
    assertOk(presignRes, 200, 'get presigned upload URL');
    const { meetingId, presignedUrl } = presignRes.data as {
      meetingId: number;
      presignedUrl: string;
    };

    const putStatus = await api.putPresigned(
      presignedUrl,
      [...new Uint8Array(await new Response(audioBlob).arrayBuffer())],
      'audio/mpeg'
    );
    if (putStatus < 200 || putStatus >= 300) {
      throw new Error(`S3 upload failed (${putStatus})`);
    }

    const confirmRes = await api.request(
      'POST',
      `/desktop/meetings/${meetingId}/audio/confirm`
    );
    assertOk(confirmRes, 202, 'confirm audio upload');
    return { meetingId };
  }

  return {
    getDeepgramToken,
    createMeeting,
    listMeetings,
    listScheduledMeetings,
    listMeetingsInWindow,
    getMeeting,
    getMeetingNotes,
    updateMeetingNotesTitle,
    getMeetingTranscript,
    getMeetingIndividualNote,
    updateMeeting,
    endMeeting,
    saveTranscript,
    updateParticipantNames,
    generateMeetingNotes,
    uploadAudio,
  };
}

export type { Meeting, PaginatedResponse, ScheduledMeeting, MeetingNotes, TranscriptChunk };
