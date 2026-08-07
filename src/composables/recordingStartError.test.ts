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

  it('does not describe a selected-device failure as a generic missing microphone', () => {
    const error = Object.assign(new Error('selected input unavailable'), {
      name: 'SelectedAudioInputUnavailableError',
    });
    expect(recordingStartErrorMessage(error)).toContain(
      'selected microphone is unavailable',
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
