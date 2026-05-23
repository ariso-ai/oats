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
    title?: string
  ): Promise<{ meetingId: number }> {
    // Presigned flow: get a presigned S3 URL, upload directly, then confirm
    const presignRes = await api.request('POST', '/desktop/meetings/audio/presign', {
      filename: 'recording.mp3',
      title: title?.trim() || undefined,
    });
    assertOk(presignRes, 200, 'get presigned upload URL');
    const { meetingId, presignedUrl } = presignRes.data as {
      meetingId: number;
      presignedUrl: string;
    };

    // PUT directly to S3 via native HTTP client (avoids CORS in built app)
    const putStatus = await api.putPresigned(
      presignedUrl,
      [...new Uint8Array(await new Response(audioBlob).arrayBuffer())],
      'audio/mpeg'
    );
    if (putStatus < 200 || putStatus >= 300) {
      throw new Error(`S3 upload failed (${putStatus})`);
    }

    // Confirm upload and trigger transcription
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
    getMeeting,
    updateMeeting,
    endMeeting,
    saveTranscript,
    updateParticipantNames,
    generateMeetingNotes,
    uploadAudio,
  };
}

export type { Meeting, PaginatedResponse };
