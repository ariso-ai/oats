import { load } from '@tauri-apps/plugin-store';
import { getBackendSetting, getDesktopConfig } from '../tauri';
import { loadPlatformCapabilities } from './usePlatformCapabilities';

// Opt-in crash/error diagnostics (issue #260). Three independent gates must all
// pass before a single byte leaves the machine:
//
//   1. `diagnosticsEnabled` in settings.json is explicitly true (default: OFF).
//   2. The active backend is 'ariso'. Offline/on-device mode promises nothing
//      leaves the device, so the toggle is inert there — see oats-security.
//   3. The build shipped a Sentry DSN (`sentryDsn` from get_desktop_config).
//
// Gates 1 and 2 are re-checked on every report so flipping the toggle off (or
// switching to offline mode) takes effect immediately, without a restart.

const SETTINGS_PATH = 'settings.json';
const ENABLED_KEY = 'diagnosticsEnabled';

/** Whether the user opted into diagnostics. Defaults to false. */
export async function isDiagnosticsEnabled(): Promise<boolean> {
  const store = await load(SETTINGS_PATH, { autoSave: true });
  return (await store.get<boolean>(ENABLED_KEY)) === true;
}

/** Persist the opt-in flag. */
export async function setDiagnosticsEnabled(enabled: boolean): Promise<void> {
  const store = await load(SETTINGS_PATH, { autoSave: true });
  await store.set(ENABLED_KEY, enabled);
}

/** Which leg of the three-step upload failed. `unknown` is the honest answer
 *  for an error that reached us without a stage — filing those under a real
 *  stage would poison that stage's issue group. */
export type UploadStage = 'presign' | 's3-put' | 'confirm' | 'combine' | 'buffer' | 'unknown';

/** Error carrying the upload leg that produced it, so a report can be grouped
 *  by stage instead of by the (highly variable) message text. */
export class UploadStageError extends Error {
  readonly stage: UploadStage;
  readonly httpStatus: number | null;

  constructor(stage: UploadStage, message: string, httpStatus: number | null = null) {
    super(message);
    this.name = 'UploadStageError';
    this.stage = stage;
    this.httpStatus = httpStatus;
  }
}

export interface UploadFailureContext {
  /** 'initial' = right after recording; 'retry' = the Pending uploads button. */
  attempt: 'initial' | 'retry';
  /** Falls back to the stage carried by an UploadStageError. */
  stage?: UploadStage;
  /** Size of the mp3 we tried to push, in bytes. */
  bytes?: number;
  durationSeconds?: number;
  /** How many buffered recordings a retry combined. */
  itemCount?: number;
  /** Whether the upload targeted an existing meeting rather than a new one. */
  hasMeetingId?: boolean;
  /** Whether an Ari session existed when the attempt started. */
  signedIn?: boolean;
}

// A failed presigned PUT surfaces reqwest's error text, which embeds the full
// S3 URL — query string included, and that query string *is* the AWS signature.
// Redact any URL before it reaches Sentry. Also strip bare `?...` tails so a
// half-logged URL can't leak credentials either.
const URL_PATTERN = /\b[a-z][a-z0-9+.-]*:\/\/\S+/gi;
// The scheme-anchored pattern above misses a URL that arrives without one (a
// truncated or reformatted error string), and the query tail is the half that
// carries the signature. Anything shaped like `?k=v` goes too.
const QUERY_TAIL_PATTERN = /\?[^\s)]*=[^\s)]*/g;

export function scrubMessage(message: string): string {
  const withoutUrls = message.replace(URL_PATTERN, (url) => {
    try {
      const parsed = new URL(url);
      return `${parsed.protocol}//${parsed.host}/<redacted>`;
    } catch {
      return '<redacted-url>';
    }
  });
  return withoutUrls.replace(QUERY_TAIL_PATTERN, '?<redacted>');
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  return String(error ?? 'Unknown error');
}

/** Best-effort HTTP status pulled out of an error message like "… (503)".
 *  Scans the *scrubbed* message: a presigned URL's `X-Amz-Expires=432` would
 *  otherwise read as a status and split one issue across two fingerprints. */
