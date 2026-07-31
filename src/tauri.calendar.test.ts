import { describe, it, expect, vi, beforeEach } from 'vitest';

// ensureCalendarAccess decides whether the second (Workspace connect) OAuth hop
// runs. Firing it when Calendar is already granted would put a consent screen in
// front of the user on every sign-in, which is the exact failure the conditional
// design exists to avoid.

const invoke = vi.fn();
// The real connect_google_calendar returns immediately and delivers its result
// over the "calendar-connect-result" event once the browser hop completes, so
// the mock has to fire the listener rather than resolve the invoke call.
const hoisted = vi.hoisted(() => ({
  emit: null as null | ((payload: unknown) => void),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock('@tauri-apps/api/webviewWindow', () => ({
  getCurrentWebviewWindow: () => ({
    listen: (_event: string, cb: (e: { payload: unknown }) => void) => {
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
});

/** Route api_request to a status payload; connect_google_calendar to a result. */
function mockBackend(opts: {
  status?: unknown;
  statusCode?: number;
  connect?: unknown;
}) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === 'api_request') {
      return Promise.resolve({ status: opts.statusCode ?? 200, data: opts.status });
    }
    if (cmd === 'connect_google_calendar') {
      const payload = opts.connect ?? {};
      // An immediate error short-circuits in the wrapper; anything else arrives
      // on the event after the browser round-trip.
      if (!(payload as { error?: string }).error) {
        queueMicrotask(() => hoisted.emit?.(payload));
      }
      return Promise.resolve(payload);
    }
    return Promise.resolve({});
  });
}

describe('auth.ensureCalendarAccess', () => {
  it('does not run the connect hop when Calendar is already granted', async () => {
    mockBackend({ status: { connected: true } });

    const result = await auth.ensureCalendarAccess();

    expect(result).toEqual({ connected: true });
    expect(invoke).not.toHaveBeenCalledWith('connect_google_calendar');
  });

  it('runs the connect hop when Calendar read scope is missing', async () => {
    mockBackend({
      status: { connected: false, reason: 'no_calendar_scope' },
      connect: { status: 'connected' },
    });

    const result = await auth.ensureCalendarAccess();

    expect(invoke).toHaveBeenCalledWith('connect_google_calendar');
    expect(result).toEqual({ connected: true });
  });

  it('runs the connect hop when no credential exists at all', async () => {
    mockBackend({
      status: { connected: false, reason: 'no_credential' },
      connect: { status: 'connected' },
    });

    await auth.ensureCalendarAccess();

    expect(invoke).toHaveBeenCalledWith('connect_google_calendar');
  });

  it('reports not-connected when the user unticks Calendar on the consent screen', async () => {
    // Granular consent lets the grant complete without Calendar. Claiming
    // success here would contradict the API and re-prompt on the next sign-in.
    mockBackend({
      status: { connected: false, reason: 'no_calendar_scope' },
      connect: { status: 'no_calendar_scope' },
    });

    const result = await auth.ensureCalendarAccess();

    expect(result).toEqual({ connected: false, reason: 'no_calendar_scope' });
  });

  it('surfaces a connect-hop error rather than reporting success', async () => {
    mockBackend({
      status: { connected: false, reason: 'no_server' },
      connect: { error: 'API returned 500' },
    });

    await expect(auth.ensureCalendarAccess()).rejects.toThrow('API returned 500');
  });

  it('throws instead of connecting when the status check fails', async () => {
    mockBackend({ status: null, statusCode: 401 });

    await expect(auth.ensureCalendarAccess()).rejects.toThrow('calendar status: 401');
    expect(invoke).not.toHaveBeenCalledWith('connect_google_calendar');
  });
});
