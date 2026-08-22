import { describe, it, expect, vi, beforeEach } from 'vitest';

const invoke = vi.fn(() => Promise.resolve({ backend: 'local', id: 'x', title: 't', status: 'done' }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { local, pending } from './tauri';

beforeEach(() => vi.clearAllMocks());

/** Mirror of `raw_ipc::meta` in Rust: hex header -> the command's arguments. */
function decodeMetaHeader(options: unknown): Record<string, unknown> {
  const header = (options as { headers: Record<string, string> }).headers['x-oats-meta'];
  const bytes = new Uint8Array(header.match(/../g)!.map((h) => parseInt(h, 16)));
  return JSON.parse(new TextDecoder().decode(bytes));
}

describe('local.finalizeRecording', () => {
  // The whole point of the raw-body shape: Tauri only skips JSON serialization
  // when the bytes ARE the args payload. Nesting them in an object costs ~15 s
  // for a 48 MB recording and freezes the recorder pill while it serializes.
  it('sends the audio as the raw args payload, not a JSON-serializable value', async () => {
    const audio = new Uint8Array([1, 2]);
    await local.finalizeRecording(audio, 'Title', '2026-06-02T10:00:00Z', 12);
    const [cmd, payload] = invoke.mock.calls[0];
    expect(cmd).toBe('local_finalize_recording');
    expect(payload).toBe(audio);
    expect(ArrayBuffer.isView(payload)).toBe(true);
  });

  it('forwards appendTo to the local_finalize_recording command', async () => {
    await local.finalizeRecording(
      new Uint8Array([1, 2]),
      'Title',
      '2026-06-02T10:00:00Z',
      12,
      '2026-06-01T09-00-00Z'
    );
    expect(decodeMetaHeader(invoke.mock.calls[0][2])).toEqual({
      title: 'Title',
      createdAt: '2026-06-02T10:00:00Z',
      durationSeconds: 12,
      appendTo: '2026-06-01T09-00-00Z',
    });
  });

  it('omits appendTo (undefined) for a normal new recording', async () => {
    await local.finalizeRecording(new Uint8Array([1]), 'T', '2026-06-02T10:00:00Z', 5);
    const meta = decodeMetaHeader(invoke.mock.calls[0][2]);
    expect(meta).toEqual({ title: 'T', createdAt: '2026-06-02T10:00:00Z', durationSeconds: 5 });
    expect('appendTo' in meta).toBe(false);
  });

  // Header values must be visible ASCII, and titles are locale-derived — hence
  // the hex encoding rather than raw JSON.
  it('carries a non-ASCII title through the header intact', async () => {
    await local.finalizeRecording(new Uint8Array([1]), '打ち合わせ ☕', '2026-06-02T10:00:00Z', 5);
    const header = (invoke.mock.calls[0][2] as { headers: Record<string, string> })
      .headers['x-oats-meta'];
    expect(header).toMatch(/^[0-9a-f]+$/);
    expect(decodeMetaHeader(invoke.mock.calls[0][2]).title).toBe('打ち合わせ ☕');
  });
});

describe('pending.bufferAudio', () => {
  it('sends the audio as the raw args payload with meta in the header', async () => {
    const audio = new Uint8Array([9]);
    const meta = {
      createdAt: '2026-06-02T14:30:05.000Z',
      startAt: '2026-06-02T14:30:05.000Z',
      endAt: '2026-06-02T15:10:00.000Z',
      durationSeconds: 2400,
    };
    await pending.bufferAudio(audio, meta);
    const [cmd, payload, options] = invoke.mock.calls[0];
    expect(cmd).toBe('buffer_pending_audio');
    expect(payload).toBe(audio);
    expect(decodeMetaHeader(options)).toEqual({ meta });
  });
});
