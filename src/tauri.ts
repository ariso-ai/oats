import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { load } from '@tauri-apps/plugin-store';

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

export const auth = {
  async googleSignIn(): Promise<{ success?: boolean; sessionToken?: string; error?: string }> {
    // Listen for the OAuth result event before triggering the flow
    const resultPromise = new Promise<SignInResult>((resolve) => {
      listen<SignInResult>('oauth-result', (event) => {
        resolve(event.payload);
      });
    });

    // Trigger the OAuth flow — opens a native webview window
    const immediate = await invoke<SignInResult>('google_sign_in');

    // If the command itself returned an error (e.g. prepare-state failed), return it
    if (immediate.error) {
      return { error: immediate.error };
    }

    // Wait for the OAuth window flow to complete
    const result = await resultPromise;
    return result;
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
};

export interface DesktopConfig {
  pusherKey: string;
  pusherCluster: string;
  webAppBaseUrl: string;
}

export async function getDesktopConfig(): Promise<DesktopConfig> {
  return invoke<DesktopConfig>('get_desktop_config');
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
    durationSeconds: number
  ): Promise<LocalFinalizeResult> {
    return invoke<LocalFinalizeResult>('local_finalize_recording', {
      audio,
      title,
      createdAt,
      durationSeconds,
    });
  },
  listRecordings(): Promise<RecordingSummary[]> {
    return invoke<RecordingSummary[]>('list_local_recordings');
  },
  readRecordingAudio(id: string): Promise<ArrayBuffer> {
    return invoke<ArrayBuffer>('read_recording_audio', { id });
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

/** Open (or focus) the first-run onboarding window. */
export async function openOnboardingWindow(): Promise<void> {
  await invoke('create_onboarding_window');
}

/** Open (or focus) Settings after flows that need the user back in native UI. */
export async function openSettingsWindow(): Promise<void> {
  await invoke('create_settings_window');
}
