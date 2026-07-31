import { api } from '../tauri';
import { UploadStageError, type UploadStage } from './useDiagnostics';

interface TranscriptSegment {
  speaker: number;
  text: string;
  start: number;
  end: number;
}

// A single line of a stored transcript: the speaker-attributed `content`
// (e.g. "Speaker 1: …"), its zero-based `chunk_index`, and `start_ms` offset
// from the start of the recording. `transcript_id` ties the chunk back to the
// audio clip it came from ('legacy' for pre-multi-clip transcripts).
interface TranscriptChunk {
  chunk_index: number;
  start_ms: number;
  content: string;
  transcript_id: string;
}

// A single recorded audio segment ("clip") within a meeting. Legacy
// (pre-multi-clip) meetings surface exactly one clip:
// `{ transcript_id: 'legacy', legacy: true }`.
export interface MeetingAudioClip {
  transcript_id: string;
  duration_ms: number | null;
  created_at: string;
  legacy: boolean;
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
  end_at?: string;
  /** Lifecycle status ('created' | 'recording' | 'done' | 'cancelled' | …).
   *  A deleted/canceled calendar event surfaces here as 'cancelled'. */
  status?: string | null;
  /** Ariso: when truthy, Ari (the notetaker bot) is scheduled to auto-join and
   *  record this meeting server-side. May arrive as bool / 0-1 / "true"-"1". */
  auto_join_scheduled?: boolean | number | string;
  /** Ariso: present when a meeting prep exists for this meeting. May arrive
   *  as a number or numeric string. */
  prep_id?: number | string;
}

/** A meeting prep, reduced to the two fields the desktop consumes: the markdown
 *  to render, and the meeting it belongs to (used to resolve a prep-ready
 *  notification back to a row in the library). */
export interface MeetingPrep {
  content: string | null;
  meetingId: string | null;
}

// Search currently returns the same meeting shape as `/meetings`, with optional
// match context left open for a backend that can provide Granola-style snippets.
interface MeetingSearchResult extends ScheduledMeeting {
  snippet?: string | null;
  matched_text?: string | null;
  highlights?: string[] | null;
}

