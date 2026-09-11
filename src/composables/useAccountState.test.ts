// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { flushPromises } from '@vue/test-utils';

type SignInResult = { success?: boolean; sessionToken?: string; error?: string };
const checkSession = vi.fn((): Promise<unknown> => Promise.resolve(null));
const googleSignIn = vi.fn((): Promise<SignInResult> => Promise.resolve({ success: true }));
const microsoftSignIn = vi.fn((): Promise<SignInResult> => Promise.resolve({ success: true }));
const cancelSignIn = vi.fn(() => Promise.resolve());
const signOut = vi.fn(() => Promise.resolve());
const apiRequest = vi.fn(
  (_method: string, _path: string): Promise<{ status: number; data: unknown }> =>
    Promise.resolve({ status: 200, data: {} })
);
const emitNotificationsSync = vi.fn(() => Promise.resolve());

vi.mock('../tauri', () => ({
  SIGN_IN_CANCELED_ERROR: 'Sign-in canceled',
  auth: {
    checkSession: () => checkSession(),
    googleSignIn: () => googleSignIn(),
    microsoftSignIn: () => microsoftSignIn(),
    cancelSignIn: () => cancelSignIn(),
    signOut: () => signOut(),
  },
  api: {
    request: (method: string, path: string) => apiRequest(method, path),
  },
}));
vi.mock('./useMeetingNotifications', () => ({
  emitNotificationsSync: () => emitNotificationsSync(),
}));

import { useAccountState } from './useAccountState';

// The avatar is preloaded through a detached Image; let it load immediately.
class FakeImage {
  referrerPolicy = '';
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  set src(_v: string) {
    queueMicrotask(() => this.onload?.());
  }
}

function mockProfile() {
  apiRequest.mockImplementation((_method: string, path: string) => {
    if (path === '/auth/me') {
      return Promise.resolve({
        status: 200,
        data: { full_name: 'Ada Lovelace', email: 'ada@example.com' },
      });
    }
    if (path === '/users/google-avatar') {
      return Promise.resolve({ status: 200, data: { avatar: 'https://example.com/a.png' } });
    }
    return Promise.resolve({ status: 200, data: {} });
  });
}

function profileFetches(): number {
  return apiRequest.mock.calls.filter((call) => call[1] === '/auth/me').length;
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal('Image', FakeImage);
  checkSession.mockResolvedValue(null);
  googleSignIn.mockResolvedValue({ success: true });
  microsoftSignIn.mockResolvedValue({ success: true });
  apiRequest.mockResolvedValue({ status: 200, data: {} });
});
afterEach(() => {
  vi.unstubAllGlobals();
});

describe('useAccountState refresh', () => {
  it('reflects a signed-in session and loads the profile', async () => {
    checkSession.mockResolvedValue({ sessionToken: 't' });
    mockProfile();
    const account = useAccountState();
    expect(account.checked.value).toBe(false);

    await account.refresh();

    expect(account.checked.value).toBe(true);
    expect(account.isSignedIn.value).toBe(true);
    expect(account.displayName.value).toBe('Ada Lovelace');
    expect(account.email.value).toBe('ada@example.com');
    expect(account.avatarUrl.value).toBe('https://example.com/a.png');
    expect(account.initials.value).toBe('AD');
  });

  it('reflects a missing session without fetching a profile', async () => {
    const account = useAccountState();
    await account.refresh();

    expect(account.checked.value).toBe(true);
    expect(account.isSignedIn.value).toBe(false);
    expect(apiRequest).not.toHaveBeenCalled();
  });

  it('never throws, and treats a failed check as signed out', async () => {
    checkSession.mockResolvedValue({ sessionToken: 't' });
    mockProfile();
    const account = useAccountState();
    await account.refresh();

    vi.spyOn(console, 'warn').mockImplementation(() => {});
    checkSession.mockRejectedValue(new Error('offline'));
    await expect(account.refresh()).resolves.toBeUndefined();

    expect(account.isSignedIn.value).toBe(false);
    expect(account.displayName.value).toBe('');
    expect(account.avatarUrl.value).toBe('');
  });

  it('shares one in-flight refresh between concurrent calls', async () => {
    checkSession.mockResolvedValue({ sessionToken: 't' });
    mockProfile();
    const account = useAccountState();

    await Promise.all([account.refresh(), account.refresh(), account.refresh()]);

    expect(checkSession).toHaveBeenCalledTimes(1);
    expect(profileFetches()).toBe(1);

    // Once it settles, the next call reads fresh state again.
    await account.refresh();
    expect(checkSession).toHaveBeenCalledTimes(2);
  });

  it('drops a refresh that started before a sign-out', async () => {
    let resolveCheck!: (v: unknown) => void;
    checkSession.mockReturnValueOnce(new Promise((resolve) => (resolveCheck = resolve)));
    mockProfile();
    const account = useAccountState();
    const stale = account.refresh();

    await account.signOut();
    resolveCheck({ sessionToken: 't' });
    await stale;

    expect(account.isSignedIn.value).toBe(false);
    expect(account.displayName.value).toBe('');
  });

  it('reset forgets the account without asking the backend', async () => {
    checkSession.mockResolvedValue({ sessionToken: 't' });
    mockProfile();
    const account = useAccountState();
    await account.refresh();
    checkSession.mockClear();

    account.reset();

    expect(account.isSignedIn.value).toBe(false);
    expect(account.checked.value).toBe(false);
    expect(account.email.value).toBe('');
    expect(checkSession).not.toHaveBeenCalled();
  });
});

