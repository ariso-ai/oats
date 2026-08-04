import { describe, expect, it } from 'vitest';
import { recordingStartErrorMessage } from './recordingStartError';

describe('recordingStartErrorMessage', () => {
  it('explains how to recover from a blocked microphone', () => {
    expect(recordingStartErrorMessage(new DOMException('', 'NotAllowedError'))).toContain(
      'Enable microphone access',
    );
  });

  it('explains how to recover from a missing microphone', () => {
    expect(recordingStartErrorMessage(new DOMException('', 'NotFoundError'))).toContain(
      'Connect or enable a microphone',
    );
  });

  it('recognizes native Windows system-audio failures', () => {
    expect(recordingStartErrorMessage('initialize WASAPI loopback: no render endpoint')).toContain(
      'output device',
    );
  });

  it('does not expose an unknown raw driver error', () => {
    expect(recordingStartErrorMessage(new Error('driver path C:\\secret'))).not.toContain(
      'C:\\secret',
    );
  });
});