interface MeetingNotesParticipant {
  id?: number;
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
  short_code?: string;
  public_share_expires_at?: string | null;
  shareMeetingNotesToPublic?: 'attendee_and_host' | 'host_only' | 'off';
  summary?: string | Record<string, unknown> | null;
  participants?: MeetingNotesParticipant[];
  hasTranscript?: boolean;
  // Requester's personal note (authenticated view); null when none written.
  individual_note?: { content?: string | null; title?: string | null } | null;
  audio_clips?: MeetingAudioClip[];
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
      transcript_id: typeof c.transcript_id === 'string' ? c.transcript_id : 'legacy',
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

// Tag whatever an upload leg throws — including invoke rejections carrying
// reqwest's transport error text — with the leg it came from. Errors already
// tagged pass through so an inner stage isn't relabelled by an outer one.
async function withStage<T>(stage: UploadStage, run: () => Promise<T>): Promise<T> {
  try {
    return await run();
  } catch (e) {
    if (e instanceof UploadStageError) throw e;
    throw new UploadStageError(stage, e instanceof Error ? e.message : String(e ?? ''));
  }
}

// Like assertOk but accepts any 2xx status — for endpoints whose exact success
// code we don't pin (mirrors the web app's axios any-2xx behavior).
function assertOk2xx(res: { status: number; data: unknown }, action: string): void {
  if (res.status < 200 || res.status >= 300) {
    const data = res.data as { error?: string } | null;
    throw new Error(data?.error || `Failed to ${action} (${res.status})`);
  }
}

// The POST /meeting-notes/audio response shape isn't strictly pinned, so pull
// the new meeting id from the handful of shapes the API uses elsewhere: a bare
// `{ id }` / `{ meetingId }`, or a nested meeting / meeting-note object.
function extractMeetingId(data: unknown): number | null {
  if (!data || typeof data !== 'object') return null;
  const d = data as Record<string, unknown>;
  const nested = (key: string): unknown =>
    (d[key] as Record<string, unknown> | undefined)?.id;
  const candidates: unknown[] = [
    d.id,
    d.meetingId,
    nested('meeting'),
    nested('meetingNote'),
    nested('meeting_note'),
  ];
  for (const c of candidates) {
    if (typeof c === 'number' && Number.isSafeInteger(c)) return c;
    if (typeof c === 'string' && /^\d+$/.test(c)) return Number(c);
  }
  return null;
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

  // Create an ad-hoc meeting to record straight into. The backend seeds it with
  // the current user as the sole participant; we only supply an optional title
  // and the start time. Returns the new meeting's id so the recorder can attach
  // its upload to it.
  async function createAudioMeeting(
    title?: string
  ): Promise<{ meetingId: number }> {
    const trimmed = title?.trim();
    const body: { startAt: string; title?: string } = {
      startAt: new Date().toISOString(),
    };
    if (trimmed) body.title = trimmed;
    const res = await api.request('POST', '/meeting-notes/audio', body);
    if (res.status !== 200 && res.status !== 201) {
      const data = res.data as { error?: string } | null;
      throw new Error(data?.error || `Failed to create meeting (${res.status})`);
    }
    const meetingId = extractMeetingId(res.data);
    if (meetingId == null) {
      throw new Error('Server did not return a meeting id');
    }
    return { meetingId };
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

  // Remote library search uses the shared `/meetings?q=...` endpoint. That
  // endpoint owns auth, visibility, and keyword matching; the desktop only
  // normalizes ordering and passes optional match context through.
  async function searchMeetings(
    query: string,
    limit = 20
  ): Promise<MeetingSearchResult[]> {
    const trimmed = query.trim();
    if (!trimmed) return [];
    const params = new URLSearchParams({
      q: trimmed,
      limit: String(limit),
    });
    const res = await api.request('GET', `/meetings?${params.toString()}`);
    assertOk(res, 200, 'search meetings');
    const data = res.data as { meetings?: MeetingSearchResult[] } | null;
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

  // Host-only delete of a single recording clip by its transcript id, leaving
  // the meeting's other clips/transcripts/notes intact.
  async function deleteMeetingRecordingClip(
    meetingId: number | string,
    transcriptId: string
  ): Promise<void> {
    const encodedMeetingId = encodeURIComponent(String(meetingId));
    const encodedTranscriptId = encodeURIComponent(transcriptId);
    const res = await api.request(
      'DELETE',
      `/meeting-notes/${encodedMeetingId}/recording/${encodedTranscriptId}`
    );
    assertOk(res, 200, 'delete recording clip');
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

  // Fetch a meeting prep: its markdown content plus the meeting it belongs to
  // (the notification path only knows the prep id, so it resolves the meeting
  // from here). Resolves to null on 404, so callers can treat "absent"
  // distinctly from a real error (same convention as getMeetingTranscript);
  // `content` is null on its own when the payload holds no usable markdown.
  async function getMeetingPrep(prepId: number): Promise<MeetingPrep | null> {
    const res = await api.request('GET', `/meeting-preps/${encodeURIComponent(String(prepId))}`);
    if (res.status === 404) return null;
    assertOk(res, 200, 'get meeting prep');
    const prep = (res.data as { meetingPrep?: Record<string, unknown> } | null)?.meetingPrep;
    if (!prep) return null;
    const content = prep.content;
    // meeting_id comes back as a string ("45565") but tolerate a number, the
    // way prep_id is tolerated on the meeting row.
    const meetingId = prep.meeting_id;
    return {
      content: typeof content === 'string' && content.trim() ? content : null,
      meetingId:
        typeof meetingId === 'string' && meetingId !== ''
          ? meetingId
          : typeof meetingId === 'number' && Number.isFinite(meetingId)
            ? String(meetingId)
            : null,
    };
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

  async function shareMeeting(
    meetingId: number | string,
    visibility: 'private' | 'workspace' | 'public',
    expiresInDays?: number
  ): Promise<{ shareUrl: string; shortCode?: string; publicShareExpiresAt: string | null }> {
    const encoded = encodeURIComponent(String(meetingId));
    const body: { visibility: string; expiresInDays?: number } = { visibility };
    if (visibility === 'public' && typeof expiresInDays === 'number') {
      body.expiresInDays = expiresInDays;
    }
    const res = await api.request('POST', `/meeting-notes/${encoded}/share`, body);
    assertOk(res, 200, 'share meeting');
    const data = res.data as
      | { shareUrl?: string; shortCode?: string; publicShareExpiresAt?: string | null }
      | null;
    return {
      shareUrl: data?.shareUrl ?? '',
      shortCode: data?.shortCode,
      publicShareExpiresAt: data?.publicShareExpiresAt ?? null,
    };
  }

  // Errors collapse to [] — this is a passive, non-critical lookup.
  async function listShareEmails(meetingId: number | string): Promise<string[]> {
    const encoded = encodeURIComponent(String(meetingId));
    try {
      const res = await api.request('GET', `/meeting-notes/${encoded}/share-emails`);
      if (res.status !== 200) return [];
      const data = res.data as { items?: Array<{ email?: string }> } | null;
      return (data?.items ?? [])
        .map((s) => (typeof s.email === 'string' ? s.email : ''))
        .filter((e) => e.length > 0);
    } catch {
      return [];
    }
  }

  async function sendShareEmail(
    meetingId: number | string,
    email: string
  ): Promise<{ alreadyShared: boolean }> {
    const encoded = encodeURIComponent(String(meetingId));
    const res = await api.request('POST', `/meeting-notes/${encoded}/share-email`, { email });
    assertOk2xx(res, 'send email');
    const data = res.data as { already_shared?: boolean } | null;
    return { alreadyShared: data?.already_shared === true };
  }

  async function unshareEmail(meetingId: number | string, email: string): Promise<void> {
    const encoded = encodeURIComponent(String(meetingId));
    const res = await api.request(
      'DELETE',
      `/meeting-notes/${encoded}/share-email?email=${encodeURIComponent(email)}`
    );
    assertOk2xx(res, 'unshare');
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

    // Each leg is tagged with its stage so a failure can be reported (and
    // grouped in Sentry) by *where* the upload broke rather than by the
    // message text, which varies with the server's error copy. See
    // useDiagnostics.reportUploadFailure.
    const presignRes = await withStage('presign', () =>
      api.request('POST', presignPath, {
        filename: 'recording.mp3',
        title: options?.title?.trim() || undefined,
        metadata,
      })
    );
    if (presignRes.status !== 200) {
      const data = presignRes.data as { error?: string } | null;
      throw new UploadStageError(
        'presign',
        data?.error || `Failed to get presigned upload URL (${presignRes.status})`,
        presignRes.status
      );
    }
    const { meetingId, presignedUrl } = presignRes.data as {
      meetingId: number;
      presignedUrl: string;
    };

    const audioBytes = [...new Uint8Array(await new Response(audioBlob).arrayBuffer())];
    const putStatus = await withStage('s3-put', () =>
      api.putPresigned(presignedUrl, audioBytes, 'audio/mpeg')
    );
    if (putStatus < 200 || putStatus >= 300) {
      throw new UploadStageError('s3-put', `S3 upload failed (${putStatus})`, putStatus);
    }

    const confirmRes = await withStage('confirm', () =>
      api.request('POST', `/desktop/meetings/${meetingId}/audio/confirm`)
    );
    if (confirmRes.status !== 202) {
      const data = confirmRes.data as { error?: string } | null;
      throw new UploadStageError(
        'confirm',
        data?.error || `Failed to confirm audio upload (${confirmRes.status})`,
        confirmRes.status
      );
    }
    return { meetingId };
  }

  return {
    getDeepgramToken,
    createMeeting,
    createAudioMeeting,
    listMeetings,
    listScheduledMeetings,
    listMeetingsInWindow,
    searchMeetings,
    getMeeting,
    getMeetingNotes,
    updateMeetingNotesTitle,
    deleteMeetingRecordingClip,
    getMeetingTranscript,
    getMeetingIndividualNote,
    getMeetingPrep,
    updateMeeting,
    endMeeting,
    saveTranscript,
    updateParticipantNames,
    generateMeetingNotes,
    uploadAudio,
    shareMeeting,
    listShareEmails,
    sendShareEmail,
    unshareEmail,
  };
}

export type {
  Meeting,
  PaginatedResponse,
  ScheduledMeeting,
  MeetingSearchResult,
  MeetingNotes,
  TranscriptChunk,
};
