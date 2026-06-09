import { local, auth, getBackendSetting, type RecordingSummary } from '../tauri';
import { useMeetingApi } from './useMeetingApi';

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
  /** Local recordings only. */
  durationSeconds?: number;
  /** Local recordings only. */
  status?: RecordingSummary['status'];
  /** Local recordings only — drives the row's audio/note/transcript controls. */
  files?: { hasAudio: boolean; hasNote: boolean; hasTranscript: boolean };
}

export interface Backend {
  id: BackendId;
  needsAuth: boolean;
  usesMeetingPicker: boolean;
  isReady(): Promise<Readiness>;
  finalizeRecording(blob: Blob, meta: RecordingMeta): Promise<FinalizeResult>;
  listMeetings(): Promise<MeetingListItem[]>;
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
    const meetings = await listMeetingsInWindow(startDate, endDate);
    return meetings.map((m) => ({
      id: String(m.id),
      title: m.title || 'Untitled meeting',
      timestamp: m.start_at,
    }));
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
}

/** Inclusive day window for the Ariso meetings list: 7 days back … 1 day
 * forward (covers the next ~24h), formatted as local `YYYY-MM-DD`. */
export function arisoMeetingWindow(now: Date): { startDate: string; endDate: string } {
  const pad = (n: number) => String(n).padStart(2, '0');
  const ymd = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  const start = new Date(now);
  start.setDate(start.getDate() - 7);
  const end = new Date(now);
  end.setDate(end.getDate() + 1);
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