describe('useAccountState signIn', () => {
  it.each([
    ['google', googleSignIn, microsoftSignIn],
    ['microsoft', microsoftSignIn, googleSignIn],
  ] as const)('%s runs its own flow, syncs notifications, and loads the account', async (provider, used, unused) => {
    checkSession.mockResolvedValue({ sessionToken: 't' });
    mockProfile();
    const account = useAccountState();

    const result = await account.signIn(provider);

    expect(result).toEqual({ success: true });
    expect(used).toHaveBeenCalledTimes(1);
    expect(unused).not.toHaveBeenCalled();
    expect(emitNotificationsSync).toHaveBeenCalledTimes(1);
    expect(account.isSignedIn.value).toBe(true);
    expect(account.email.value).toBe('ada@example.com');
    expect(account.signingInWith.value).toBeNull();
  });

  it('marks the pending provider until the flow resolves', async () => {
    let resolveSignIn!: (r: SignInResult) => void;
    microsoftSignIn.mockReturnValue(new Promise((resolve) => (resolveSignIn = resolve)));
    const account = useAccountState();

    const pending = account.signIn('microsoft');
    expect(account.signingInWith.value).toBe('microsoft');

    resolveSignIn({ error: 'Sign-in canceled' });
    await pending;
    expect(account.signingInWith.value).toBeNull();
  });

  it('stays silent on a cancel', async () => {
    googleSignIn.mockResolvedValue({ error: 'Sign-in canceled' });
    const account = useAccountState();

    const result = await account.signIn('google');

    expect(result.error).toBe('Sign-in canceled');
    expect(account.errorMessage.value).toBe('');
    expect(account.isSignedIn.value).toBe(false);
    expect(emitNotificationsSync).not.toHaveBeenCalled();
    expect(checkSession).not.toHaveBeenCalled();
  });

  it('surfaces a failure as the error message', async () => {
    microsoftSignIn.mockResolvedValue({ error: 'API returned 500' });
    const account = useAccountState();

    await account.signIn('microsoft');

    expect(account.errorMessage.value).toBe('API returned 500');
    expect(emitNotificationsSync).not.toHaveBeenCalled();
  });

  it('turns a thrown sign-in into an error result', async () => {
    googleSignIn.mockRejectedValue(new Error('listener setup failed'));
    const account = useAccountState();

    const result = await account.signIn('google');

    expect(result).toEqual({ error: 'listener setup failed' });
    expect(account.errorMessage.value).toBe('listener setup failed');
    expect(account.signingInWith.value).toBeNull();
  });

  it('fetches the profile once when the sign-in and its broadcast both refresh', async () => {
    // The backend broadcasts AUTH_CHANGED_EVENT as it stores the token, just
    // before the sign-in result reaches this window.
    let resolveSignIn!: (r: SignInResult) => void;
    googleSignIn.mockReturnValue(new Promise((resolve) => (resolveSignIn = resolve)));
    checkSession.mockResolvedValue({ sessionToken: 't' });
    mockProfile();
    const account = useAccountState();

    const signingIn = account.signIn('google');
    const onBroadcast = account.refresh();
    resolveSignIn({ success: true });
    await Promise.all([signingIn, onBroadcast]);

    expect(profileFetches()).toBe(1);
  });
});

describe('useAccountState signOut and cancel', () => {
  it('signOut clears the account and syncs notifications', async () => {
    checkSession.mockResolvedValue({ sessionToken: 't' });
    mockProfile();
    const account = useAccountState();
    await account.refresh();

    await account.signOut();

    expect(signOut).toHaveBeenCalledTimes(1);
    expect(account.isSignedIn.value).toBe(false);
    expect(account.displayName.value).toBe('');
    expect(account.email.value).toBe('');
    expect(account.avatarUrl.value).toBe('');
    expect(emitNotificationsSync).toHaveBeenCalledTimes(1);
  });

  it('cancelSignIn asks the backend and swallows a failure', async () => {
    cancelSignIn.mockRejectedValue(new Error('nothing pending'));
    const account = useAccountState();

    await expect(account.cancelSignIn()).resolves.toBeUndefined();
    await flushPromises();
    expect(cancelSignIn).toHaveBeenCalledTimes(1);
  });
});
