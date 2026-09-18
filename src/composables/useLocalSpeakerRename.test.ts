import { describe, it, expect, vi, beforeEach } from 'vitest';
import { computed, ref } from 'vue';
import type { Participant } from '../tauri';

const renameSpeaker = vi.fn();
vi.mock('../tauri', () => ({
  local: {
    renameSpeaker: (...a: unknown[]) => renameSpeaker(...a),
  },
}));

import { useLocalSpeakerRename, MAX_SPEAKER_LABEL_CHARS } from './useLocalSpeakerRename';

function setup(initial: Participant[] = [
  { id: 0, label: 'Speaker 1' },
  { id: 1, label: 'Speaker 2' },
]) {
  const recordingId = ref<string | null>('2026-09-15T10-00-00Z');
  const list = ref<Participant[]>(initial);
  const md = ref<string | undefined>('---\n---\n\n**Speaker 1** [00:00:00]\nHi\n');
  const rename = useLocalSpeakerRename({
    recordingId,
    speakers: computed({ get: () => list.value, set: (v) => { list.value = v; } }),
    transcript: computed({ get: () => md.value, set: (v) => { md.value = v; } }),
  });
  return { rename, list, md, recordingId };
}

beforeEach(() => {
  renameSpeaker.mockReset();
  renameSpeaker.mockResolvedValue('---\n---\n\n**Priya** [00:00:00]\nHi\n');
});