function inferHttpStatus(error: unknown): number | null {
  if (error instanceof UploadStageError && error.httpStatus != null) return error.httpStatus;
  const match = /\b([45]\d{2})\b/.exec(scrubMessage(errorMessage(error)));
  return match ? Number(match[1]) : null;
}

// macOS and Windows ship the same bundle to the same Sentry project, so the OS
// has to be tagged to tell their issues apart. Nothing else carries it:
// `defaultIntegrations: false` drops the integration that would attach a
// User-Agent header, and `beforeSend` deletes `event.request` anyway, leaving
// Sentry nothing to infer the platform from. A capability lookup that fails
// costs the tag, never the report.
async function platformOsTag(): Promise<string> {
  try {
    return (await loadPlatformCapabilities()).os;
  } catch {
    return 'unknown';
  }
}

type SentryModule = typeof import('@sentry/browser');

let clientPromise: Promise<SentryModule | null> | null = null;

/** Load + initialize Sentry once per window. Resolves to null when this build
 *  has no DSN. The user-facing gates are checked by the caller, not here. */
function ensureClient(): Promise<SentryModule | null> {
  if (!clientPromise) {
    clientPromise = (async () => {
      const { sentryDsn } = await getDesktopConfig();
      if (!sentryDsn) return null;
      const Sentry = await import('@sentry/browser');
      Sentry.init({
        dsn: sentryDsn,
        release: __APP_VERSION__,
        initialScope: { tags: { os: await platformOsTag() } },
        // Everything is reported by an explicit captureException call. Default
        // integrations would add global error handlers plus console/DOM/fetch
        // breadcrumbs, which in this app can carry meeting content and
        // presigned URLs. Opting into diagnostics must not opt into that.
        defaultIntegrations: false,
        integrations: [],
        sendDefaultPii: false,
        beforeSend(event) {
          for (const value of event.exception?.values ?? []) {
            if (value.value) value.value = scrubMessage(value.value);
          }
          if (event.message) event.message = scrubMessage(event.message);
          delete event.request;
          delete event.user;
          return event;
        },
      });
      return Sentry;
    })().catch(() => {
      // A config IPC hiccup or a failed chunk fetch is transient. Memoizing the
      // failure would silence diagnostics for the window's whole life, so drop
      // the memo and let the next report try again.
      clientPromise = null;
      return null;
    });
  }
  return clientPromise;
}

/** All three gates. Exported for the Settings toggle's live status copy. */
export async function diagnosticsAllowed(): Promise<boolean> {
  if (!(await isDiagnosticsEnabled())) return false;
  return (await getBackendSetting()) === 'ariso';
}

/** Report a failed recording upload. Never throws and never rejects —
 *  diagnostics must not be able to break the upload's own error handling. */
export async function reportUploadFailure(
  error: unknown,
  context: UploadFailureContext
): Promise<void> {
  try {
    if (!(await diagnosticsAllowed())) return;
    const Sentry = await ensureClient();
    if (!Sentry) return;

    const stage = context.stage ?? (error instanceof UploadStageError ? error.stage : 'unknown');
    const httpStatus = inferHttpStatus(error);
    const reported =
      error instanceof Error ? error : new Error(errorMessage(error));

    Sentry.captureException(reported, {
      tags: {
        feature: 'recording-upload',
        upload_stage: stage,
        upload_attempt: context.attempt,
        http_status: httpStatus == null ? 'none' : String(httpStatus),
      },
      contexts: {
        upload: {
          stage,
          attempt: context.attempt,
          httpStatus,
          bytes: context.bytes ?? null,
          durationSeconds: context.durationSeconds ?? null,
          itemCount: context.itemCount ?? null,
          hasMeetingId: context.hasMeetingId ?? null,
          signedIn: context.signedIn ?? null,
        },
      },
      // Group by what failed, not by the message — retry failures for the same
      // stage/status should land in one issue rather than one per attempt.
      fingerprint: ['recording-upload', stage, httpStatus == null ? 'none' : String(httpStatus)],
    });
  } catch {
    // Swallowed on purpose: see the doc comment.
  }
}

/** Test-only: drop the memoized client so each test re-initializes. */
export function resetDiagnosticsClientForTest(): void {
  clientPromise = null;
}
