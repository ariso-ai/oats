import { describe, it, expect, vi, beforeEach } from 'vitest';

const invoke = vi.fn(() => Promise.resolve({ backend: 'local', id: 'x', title: 't', status: 'done' }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { local } from './tauri';

beforeEach(() => vi.clearAllMocks());

describe('local.finalizeRecording', () => {
  it('forwards appendTo to the local_finalize_recording command', async () => {
    await local.finalizeRecording([1, 2], 'Title', '2026-06-02T10:00:00Z', 12, '2026-06-01T09-00-00Z');
    expect(invoke).toHaveBeenCalledWith('local_finalize_recording', {
      audio: [1, 2],
      title: 'Title',
      createdAt: '2026-06-02T10:00:00Z',
      durationSeconds: 12,
      appendTo: '2026-06-01T09-00-00Z',
    });
  });

  it('omits appendTo (undefined) for a normal new recording', async () => {
    await local.finalizeRecording([1], 'T', '2026-06-02T10:00:00Z', 5);
    expect(invoke).toHaveBeenCalledWith('local_finalize_recording', {
      audio: [1], title: 'T', createdAt: '2026-06-02T10:00:00Z', durationSeconds: 5, appendTo: undefined,
    });
  });
});
