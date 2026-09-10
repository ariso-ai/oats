import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const getMeetingNotes = vi.fn();

vi.mock('./useMeetingApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./useMeetingApi')>();
  return {
    ...actual,
    useMeetingApi: () => ({ getMeetingNotes: (id: unknown) => getMeetingNotes(id) }),
  };
});

import { useMeetingProcessing } from './useMeetingProcessing';

const processing = useMeetingProcessing();

// Advance past one 5s poll interval and let the in-flight requests settle.
async function tick(): Promise<void> {
  await vi.advanceTimersByTimeAsync(5000);
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.useFakeTimers();
  processing.reset();
  processing.version.value = 0;
});
afterEach(() => {
  processing.reset();
  vi.useRealTimers();
});

describe('useMeetingProcessing', () => {
  it('tracks a meeting once marked uploaded', () => {
    expect(processing.isProcessing('42')).toBe(false);
    processing.markUploaded('42');
    expect(processing.isProcessing('42')).toBe(true);
  });

  it('accepts a numeric id and matches it as a string', () => {
    processing.markUploaded(42);
    expect(processing.isProcessing('42')).toBe(true);
  });

  it('does not poll before the interval elapses', async () => {
    processing.markUploaded('42');
    await vi.advanceTimersByTimeAsync(4000);
    expect(getMeetingNotes).not.toHaveBeenCalled();
  });

  it('stops tracking and bumps version once the meeting has a transcript', async () => {
    getMeetingNotes.mockResolvedValue({ hasTranscript: true });
    processing.markUploaded('42');

    await tick();

    expect(getMeetingNotes).toHaveBeenCalledWith('42');
    expect(processing.isProcessing('42')).toBe(false);
    expect(processing.version.value).toBe(1);
  });

  it('stops tracking once the meeting has a summary but no transcript', async () => {
    getMeetingNotes.mockResolvedValue({ summary: '{"digest":"x"}' });
    processing.markUploaded('42');
    await tick();
    expect(processing.isProcessing('42')).toBe(false);
  });

  it('keeps polling while the meeting has no content yet', async () => {
    getMeetingNotes.mockResolvedValue({ hasTranscript: false, summary: null });
    processing.markUploaded('42');

    await tick();
    expect(processing.isProcessing('42')).toBe(true);
    expect(processing.version.value).toBe(0);

    await tick();
    expect(getMeetingNotes).toHaveBeenCalledTimes(2);
    expect(processing.isProcessing('42')).toBe(true);
  });

  it('keeps the meeting tracked when the poll request fails', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    getMeetingNotes.mockRejectedValue(new Error('offline'));
    processing.markUploaded('42');

    await tick();

    expect(processing.isProcessing('42')).toBe(true);
    expect(processing.version.value).toBe(0);
  });

  it('resolves tracked meetings independently', async () => {
    getMeetingNotes.mockImplementation((id: string) =>
      id === '1' ? Promise.resolve({ hasTranscript: true }) : Promise.reject(new Error('offline'))
    );
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    processing.markUploaded('1');
    processing.markUploaded('2');

    await tick();

    expect(processing.isProcessing('1')).toBe(false);
    expect(processing.isProcessing('2')).toBe(true);
    expect(processing.version.value).toBe(1);
  });

  it('stops polling once every tracked meeting is ready', async () => {
    getMeetingNotes.mockResolvedValue({ hasTranscript: true });
    processing.markUploaded('42');
    await tick();
    expect(getMeetingNotes).toHaveBeenCalledTimes(1);

    await tick();
    expect(getMeetingNotes).toHaveBeenCalledTimes(1);
  });

  it('ignores a duplicate mark for an already-tracked meeting', async () => {
    getMeetingNotes.mockResolvedValue({ hasTranscript: false });
    processing.markUploaded('42');
    processing.markUploaded('42');

    await tick();

    expect(getMeetingNotes).toHaveBeenCalledTimes(1);
  });

  it('reset drops tracking and stops polling', async () => {
    getMeetingNotes.mockResolvedValue({ hasTranscript: false });
    processing.markUploaded('42');
    processing.reset();

    expect(processing.isProcessing('42')).toBe(false);
    await tick();
    expect(getMeetingNotes).not.toHaveBeenCalled();
  });
});
