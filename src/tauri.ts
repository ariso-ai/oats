import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

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
