import { describe, it, expect } from 'vitest';
import { localRecordingIdFromStart, timestampFromLocalRecordingId } from './localRecordingId';

// These mirror the Rust sanitize_iso_to_id tests in src-tauri/src/storage.rs —
// the two implementations must agree or the library pins UI to the wrong row.
describe('localRecordingIdFromStart', () => {
  it('sanitizes an ISO timestamp with millis', () => {
    expect(localRecordingIdFromStart('2026-06-02T14:30:05.123Z')).toBe('2026-06-02T14-30-05Z');
  });

  it('sanitizes an ISO timestamp without millis', () => {
    expect(localRecordingIdFromStart('2026-06-02T14:30:05Z')).toBe('2026-06-02T14-30-05Z');
  });

  it('drops a +HH:MM offset', () => {
    expect(localRecordingIdFromStart('2026-06-02T14:30:05+00:00')).toBe('2026-06-02T14-30-05Z');
  });
});

describe('timestampFromLocalRecordingId', () => {
  it('round-trips a sanitized id back to seconds-precision ISO', () => {
    expect(timestampFromLocalRecordingId('2026-06-02T14-30-05Z')).toBe('2026-06-02T14:30:05Z');
  });

  it('returns null for non-local ids (e.g. numeric Ariso meeting ids)', () => {
    expect(timestampFromLocalRecordingId('42')).toBeNull();
    expect(timestampFromLocalRecordingId('not-a-recording')).toBeNull();
  });
});
