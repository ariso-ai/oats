import { api } from '../tauri';

interface TranscriptSegment {
  speaker: number;
  text: string;
  start: number;
  end: number;
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

interface ScheduledMeetingsResponse {
  meetings: ScheduledMeeting[];
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

  async function getMeeting(
    meetingId: number
  ): Promise<{ meeting: Meeting }> {
    const res = await api.request('GET', `/desktop/meetings/${meetingId}`);
    assertOk(res, 200, 'get meeting');
    return res.data as { meeting: Meeting };
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
    getMeeting,
    updateMeeting,
    endMeeting,
    saveTranscript,
    updateParticipantNames,
    generateMeetingNotes,
    uploadAudio,
  };
}

export type { Meeting, PaginatedResponse, ScheduledMeeting };
