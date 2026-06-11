import { local, auth, getBackendSetting, type RecordingSummary } from '../tauri';
import { useMeetingApi, type ScheduledMeeting } from './useMeetingApi';

export type BackendId = 'ariso' | 'local';

export interface Readiness {
  ready: boolean;
  reason?: 'signed-out' | 'model-missing' | 'unsupported-platform';
}

export interface RecordingMeta {
  startAt: string | null;
  endAt: string;
  durationSeconds: number;
  meetingId?: number;
}

export interface FinalizeResult {
  backend: BackendId;
  [k: string]: unknown;
}

export interface MeetingListItem {
  id: string;
  title: string;
  /** ISO timestamp: local `createdAt` or ariso `start_at`. View formats it. */
  timestamp: string;
  /** ISO end timestamp (ariso `end_at`); drives "Now" / start–end display. */
  endTimestamp?: string;
  /** Local recordings only. */
  durationSeconds?: number;
  /** Local recordings only. */
  status?: RecordingSummary['status'];
  /** Local recordings only — drives the row's audio/note/transcript controls. */
  files?: { hasAudio: boolean; hasNote: boolean; hasTranscript: boolean };
}

export interface MeetingActionItem {
  /** Owner/assignee name; absent for ungrouped items. */
  name?: string;
  item: string;
}

export interface MeetingCoaching {
  strengths?: string[];
  improvements?: string[];
  patterns?: string;
}

export interface MeetingParticipantInfo {
  name?: string;
  email?: string;
  role?: string;
  self?: boolean;
  avatarUrl?: string | null;
}

/** Normalized meeting detail rendered by the library's right-hand panel.
 *  Ariso meetings populate the rich fields (digest/summary/assessment/
 *  coaching); local recordings populate `note`/`transcript` markdown instead. */
export interface MeetingDetail {
  id: string;
  title: string;
  startAt: string;
  endAt?: string;
  visibility?: string;
  external?: boolean;
  participants: MeetingParticipantInfo[];
  // Ariso rich fields (markdown where noted)
  digest?: string;
  summary?: string;
  actionItems: MeetingActionItem[];
  score?: number;
  rationale?: string;
  recommendation?: string;
  coaching?: MeetingCoaching;
  meetingType?: string;
  /** Whether a transcript exists — drives the Live Transcript tab. Content is
   *  loaded lazily via `getMeetingTranscript`. */
  hasTranscript?: boolean;
  /** Whether the requester has an individual note — drives the "My note" tab.
   *  Content is loaded lazily via `getIndividualNote`. */
  hasIndividualNote?: boolean;
  // Local-recording fields
  isLocal: boolean;
  durationSeconds?: number;
  note?: string;
  transcript?: string;
}

export interface Backend {
  id: BackendId;
  needsAuth: boolean;
  usesMeetingPicker: boolean;
  isReady(): Promise<Readiness>;
  finalizeRecording(blob: Blob, meta: RecordingMeta): Promise<FinalizeResult>;
  listMeetings(): Promise<MeetingListItem[]>;
  /** Load the detail for a single row (from the list item the user clicked). */
  getMeetingDetail(item: MeetingListItem): Promise<MeetingDetail>;
  /** Lazily load the meeting's transcript text (null when none). */
  getMeetingTranscript(item: MeetingListItem): Promise<string | null>;
  /** Lazily load the requester's individual note (null when none). */
  getIndividualNote(item: MeetingListItem): Promise<{ content: string; title: string | null } | null>;
}

interface RawMeetingSummary {
  digest?: string;
  summary?: string;
  actionItems?: Array<string | { name?: string; item?: string }>;
  score?: number;
  rationale?: string;
  recommendation?: string;
  coaching?: MeetingCoaching;
  meetingType?: string;
}

