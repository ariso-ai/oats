import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { load } from '@tauri-apps/plugin-store';
import { open as openDialog } from '@tauri-apps/plugin-dialog';

// Broadcast when any window completes desktop auth. Settings is pre-created and
// can mount before onboarding signs in, so it needs a cross-window refresh cue.
export const AUTH_SIGNED_IN_EVENT = 'auth://signed-in';

interface SignInResult {
  success?: boolean;
  sessionToken?: string;
  error?: string;
}

interface SessionResult {
  sessionToken: string;
}

interface ApiResponse {
  status: number;
  data: unknown;
}

// Error emitted by the backend when a pending sign-in is canceled (user gave
// up waiting on the browser). Views treat it as a silent cancel, not a failure.
export const SIGN_IN_CANCELED_ERROR = 'Sign-in canceled';

export const auth = {
  async googleSignIn(): Promise<{ success?: boolean; sessionToken?: string; error?: string }> {
    let resolveResult: (result: SignInResult) => void;
    const resultPromise = new Promise<SignInResult>((resolve) => {
      resolveResult = resolve;
    });

    // Await listener setup before triggering the flow. The backend scopes
    // its "oauth-result" emit to this webview (it carries the session
    // token), so listen on the current webview window too. If setup fails,
    // let it throw here rather than starting a sign-in that can never
    // resolve resultPromise.
    const unlisten = await getCurrentWebviewWindow().listen<SignInResult>(
      'oauth-result',
      (event) => {
        resolveResult(event.payload);
      }
    );

    try {
      // Trigger the OAuth flow — opens the sign-in page in the default browser
      const immediate = await invoke<SignInResult>('google_sign_in');

      // If the command itself returned an error (e.g. prepare-state failed), return it
      if (immediate.error) {
        return { error: immediate.error };
      }

      // Wait for the browser flow to hit the loopback callback and complete
      return await resultPromise;
    } finally {
      unlisten();
    }
  },

  async cancelSignIn(): Promise<void> {
    await invoke('cancel_google_sign_in');
  },

  async checkSession(): Promise<{ sessionToken: string } | null> {
    return invoke<SessionResult | null>('check_session');
  },

  async signOut(): Promise<void> {
    await invoke('sign_out');
  },
};

export const api = {
  async request(method: string, path: string, body?: unknown): Promise<{ status: number; data: unknown }> {
    return invoke<ApiResponse>('api_request', { method, path, body: body ?? null });
  },

  async uploadFile(
    path: string,
    fileData: number[],
    fileName: string,
    fields?: Record<string, string>
  ): Promise<{ status: number; data: unknown }> {
    return invoke<ApiResponse>('upload_file', {
      path,
      fileData,
      fileName,
      fields: fields ?? {},
    });
  },

  async putPresigned(
    url: string,
    data: number[],
    contentType: string
  ): Promise<number> {
    return invoke<number>('put_presigned', { url, data, contentType });
  },

  /** Raw audio bytes for an Ariso meeting. Rejects with a message prefixed
   *  by the HTTP status (e.g. "404: …") when the server has no audio. */
  async fetchMeetingAudio(
    meetingId: string | number,
    transcriptId?: string
  ): Promise<ArrayBuffer> {
    return invoke<ArrayBuffer>('fetch_meeting_audio', {
      meetingId: String(meetingId),
      transcriptId: transcriptId ?? null,
    });
  },
};

export interface DesktopConfig {
  pusherKey: string;
  pusherCluster: string;
  webAppBaseUrl: string;
}

export async function getDesktopConfig(): Promise<DesktopConfig> {
  return invoke<DesktopConfig>('get_desktop_config');
}

/** Product-level OS vocabulary shared with Rust. It intentionally omits target
 * architecture because feature decisions should not parse target triples. */
export type PlatformOs = 'macos' | 'windows' | 'linux';

/** Diagnostic identity of the native Local implementation. Frontend workflows
 * branch on `supported`; they do not dispatch directly on this engine label. */
export type LocalBackendEngine = 'swift-mlx' | 'cpp-sidecar' | null;

/** Compile-time feature support reported by the native host. This does not
 * represent OS permission state or whether local model files are installed. */
export interface PlatformCapabilities {
  os: PlatformOs;
  localBackend: { supported: boolean; engine: LocalBackendEngine };
  systemAudio: { supported: boolean; settingsUrl: string | null };
  autoRecord: { supported: boolean };
  nativeShare: { supported: boolean };
  notificationSettingsUrl: string | null;
  microphoneSettingsUrl: string | null;
}

/** Thin IPC boundary for callers that need the native source of truth. Caching
 * and browser/test fallback policy live in `usePlatformCapabilities`. */
export function getPlatformCapabilities(): Promise<PlatformCapabilities> {
  return invoke<PlatformCapabilities>('platform_capabilities');
}

