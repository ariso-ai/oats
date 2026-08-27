import { describe, expect, it } from 'vitest';
import {
  recordingBlockedMessage,
  recordingBlockedPayload,
  recordingStartErrorMessage,
} from './recordingStartError';

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

describe('recordingBlockedPayload', () => {
  it('reads the reason and stringifies the numeric meeting id', () => {
    expect(recordingBlockedPayload({ reason: 'uploading', meetingId: 42 })).toEqual({
      reason: 'uploading',
      meetingId: '42',
    });
  });

  it('accepts a blocked start with no identifiable meeting', () => {
    expect(recordingBlockedPayload({ reason: 'capturing', meetingId: null })).toEqual({
      reason: 'capturing',
      meetingId: null,
    });
  });

  it('ignores payloads from every other start failure', () => {
    expect(recordingBlockedPayload({ message: 'No microphone was found.' })).toBeNull();
    expect(recordingBlockedPayload({ reason: 'something-else' })).toBeNull();
    expect(recordingBlockedPayload(null)).toBeNull();
  });
});

describe('recordingBlockedMessage', () => {
  it('names the meeting still recording and says how to unblock it', () => {
    const message = recordingBlockedMessage('capturing', 'Standup');
    expect(message).toContain('Standup');
    expect(message).toContain('still recording');
  });

  it('names the meeting still uploading', () => {
    const message = recordingBlockedMessage('uploading', 'Standup');
    expect(message).toContain('Standup');
    expect(message).toContain('still uploading');
  });

  // The two states are not interchangeable: a pill that is mid-capture is not
  // uploading, and telling the user to wait for an upload that never runs is
  // worse than saying nothing (#320).
  it('does not describe an in-progress recording as an upload', () => {
    expect(recordingBlockedMessage('capturing', 'Standup')).not.toContain('uploading');
    expect(recordingBlockedMessage('capturing', null)).not.toContain('uploading');
  });

  it('stays accurate when the blocking meeting cannot be named', () => {
    expect(recordingBlockedMessage('uploading', null)).toContain('previous recording');
    expect(recordingBlockedMessage('capturing', '  ')).toContain('Another recording');
  });
});
