import { describe, it, expect, vi, beforeEach } from 'vitest';

const storeGet = vi.fn();
const storeSet = vi.fn(() => Promise.resolve());
const getBackendSetting = vi.fn();
const getDesktopConfig = vi.fn();
const sentryInit = vi.fn();
const captureException = vi.fn();

vi.mock('@tauri-apps/plugin-store', () => ({
  load: () => Promise.resolve({ get: storeGet, set: storeSet }),
}));
vi.mock('../tauri', () => ({
  getBackendSetting: (...a: unknown[]) => getBackendSetting(...a),
  getDesktopConfig: (...a: unknown[]) => getDesktopConfig(...a),
}));
vi.mock('@sentry/browser', () => ({
  init: (...a: unknown[]) => sentryInit(...a),
  captureException: (...a: unknown[]) => captureException(...a),
}));

import {
  isDiagnosticsEnabled,
  setDiagnosticsEnabled,
  diagnosticsAllowed,
  reportUploadFailure,
  resetDiagnosticsClientForTest,
  scrubMessage,
  UploadStageError,
} from './useDiagnostics';

beforeEach(() => {
  vi.clearAllMocks();
  resetDiagnosticsClientForTest();
  storeGet.mockResolvedValue(true);
  getBackendSetting.mockResolvedValue('ariso');
  getDesktopConfig.mockResolvedValue({ sentryDsn: 'https://key@sentry.io/1' });
});

describe('the diagnostics opt-in', () => {
  it('defaults to off when unset', async () => {
    storeGet.mockResolvedValue(undefined);
    expect(await isDiagnosticsEnabled()).toBe(false);
  });

  it('is on only when explicitly true', async () => {
    storeGet.mockResolvedValue('yes');
    expect(await isDiagnosticsEnabled()).toBe(false);
    storeGet.mockResolvedValue(true);
    expect(await isDiagnosticsEnabled()).toBe(true);
  });

  it('persists the flag on set', async () => {
    await setDiagnosticsEnabled(true);
    expect(storeSet).toHaveBeenCalledWith('diagnosticsEnabled', true);
  });
});

describe('diagnosticsAllowed', () => {
  it('is false when the user has not opted in', async () => {
    storeGet.mockResolvedValue(false);
    expect(await diagnosticsAllowed()).toBe(false);
  });

  it('is false in offline mode even when opted in', async () => {
    getBackendSetting.mockResolvedValue('local');
    expect(await diagnosticsAllowed()).toBe(false);
  });

  it('is true when opted in on the Ariso backend', async () => {
    expect(await diagnosticsAllowed()).toBe(true);
  });
});

describe('scrubMessage', () => {
  it('strips the signature-bearing query string off a presigned URL', () => {
    const raw =
      'error sending request for url (https://bucket.s3.amazonaws.com/rec.mp3?X-Amz-Signature=deadbeef&X-Amz-Credential=AKIA)';
    const scrubbed = scrubMessage(raw);
    expect(scrubbed).not.toContain('X-Amz-Signature');
    expect(scrubbed).not.toContain('AKIA');
    expect(scrubbed).toContain('bucket.s3.amazonaws.com');
  });

  it('leaves URL-free messages alone', () => {
    expect(scrubMessage('S3 upload failed (403)')).toBe('S3 upload failed (403)');
  });
});