export interface ShareAnchor {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Open the native macOS share sheet over `text`, anchored to `anchor`
 *  (the Share button's getBoundingClientRect in CSS px). macOS only. */
export function shareTextNative(text: string, anchor: ShareAnchor): Promise<void> {
  return invoke('share_text_native', { text, anchor });
}

export interface PusherAuthResponse {
  auth: string;
  channel_data?: string;
}

export async function pusherAuth(
  socketId: string,
  channelName: string
): Promise<PusherAuthResponse> {
  const res = await api.request('POST', '/pusher/auth', {
    socketId,
    channelName,
  });
  if (res.status !== 200) {
    throw new Error(`Pusher auth failed (${res.status})`);
  }
  return res.data as PusherAuthResponse;
}

export interface UpdateInfo {
  version: string;
  notes: string;
  mandatory: boolean;
}

export interface UpdateStateSnapshot {
  last_check_unix: number | null;
  latest_known: UpdateInfo | null;
  auto_check_enabled: boolean;
  skipped_version: string | null;
  snoozed_until_unix: number | null;
}

export const updater = {
  check(force = false): Promise<void> {
    return invoke('update_check', { force });
  },
  installAndRelaunch(): Promise<void> {
    return invoke('update_install_and_relaunch');
  },
  skipVersion(version: string): Promise<void> {
    return invoke('update_skip_version', { version });
  },
  snooze(): Promise<void> {
    return invoke('update_snooze');
  },
  setAutoCheck(enabled: boolean): Promise<void> {
    return invoke('update_set_auto_check', { enabled });
  },
  getState(): Promise<UpdateStateSnapshot> {
    return invoke('update_get_state');
  },
};

export interface RecordingSummary {
  id: string;
  title: string;
  createdAt: string;
  durationSeconds: number;
  status: 'recording' | 'transcribing' | 'done' | 'failed';
  hasAudio: boolean;
  hasNote: boolean;
  hasTranscript: boolean;
}

export type NotesStatus = 'pending' | 'ready' | 'failed';

/** Mirrors the Rust `RecordingStatusView`. Drives the detail panel's local
 *  generation poller (tab enable/disable + the inline status chip). */
export interface RecordingStatusView {
  status: RecordingSummary['status'];
  hasTranscript: boolean;
  hasNote: boolean;
  notesStatus: NotesStatus;
}

export interface LocalFinalizeResult {
  backend: 'local';
  id: string;
  title: string;
  status: 'recording' | 'transcribing' | 'done' | 'failed';
}

export interface ModelStatus {
  state: 'not_downloaded' | 'downloading' | 'ready' | 'error' | 'unsupported';
  version?: string;
  /** Whether the on-device notes LLM (gemma) has been downloaded. */
  llmReady?: boolean;
}

export const local = {
  finalizeRecording(
    audio: number[],
    title: string,
    createdAt: string,
    durationSeconds: number,
    appendTo?: string,
    forceNew?: boolean
  ): Promise<LocalFinalizeResult> {
    return invoke<LocalFinalizeResult>('local_finalize_recording', {
      audio,
      title,
      createdAt,
      durationSeconds,
      appendTo,
      forceNew,
    });
  },
  listRecordings(): Promise<RecordingSummary[]> {
    return invoke<RecordingSummary[]>('list_local_recordings');
  },
  /** Resolve the recording id a new local recording (starting at `createdAt`)
   *  will finalize into — the append target if it will merge into the recent
   *  recording, else the new recording's own id. Lets the recorder surface the
   *  right row up front instead of a phantom new note. */
  recordingIdForStart(createdAt: string, forceNew?: boolean): Promise<string> {
    return invoke<string>('local_recording_id_for_start', { createdAt, forceNew });
  },
  /** Cheap single-recording status for the detail panel's generation poller. */
  recordingStatus(id: string): Promise<RecordingStatusView> {
    return invoke<RecordingStatusView>('local_recording_status', { id });
  },
  /** Re-run transcription (and notes) for a failed recording from saved audio. */
  retryTranscription(id: string): Promise<LocalFinalizeResult> {
    return invoke<LocalFinalizeResult>('retry_local_transcription', { id });
  },
  /** Regenerate AI notes from the existing transcript (no STT re-run). */
  retryNotes(id: string): Promise<void> {
    return invoke('retry_local_notes', { id });
  },
  readRecordingAudio(id: string): Promise<ArrayBuffer> {
    return invoke<ArrayBuffer>('read_recording_audio', { id });
  },
  /** Reads the local `user-note.md` artifact the Library editor autosaves. */
  readRecordingNote(id: string): Promise<string> {
    return invoke<string>('read_recording_note', { id });
  },
  writeRecordingNote(id: string, markdown: string): Promise<void> {
    return invoke('write_recording_note', { id, markdown });
  },
  /** Reads the local `user-note-title.txt` sidecar holding the My-note title. */
  readRecordingNoteTitle(id: string): Promise<string> {
    return invoke<string>('read_recording_note_title', { id });
  },
  writeRecordingNoteTitle(id: string, title: string): Promise<void> {
    return invoke('write_recording_note_title', { id, title });
  },
  openRecordingFile(id: string, kind: 'note' | 'transcript'): Promise<void> {
    return invoke('open_recording_file', { id, kind });
  },
  /** Read a recording's note/transcript markdown for in-app rendering.
   *  Resolves to null when the file hasn't been generated yet. */
  readRecordingFile(id: string, kind: 'note' | 'transcript'): Promise<string | null> {
    return invoke<string | null>('read_recording_file', { id, kind });
  },
  /** Update a local recording's title in its meta.json (folder id unchanged). */
  renameRecording(id: string, title: string): Promise<void> {
    return invoke('rename_local_recording', { id, title });
  },
  modelStatus(): Promise<ModelStatus> {
    return invoke<ModelStatus>('local_model_status');
  },
  downloadStt(): Promise<void> {
    return invoke('download_local_stt');
  },
  downloadLlm(): Promise<void> {
    return invoke('download_local_llm');
  },
  openLibraryWindow(): Promise<void> {
    return invoke('create_library_window');
  },
};

/** Metadata persisted next to a buffered Ariso upload, mirrors the Rust
 *  `PendingUploadMeta`. Lets the Library resume a failed upload after restart. */
export interface PendingUploadMeta {
  createdAt: string;
  startAt: string | null;
  endAt: string;
  durationSeconds: number;
  meetingId?: number;
}

/** Disk buffer for Ariso uploads: audio + metadata are persisted before the
 *  upload attempt and removed once the server confirms (or the user
 *  dismisses). Keyed by the recording's ISO `createdAt`; Rust derives the id. */
export const pending = {
  bufferAudio(audio: number[], meta: PendingUploadMeta): Promise<string> {
    return invoke<string>('buffer_pending_audio', { audio, meta });
  },
  /** Idempotent — missing buffer files are not an error. */
  discardAudio(createdAt: string): Promise<void> {
    return invoke('discard_pending_audio', { createdAt });
  },
  /** Buffered uploads awaiting resume, oldest-first. */
  list(): Promise<PendingUploadMeta[]> {
    return invoke<PendingUploadMeta[]>('list_pending_uploads');
  },
  /** Concatenate the given buffers (chronological keys) into one mp3's bytes. */
  combine(createdAtKeys: string[]): Promise<ArrayBuffer> {
    return invoke<ArrayBuffer>('combine_pending_audio', { createdAtKeys });
  },
  /** Open `~/.ariso/pending-uploads` in Finder/Explorer, selecting this buffer. */
  reveal(createdAt: string): Promise<void> {
    return invoke('reveal_pending_upload', { createdAt });
  },
};

export async function getBackendSetting(): Promise<'ariso' | 'local'> {
  const store = await load('settings.json', { autoSave: true });
  const v = await store.get<string>('backend');
  return v === 'local' ? 'local' : 'ariso';
}

export async function setBackendSetting(backend: 'ariso' | 'local'): Promise<void> {
  const store = await load('settings.json', { autoSave: true });
  await store.set('backend', backend);
}

export async function isOnboarded(): Promise<boolean> {
  const store = await load('settings.json', { autoSave: true });
  return (await store.get<boolean>('onboarded')) === true;
}

export async function setOnboarded(value: boolean): Promise<void> {
  const store = await load('settings.json', { autoSave: true });
  await store.set('onboarded', value);
}

/** Whether the first-time "download local models?" dialog has been confirmed. */
export async function hasPromptedLocalModels(): Promise<boolean> {
  const store = await load('settings.json', { autoSave: true });
  return (await store.get<boolean>('localModelsPrompted')) === true;
}

export async function setPromptedLocalModels(value: boolean): Promise<void> {
  const store = await load('settings.json', { autoSave: true });
  await store.set('localModelsPrompted', value);
}

/** Open (or focus) the first-run onboarding window. */
export async function openOnboardingWindow(): Promise<void> {
  await invoke('create_onboarding_window');
}

/** Open (or focus) Settings after flows that need the user back in native UI. */
export async function openSettingsWindow(): Promise<void> {
  await invoke('create_settings_window');
}

/** Start native microphone capture. */
export async function startMicrophoneCapture(): Promise<void> {
  await invoke('start_microphone_capture');
}

/** Stop native microphone capture. */
export async function stopMicrophoneCapture(): Promise<void> {
  await invoke('stop_microphone_capture');
}

/** Prompt the user for microphone permission via the native OS dialog. */
export async function requestMicrophonePermission(): Promise<boolean> {
  return invoke<boolean>('request_microphone_permission');
}

/** Return the current microphone permission state without prompting. */
export async function checkMicrophonePermission(): Promise<boolean> {
  return invoke<boolean>('check_microphone_permission');
}

/** The active local vault directory (resolved absolute path). */
export function getVaultDir(): Promise<string> {
  return invoke<string>('get_vault_dir');
}

/** Point the local backend at a new vault directory (fresh store, no copy). */
export function setVaultDir(path: string): Promise<void> {
  return invoke('set_vault_dir', { path });
}

/** Open a native folder picker; returns the chosen absolute path or null. */
export async function pickVaultFolder(current?: string): Promise<string | null> {
  const picked = await openDialog({ directory: true, multiple: false, defaultPath: current });
  return typeof picked === 'string' ? picked : null;
}
