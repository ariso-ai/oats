import { local, auth, getBackendSetting } from '../tauri';
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

export interface Backend {
  id: BackendId;
  needsAuth: boolean;
  usesMeetingPicker: boolean;
  isReady(): Promise<Readiness>;
  finalizeRecording(blob: Blob, meta: RecordingMeta): Promise<FinalizeResult>;
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
}

export async function getActiveBackend(): Promise<Backend> {
  const id = await getBackendSetting();
  return id === 'local' ? new LocalBackend() : new ArisoBackend();
}
