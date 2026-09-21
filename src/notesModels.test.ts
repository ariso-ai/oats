import { describe, it, expect } from 'vitest';
import {
  DEFAULT_NOTES_MODEL,
  NOTES_MODEL_OPTIONS,
  notesModelKey,
  notesModelLabel,
  parseNotesModel,
  type NotesModelId,
} from './notesModels';

describe('notes model registry', () => {
  it('defaults to the on-device Gemma that generates notes today', () => {
    expect(DEFAULT_NOTES_MODEL).toEqual({ kind: 'local', id: 'gemma-3-1b-it-qat-4bit' });
  });

  it('offers the default as the first pickable option', () => {
    expect(NOTES_MODEL_OPTIONS[0].value).toEqual(DEFAULT_NOTES_MODEL);
  });

  it('gives every option a distinct key', () => {
    const keys = NOTES_MODEL_OPTIONS.map((o) => notesModelKey(o.value));
    expect(new Set(keys).size).toBe(keys.length);
  });

  it('keys a remote model apart from a local one sharing its id', () => {
    const local: NotesModelId = { kind: 'local', id: 'x' };
    const remote: NotesModelId = { kind: 'remote', provider: 'anthropic', id: 'x' };
    expect(notesModelKey(local)).not.toBe(notesModelKey(remote));
  });
});

describe('notesModelLabel', () => {
  it('uses the registry display name for a registered model', () => {
    expect(notesModelLabel(DEFAULT_NOTES_MODEL)).toBe(NOTES_MODEL_OPTIONS[0].label);
  });

  it('falls back to the bare id for a model not in the registry', () => {
    expect(notesModelLabel({ kind: 'local', id: 'not-shipped' })).toBe('not-shipped');
  });
});

describe('parseNotesModel', () => {
  it('round-trips a registered selection', () => {
    expect(parseNotesModel(DEFAULT_NOTES_MODEL)).toEqual(DEFAULT_NOTES_MODEL);
  });

  it('falls back to the default when nothing is persisted', () => {
    expect(parseNotesModel(undefined)).toEqual(DEFAULT_NOTES_MODEL);
    expect(parseNotesModel(null)).toEqual(DEFAULT_NOTES_MODEL);
  });

  it('rejects an on-device id that is not in the registry', () => {
    // An unpinned id must never reach the model directory path.
    expect(parseNotesModel({ kind: 'local', id: '../../etc/passwd' })).toEqual(
      DEFAULT_NOTES_MODEL,
    );
  });

  it('rejects a remote model until one is actually registered', () => {
    expect(
      parseNotesModel({ kind: 'remote', provider: 'anthropic', id: 'claude-haiku-4-5' }),
    ).toEqual(DEFAULT_NOTES_MODEL);
  });

  it('rejects malformed persisted values', () => {
    for (const raw of ['gemma', 42, {}, { kind: 'local' }, { id: 'gemma-3-1b-it-qat-4bit' }]) {
      expect(parseNotesModel(raw)).toEqual(DEFAULT_NOTES_MODEL);
    }
  });
});