describe('reportUploadFailure', () => {
  it('sends nothing when diagnostics are off', async () => {
    storeGet.mockResolvedValue(false);
    await reportUploadFailure(new Error('boom'), { attempt: 'initial' });
    expect(sentryInit).not.toHaveBeenCalled();
    expect(captureException).not.toHaveBeenCalled();
  });

  it('sends nothing in offline mode', async () => {
    getBackendSetting.mockResolvedValue('local');
    await reportUploadFailure(new Error('boom'), { attempt: 'initial' });
    expect(captureException).not.toHaveBeenCalled();
  });

  it('sends nothing when the build ships no DSN', async () => {
    getDesktopConfig.mockResolvedValue({ sentryDsn: '' });
    await reportUploadFailure(new Error('boom'), { attempt: 'initial' });
    expect(sentryInit).not.toHaveBeenCalled();
    expect(captureException).not.toHaveBeenCalled();
  });

  it('initializes Sentry with no default integrations and no PII', async () => {
    await reportUploadFailure(new Error('boom'), { attempt: 'initial' });
    const options = sentryInit.mock.calls[0][0];
    expect(options.dsn).toBe('https://key@sentry.io/1');
    expect(options.defaultIntegrations).toBe(false);
    expect(options.integrations).toEqual([]);
    expect(options.sendDefaultPii).toBe(false);
  });

  it('initializes only once across reports', async () => {
    await reportUploadFailure(new Error('one'), { attempt: 'initial' });
    await reportUploadFailure(new Error('two'), { attempt: 'retry' });
    expect(sentryInit).toHaveBeenCalledTimes(1);
    expect(captureException).toHaveBeenCalledTimes(2);
  });

  it('tags the stage and status carried by an UploadStageError', async () => {
    await reportUploadFailure(new UploadStageError('s3-put', 'S3 upload failed (403)', 403), {
      attempt: 'retry',
      bytes: 2048,
      itemCount: 3,
    });
    const [error, hint] = captureException.mock.calls[0];
    expect((error as Error).message).toBe('S3 upload failed (403)');
    expect(hint.tags).toMatchObject({
      feature: 'recording-upload',
      upload_stage: 's3-put',
      upload_attempt: 'retry',
      http_status: '403',
    });
    expect(hint.contexts.upload).toMatchObject({
      stage: 's3-put',
      attempt: 'retry',
      httpStatus: 403,
      bytes: 2048,
      itemCount: 3,
    });
    expect(hint.fingerprint).toEqual(['recording-upload', 's3-put', '403']);
  });

  it('lets an explicit context stage override the error', async () => {
    await reportUploadFailure(new Error('buffer missing'), {
      attempt: 'retry',
      stage: 'combine',
    });
    expect(captureException.mock.calls[0][1].tags.upload_stage).toBe('combine');
  });

  it('infers a status code from a plain error message', async () => {
    await reportUploadFailure(new Error('Failed to confirm audio upload (502)'), {
      attempt: 'initial',
    });
    expect(captureException.mock.calls[0][1].tags.http_status).toBe('502');
  });

  it('reports a non-Error rejection without throwing', async () => {
    await reportUploadFailure('invoke rejected', { attempt: 'initial' });
    expect((captureException.mock.calls[0][0] as Error).message).toBe('invoke rejected');
  });

  it('swallows its own failures so uploads keep their error handling', async () => {
    captureException.mockImplementation(() => {
      throw new Error('sentry exploded');
    });
    await expect(
      reportUploadFailure(new Error('boom'), { attempt: 'initial' })
    ).resolves.toBeUndefined();
  });
});

describe('the Sentry beforeSend hook', () => {
  async function beforeSend() {
    await reportUploadFailure(new Error('boom'), { attempt: 'initial' });
    return sentryInit.mock.calls[0][0].beforeSend;
  }

  it('scrubs URLs out of exception values and drops request/user data', async () => {
    const hook = await beforeSend();
    const event = hook({
      exception: {
        values: [{ value: 'error sending request for url (https://b.s3.amazonaws.com/a?sig=xyz)' }],
      },
      message: 'see https://b.s3.amazonaws.com/a?sig=xyz',
      request: { url: 'https://b.s3.amazonaws.com/a?sig=xyz' },
      user: { email: 'someone@example.com' },
    });
    expect(event.exception.values[0].value).not.toContain('sig=xyz');
    expect(event.message).not.toContain('sig=xyz');
    expect(event.request).toBeUndefined();
    expect(event.user).toBeUndefined();
  });
});
