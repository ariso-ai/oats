// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const recordingStatus = vi.fn();
const retryTranscription = vi.fn();
const retryNotes = vi.fn();

vi.mock('../tauri', () => ({
  local: {
    recordingStatus: (id: string) => recordingStatus(id),
    retryTranscription: (id: string) => retryTranscription(id),
    retryNotes: (id: string) => retryNotes(id),
  },
}));

import { deriveStage, useLocalRecordingProgress } from './useLocalRecordingProgress';
import type { RecordingStatusView } from '../tauri';

function view(over: Partial<RecordingStatusView> = {}): RecordingStatusView {
  return { status: 'done', hasTranscript: false, hasNote: false, notesStatus: 'pending', ...over };
}

describe('deriveStage', () => {
  it('returns idle for null', () => {
    expect(deriveStage(null)).toBe('idle');
  });
  it('maps recording/transcribing to transcribing', () => {
    expect(deriveStage(view({ status: 'recording' }))).toBe('transcribing');
    expect(deriveStage(view({ status: 'transcribing' }))).toBe('transcribing');
  });
  it('maps failed status to transcript-failed', () => {
    expect(deriveStage(view({ status: 'failed' }))).toBe('transcript-failed');
  });
  it('maps done+notes pending to notes-pending', () => {
    expect(deriveStage(view({ status: 'done', hasTranscript: true, notesStatus: 'pending' }))).toBe('notes-pending');
  });
  it('maps done+notes failed to notes-failed', () => {
    expect(deriveStage(view({ status: 'done', hasTranscript: true, notesStatus: 'failed' }))).toBe('notes-failed');
  });
  it('maps done+note ready to ready', () => {
    expect(deriveStage(view({ status: 'done', hasTranscript: true, hasNote: true, notesStatus: 'ready' }))).toBe('ready');
  });
});

describe('useLocalRecordingProgress polling', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('polls until terminal and exposes the derived stage', async () => {
    recordingStatus
      .mockResolvedValueOnce(view({ status: 'transcribing' }))
      .mockResolvedValueOnce(view({ status: 'done', hasTranscript: true, notesStatus: 'pending' }))
      .mockResolvedValueOnce(view({ status: 'done', hasTranscript: true, hasNote: true, notesStatus: 'ready' }));

    const p = useLocalRecordingProgress(() => 'rec-1');
    p.begin();

    await vi.advanceTimersByTimeAsync(0);
    expect(p.stage.value).toBe('transcribing');

    await vi.advanceTimersByTimeAsync(2000);
    expect(p.stage.value).toBe('notes-pending');
    expect(p.hasTranscript.value).toBe(true);

    await vi.advanceTimersByTimeAsync(2000);
    expect(p.stage.value).toBe('ready');
    expect(p.hasNote.value).toBe(true);

    // Terminal: no further polls scheduled.
    const calls = recordingStatus.mock.calls.length;
    await vi.advanceTimersByTimeAsync(4000);
    expect(recordingStatus.mock.calls.length).toBe(calls);
  });

  it('stops polling at a failed stage', async () => {
    recordingStatus.mockResolvedValue(view({ status: 'failed' }));
    const p = useLocalRecordingProgress(() => 'rec-1');
    p.begin();
    await vi.advanceTimersByTimeAsync(0);
    expect(p.stage.value).toBe('transcript-failed');
    const calls = recordingStatus.mock.calls.length;
    await vi.advanceTimersByTimeAsync(4000);
    expect(recordingStatus.mock.calls.length).toBe(calls);
  });

  it('retryTranscription optimistically shows transcribing, calls the binding, and resumes polling', async () => {
    recordingStatus.mockResolvedValue(view({ status: 'transcribing' }));
    retryTranscription.mockResolvedValue({ backend: 'local', id: 'rec-1', title: 'T', status: 'done' });
    const p = useLocalRecordingProgress(() => 'rec-1');

    void p.retryTranscription();
    // Optimistic state is set synchronously before any await resolves.
    expect(p.stage.value).toBe('transcribing');
    await vi.advanceTimersByTimeAsync(0);
    expect(retryTranscription).toHaveBeenCalledWith('rec-1');
    expect(recordingStatus).toHaveBeenCalledWith('rec-1');
  });

  it('retryNotes optimistically shows notes-pending and calls the binding', async () => {
    recordingStatus.mockResolvedValue(view({ status: 'done', hasTranscript: true, notesStatus: 'pending' }));
    retryNotes.mockResolvedValue(undefined);
    const p = useLocalRecordingProgress(() => 'rec-1');

    void p.retryNotes();
    expect(p.stage.value).toBe('notes-pending');
    await vi.advanceTimersByTimeAsync(0);
    expect(retryNotes).toHaveBeenCalledWith('rec-1');
  });

  it('does not poll until the retry binding resolves (no stale-terminal regression)', async () => {
    let resolveRetry: (() => void) | null = null;
    retryTranscription.mockImplementation(
      () => new Promise<void>((resolve) => { resolveRetry = () => resolve(); })
    );
    // If a poll DID run before the retry resolved, it would read this terminal view.
    recordingStatus.mockResolvedValue(view({ status: 'failed' }));

    const p = useLocalRecordingProgress(() => 'rec-1');
    void p.retryTranscription();

    // Optimistic stage is shown and no poll has happened while the RPC is in flight.
    expect(p.stage.value).toBe('transcribing');
    await vi.advanceTimersByTimeAsync(2000);
    expect(recordingStatus).not.toHaveBeenCalled();
    expect(p.stage.value).toBe('transcribing');

    // Once the RPC resolves, polling starts.
    resolveRetry!();
    await vi.advanceTimersByTimeAsync(0);
    expect(recordingStatus).toHaveBeenCalledWith('rec-1');
  });

  it('reset clears state and stops polling', async () => {
    recordingStatus.mockResolvedValue(view({ status: 'transcribing' }));
    const p = useLocalRecordingProgress(() => 'rec-1');
    p.begin();
    await vi.advanceTimersByTimeAsync(0);
    expect(p.stage.value).toBe('transcribing');

    p.reset();
    expect(p.stage.value).toBe('idle');
    const calls = recordingStatus.mock.calls.length;
    await vi.advanceTimersByTimeAsync(4000);
    expect(recordingStatus.mock.calls.length).toBe(calls);
  });
});