// `summary` arrives as a JSON string, an already-parsed object, or plain prose.
// Normalize all three to a structured object; plain prose becomes the summary.
function parseMeetingSummary(
  summary: string | Record<string, unknown> | null | undefined
): RawMeetingSummary {
  if (!summary) return {};
  if (typeof summary === 'string') {
    try {
      return JSON.parse(summary) as RawMeetingSummary;
    } catch {
      return { summary };
    }
  }
  return summary as RawMeetingSummary;
}

function normalizeActionItems(
  items: RawMeetingSummary['actionItems']
): MeetingActionItem[] {
  if (!Array.isArray(items)) return [];
  return items
    .map((it) => {
      if (typeof it === 'string') return { item: it };
      const raw =
        it && typeof it === 'object'
          ? (it as { name?: unknown; item?: unknown })
          : {};
      const name = typeof raw.name === 'string' ? raw.name : undefined;
      const item = typeof raw.item === 'string' ? raw.item : '';
      return { name, item };
    })
    .filter((it) => it.item.trim().length > 0);
}

async function blobToBytes(blob: Blob): Promise<number[]> {
  return [...new Uint8Array(await blob.arrayBuffer())];
}

function timestampTitle(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return `Recording ${iso}`;
  // Build a consistent LOCAL "YYYY-MM-DD HH:MM" — both parts must use the same
  // timezone (mixing toISOString's UTC date with toTimeString's local time can
  // disagree near midnight).
  const pad = (n: number) => String(n).padStart(2, '0');
  const date = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  const time = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  return `Recording ${date} ${time}`;
}

export class ArisoBackend implements Backend {
  id: BackendId = 'ariso';
  needsAuth = true;
  usesMeetingPicker = true;

  async isReady(): Promise<Readiness> {
    const session = await auth.checkSession();
    return session ? { ready: true } : { ready: false, reason: 'signed-out' };
  }

  async finalizeRecording(blob: Blob, meta: RecordingMeta): Promise<FinalizeResult> {
    const { uploadAudio } = useMeetingApi();
    const { meetingId } = await uploadAudio(blob, {
      startAt: meta.startAt,
      endAt: meta.endAt,
      meetingId: meta.meetingId,
    });
    return { backend: 'ariso', meetingId };
  }

  async listMeetings(): Promise<MeetingListItem[]> {
    const { listMeetingsInWindow } = useMeetingApi();
    const { startDate, endDate } = arisoMeetingWindow(new Date());
    const meetings: ScheduledMeeting[] = await listMeetingsInWindow(startDate, endDate);
    return meetings.map((m) => ({
      id: String(m.id),
      title: m.title || 'Untitled meeting',
      timestamp: m.start_at,
      endTimestamp: m.end_at,
    }));
  }

  async getMeetingDetail(item: MeetingListItem): Promise<MeetingDetail> {
    const { getMeetingNotes } = useMeetingApi();
    const data = await getMeetingNotes(item.id);
    const s = parseMeetingSummary(data.summary);
    return {
      id: String(data.id ?? item.id),
      title: data.title || item.title || 'Untitled meeting',
      startAt: data.start_at || item.timestamp,
      endAt: data.end_at,
      visibility: data.visibility,
      external: data.external,
      participants: (data.participants ?? []).map((p) => ({
        name: p.name,
        email: p.email,
        role: p.role,
        self: p.self,
        avatarUrl: p.avatar_url ?? null,
      })),
      digest: s.digest,
      summary: typeof s.summary === 'string' ? s.summary : undefined,
      actionItems: normalizeActionItems(s.actionItems),
      score: typeof s.score === 'number' ? s.score : undefined,
      rationale: s.rationale,
      recommendation: s.recommendation,
      coaching: s.coaching,
      meetingType: s.meetingType,
      hasTranscript: !!data.hasTranscript,
      hasIndividualNote: !!data.individual_note?.content,
      isLocal: false,
    };
  }

  async getMeetingTranscript(item: MeetingListItem): Promise<string | null> {
    const { getMeetingTranscript } = useMeetingApi();
    return getMeetingTranscript(item.id);
  }