describe('useLocalSpeakerRename', () => {
  it('exposes the speakers it was given', () => {
    const { rename } = setup();
    expect(rename.speakers.value.map((s) => s.label)).toEqual(['Speaker 1', 'Speaker 2']);
  });

  it('seeds the draft with the current label when editing starts', () => {
    const { rename } = setup();
    rename.startEdit(1);
    expect(rename.editingId.value).toBe(1);
    expect(rename.draft.value).toBe('Speaker 2');
  });

  it('commits a trimmed rename and patches both the list and the transcript', async () => {
    const { rename, list, md } = setup();
    rename.startEdit(0);
    rename.draft.value = '  Priya  ';
    await rename.commit();

    expect(renameSpeaker).toHaveBeenCalledWith('2026-09-15T10-00-00Z', 0, 'Priya');
    expect(list.value).toEqual([
      { id: 0, label: 'Priya' },
      { id: 1, label: 'Speaker 2' },
    ]);
    expect(md.value).toBe('---\n---\n\n**Priya** [00:00:00]\nHi\n');
    expect(rename.editingId.value).toBeNull();
    expect(rename.error.value).toBeNull();
    expect(rename.saving.value).toBe(false);
  });

  it('rejects an empty draft in the UI without calling the backend', async () => {
    const { rename } = setup();
    rename.startEdit(0);
    rename.draft.value = '   ';
    await rename.commit();

    expect(renameSpeaker).not.toHaveBeenCalled();
    expect(rename.error.value).toBe('Name must not be empty');
    // The field stays open so the user can fix it in place.
    expect(rename.editingId.value).toBe(0);
  });

  it('rejects an over-length draft in the UI without calling the backend', async () => {
    const { rename } = setup();
    rename.startEdit(0);
    rename.draft.value = 'x'.repeat(MAX_SPEAKER_LABEL_CHARS + 1);
    await rename.commit();

    expect(renameSpeaker).not.toHaveBeenCalled();
    expect(rename.error.value).toBe(`Name must be ${MAX_SPEAKER_LABEL_CHARS} characters or fewer`);
    expect(rename.editingId.value).toBe(0);
  });

  it('surfaces a backend rejection string and leaves the field open', async () => {
    renameSpeaker.mockRejectedValue(
      "this recording predates structured transcripts and can't be renamed"
    );
    const { rename, list, md } = setup();
    const before = md.value;
    rename.startEdit(0);
    rename.draft.value = 'Priya';
    await rename.commit();

    expect(rename.error.value).toBe(
      "this recording predates structured transcripts and can't be renamed"
    );
    expect(rename.editingId.value).toBe(0);
    expect(rename.saving.value).toBe(false);
    // Nothing optimistic: the list and transcript are untouched on failure.
    expect(list.value[0].label).toBe('Speaker 1');
    expect(md.value).toBe(before);
  });

  it('drops a stale rename result when the displayed recording changes mid-commit', async () => {
    let resolveRename: (markdown: string) => void = () => {};
    renameSpeaker.mockImplementation(
      () =>
        new Promise<string>((resolve) => {
          resolveRename = resolve;
        })
    );
    const { rename, list, md, recordingId } = setup();
    const listBefore = list.value.map((p) => ({ ...p }));
    const mdBefore = md.value;

    rename.startEdit(0);
    rename.draft.value = 'Priya';
    const commitPromise = rename.commit();

    // The user navigates to a different recording before the rename resolves.
    recordingId.value = 'some-other-recording';
    resolveRename('---\n---\n\n**Priya** [00:00:00]\nHi\n');
    await commitPromise;

    expect(list.value).toEqual(listBefore);
    expect(md.value).toBe(mdBefore);
    expect(rename.saving.value).toBe(false);
  });

  it('drops a stale rename *error* when the displayed recording changes mid-commit', async () => {
    // Regression: the catch had no id guard, so a failure on meeting A could
    // paint A's error banner into the panel after the user switched to
    // meeting B (load() resets the panel, but nothing re-cleared the error).
    let rejectRename: (reason: string) => void = () => {};
    renameSpeaker.mockImplementation(
      () =>
        new Promise<string>((_resolve, reject) => {
          rejectRename = reject;
        })
    );
    const { rename, recordingId } = setup();

    rename.startEdit(0);
    rename.draft.value = 'Priya';
    const commitPromise = rename.commit();

    // The user navigates to a different recording; load() resets the panel.
    recordingId.value = 'some-other-recording';
    rename.reset();
    rejectRename('unknown speaker id: 0');
    await commitPromise;

    expect(rename.error.value).toBeNull();
    expect(rename.saving.value).toBe(false);
  });

  it('does not clobber a newer edit session opened on another speaker while a rename is in flight', async () => {
    let resolveRename: (markdown: string) => void = () => {};
    renameSpeaker.mockImplementation(
      () =>
        new Promise<string>((resolve) => {
          resolveRename = resolve;
        })
    );
    const { rename } = setup();

    rename.startEdit(0);
    rename.draft.value = 'Priya';
    const commitPromise = rename.commit();

    // Same recording, but the user opens a different speaker's edit field
    // before the first rename resolves.
    rename.startEdit(1);
    resolveRename('---\n---\n\n**Priya** [00:00:00]\nHi\n');
    await commitPromise;

    expect(rename.editingId.value).toBe(1);
    expect(rename.draft.value).toBe('Speaker 2');
  });

  it('disowns a rename that reset() abandoned, even on the same recording', async () => {
    // Regression: the recording-id guards can't see a close-and-reopen on the
    // *same* recording — the id still matches. Without a generation counter the
    // abandoned request patches over the reopened panel and, worse, its
    // `finally` clears `saving` while a second rename is still in flight,
    // re-enabling the input mid-save.
    const pending: Array<(markdown: string) => void> = [];
    renameSpeaker.mockImplementation(
      () => new Promise<string>((resolve) => pending.push(resolve))
    );
    const { rename, list, md } = setup();

    rename.startEdit(0);
    rename.draft.value = 'Priya';
    const abandoned = rename.commit();

    // Panel closed and reopened on the same recording, then a second rename
    // starts before the first one has resolved.
    rename.reset();
    rename.startEdit(1);
    rename.draft.value = 'Sam';
    const current = rename.commit();

    const listBefore = [...list.value];
    const mdBefore = md.value;

    // The abandoned request resolves last.
    pending[0]('---\n---\n\n**Priya** [00:00:00]\nHi\n');
    await abandoned;

    // It touched nothing, and left the in-flight rename's `saving` alone.
    expect(list.value).toEqual(listBefore);
    expect(md.value).toBe(mdBefore);
    expect(rename.editingId.value).toBe(1);
    expect(rename.saving.value).toBe(true);

    // The current request still lands normally.
    pending[1]('---\n---\n\n**Sam** [00:00:00]\nHi\n');
    await current;
    expect(list.value[1].label).toBe('Sam');
    expect(rename.saving.value).toBe(false);
  });

  it('ignores a commit with no recording loaded', async () => {
    const { rename, recordingId } = setup();
    rename.startEdit(0);
    rename.draft.value = 'Priya';
    recordingId.value = null;
    await rename.commit();
    expect(renameSpeaker).not.toHaveBeenCalled();
  });

  it('clears the error and the draft on cancel', () => {
    const { rename } = setup();
    rename.startEdit(0);
    rename.error.value = 'boom';
    rename.cancelEdit();
    expect(rename.editingId.value).toBeNull();
    expect(rename.draft.value).toBe('');
    expect(rename.error.value).toBeNull();
  });

  it('reset closes the panel and drops all edit state', () => {
    const { rename } = setup();
    rename.open.value = true;
    rename.startEdit(0);
    rename.error.value = 'boom';
    rename.reset();
    expect(rename.open.value).toBe(false);
    expect(rename.editingId.value).toBeNull();
    expect(rename.draft.value).toBe('');
    expect(rename.error.value).toBeNull();
  });
});
