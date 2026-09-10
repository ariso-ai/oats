import { describe, it, expect, vi, beforeEach } from 'vitest';

// The sign-in commands return immediately and deliver the session over the
// "oauth-result" event once the browser round-trip completes, so the mock
// fires the listener rather than resolving the invoke call with the result.
const invoke = vi.fn();
const hoisted = vi.hoisted(() => ({
  emit: null as null | ((payload: unknown) => void),
  listenedTo: [] as string[],
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock('@tauri-apps/api/webviewWindow', () => ({
  getCurrentWebviewWindow: () => ({
    listen: (event: string, cb: (e: { payload: unknown }) => void) => {
      hoisted.listenedTo.push(event);
      hoisted.emit = (payload: unknown) => cb({ payload });
      return Promise.resolve(() => {
        hoisted.emit = null;
      });
    },
  }),
}));

import { auth } from './tauri';

beforeEach(() => {
  vi.clearAllMocks();
  hoisted.emit = null;
  hoisted.listenedTo = [];
});

/** The command starts the browser flow; the result arrives on oauth-result. */
function mockBrowserFlow(result: unknown) {
  invoke.mockImplementation(() => {
    queueMicrotask(() => hoisted.emit?.(result));
    return Promise.resolve({});
  });
}

describe('auth.microsoftSignIn', () => {
  it('invokes microsoft_sign_in and resolves from oauth-result', async () => {
    mockBrowserFlow({ success: true, sessionToken: 't' });

    const result = await auth.microsoftSignIn();

    expect(invoke).toHaveBeenCalledWith('microsoft_sign_in');
    expect(hoisted.listenedTo).toEqual(['oauth-result']);
    expect(result).toEqual({ success: true, sessionToken: 't' });
  });

  it('returns an immediate command error and stops listening', async () => {
    invoke.mockResolvedValue({ error: 'API returned 500' });

    await expect(auth.microsoftSignIn()).resolves.toEqual({ error: 'API returned 500' });
    expect(hoisted.emit).toBeNull();
  });
});

describe('auth.googleSignIn', () => {
  it('invokes google_sign_in and resolves from oauth-result', async () => {
    mockBrowserFlow({ success: true, sessionToken: 'g' });

    const result = await auth.googleSignIn();

    expect(invoke).toHaveBeenCalledWith('google_sign_in');
    expect(result).toEqual({ success: true, sessionToken: 'g' });
  });
});

describe('auth.cancelSignIn', () => {
  it('invokes the provider-neutral cancel_sign_in', async () => {
    invoke.mockResolvedValue(undefined);

    await auth.cancelSignIn();

    expect(invoke).toHaveBeenCalledWith('cancel_sign_in');
  });
});