  async getIndividualNote(
    item: MeetingListItem
  ): Promise<{ content: string; title: string | null } | null> {
    const { getMeetingIndividualNote } = useMeetingApi();
    return getMeetingIndividualNote(item.id);
  }
}

export class LocalBackend implements Backend {
  id: BackendId = 'local';
  needsAuth = false;
  usesMeetingPicker = false;

  async isReady(): Promise<Readiness> {
    const status = await local.modelStatus();
    // 'unsupported' is reserved for a future Rust-side platform check
    // (Apple Silicon / macOS 14+); local_model_status does not emit it yet.
    if (status.state === 'unsupported') return { ready: false, reason: 'unsupported-platform' };
    return status.state === 'ready' ? { ready: true } : { ready: false, reason: 'model-missing' };
  }

  async finalizeRecording(blob: Blob, meta: RecordingMeta): Promise<FinalizeResult> {
    const createdAt = meta.startAt ?? meta.endAt;
    const bytes = await blobToBytes(blob);
    const res = await local.finalizeRecording(
      bytes,
      timestampTitle(createdAt),
      createdAt,
      meta.durationSeconds
    );
    return { backend: 'local', ...res };
  }

  async listMeetings(): Promise<MeetingListItem[]> {
    // No date window: the local library shows the full recording history.
    const recs = await local.listRecordings();
    return recs.map((r) => ({
      id: r.id,
      title: r.title,
      timestamp: r.createdAt,
      durationSeconds: r.durationSeconds,
      status: r.status,
      files: {
        hasAudio: r.hasAudio,
        hasNote: r.hasNote,
        hasTranscript: r.hasTranscript,
      },
    }));
  }

  async getMeetingDetail(item: MeetingListItem): Promise<MeetingDetail> {
    // Local recordings have no rich summary — just the generated note and
    // transcript markdown on disk. Read whichever exist (missing → null).
    const [note, transcript] = await Promise.all([
      item.files?.hasNote
        ? local.readRecordingFile(item.id, 'note').catch(() => null)
        : Promise.resolve(null),
      item.files?.hasTranscript
        ? local.readRecordingFile(item.id, 'transcript').catch(() => null)
        : Promise.resolve(null),
    ]);
    return {
      id: item.id,
      title: item.title,
      startAt: item.timestamp,
      participants: [],
      actionItems: [],
      isLocal: true,
      durationSeconds: item.durationSeconds,
      note: note ?? undefined,
      transcript: transcript ?? undefined,
      hasTranscript: !!item.files?.hasTranscript,
    };
  }

  async getMeetingTranscript(item: MeetingListItem): Promise<string | null> {
    if (!item.files?.hasTranscript) return null;
    return local.readRecordingFile(item.id, 'transcript').catch(() => null);
  }

  // Local recordings have no per-user individual note.
  async getIndividualNote(): Promise<{ content: string; title: string | null } | null> {
    return null;
  }
}

/** Inclusive day window for the Ariso meetings list: 7 days back … 7 days
 * forward, formatted as local `YYYY-MM-DD`. */
export function arisoMeetingWindow(now: Date): { startDate: string; endDate: string } {
  const pad = (n: number) => String(n).padStart(2, '0');
  const ymd = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  const start = new Date(now);
  start.setDate(start.getDate() - 7);
  const end = new Date(now);
  end.setDate(end.getDate() + 7);
  return { startDate: ymd(start), endDate: ymd(end) };
}

export async function getActiveBackend(): Promise<Backend> {
  // Never reject: a failed settings read must not break the recording window.
  // Fall back to the default (Ariso) backend.
  let id: BackendId = 'ariso';
  try {
    id = await getBackendSetting();
  } catch (e) {
    console.error('Failed to read backend setting; defaulting to Ariso', e);
  }
  return id === 'local' ? new LocalBackend() : new ArisoBackend();
}
