import { describe, it, expect } from 'vitest';
import {
  modelCatalog,
  formatModelSize,
  parseSpeechModelKey,
  speechModelIdFromKey,
  DEFAULT_SPEECH_MODEL_KEY,
} from './modelCatalog';
import { DEFAULT_NOTES_MODEL, notesModelKey } from './notesModels';

describe('modelCatalog', () => {
  it('lists the speech model alongside the notes models', () => {
    const types = modelCatalog().map((m) => m.type);
    expect(types).toContain('Notes');
    expect(types).toContain('Speech');
  });

  it('names the speech model rather than describing its role', () => {
    const speech = modelCatalog().find((m) => m.type === 'Speech');
    expect(speech?.name).toBe('Parakeet TDT 0.6B v3');
  });

  it("derives a notes row's runtime from the model itself", () => {
    const gemma = modelCatalog().find(
      (m) => m.key === notesModelKey(DEFAULT_NOTES_MODEL),
    );
    expect(gemma?.runtime).toBe('local');
    expect(gemma?.name).toBe('Gemma 3 1B');
  });

  it('carries a notes model only on notes rows', () => {
    for (const row of modelCatalog()) {
      expect(Boolean(row.notesModel)).toBe(row.type === 'Notes');
    }
  });

  it('gives every row hover details naming the underlying model id', () => {
    const gemma = modelCatalog().find((m) => m.type === 'Notes');
    expect(gemma?.details).toContain('gemma-3-1b-it-qat-4bit');
    const speech = modelCatalog().find((m) => m.type === 'Speech');
    expect(speech?.details).toContain('parakeet-tdt-0.6b-v3');
  });

  it('gives every row a distinct key', () => {
    const keys = modelCatalog().map((m) => m.key);
    expect(new Set(keys).size).toBe(keys.length);
  });
});

describe('formatModelSize', () => {
  it('shows a dash when the size is not known', () => {
    expect(formatModelSize(null)).toBe('—');
  });

  it('rounds to whole megabytes', () => {
    expect(formatModelSize(750 * 1024 * 1024)).toBe('750 MB');
  });

  it('switches to gigabytes past a thousand megabytes', () => {
    expect(formatModelSize(2 * 1024 * 1024 * 1024)).toBe('2.0 GB');
  });

  it('shows a zero-byte model as 0 MB rather than a dash', () => {
    expect(formatModelSize(0)).toBe('0 MB');
  });
});

describe('parseSpeechModelKey', () => {
  it('keeps a key this build ships', () => {
    expect(parseSpeechModelKey(DEFAULT_SPEECH_MODEL_KEY)).toBe(DEFAULT_SPEECH_MODEL_KEY);
  });

  it('falls back to the default for anything else', () => {
    for (const raw of [undefined, null, 42, {}, 'speech:not-shipped']) {
      expect(parseSpeechModelKey(raw)).toBe(DEFAULT_SPEECH_MODEL_KEY);
    }
  });
});

describe('Qwen3-ASR speech model', () => {
  it('lists Qwen3-ASR as a second local speech model', () => {
    const speech = modelCatalog().filter((m) => m.type === 'Speech');
    expect(speech.map((m) => m.name)).toEqual(['Parakeet TDT 0.6B v3', 'Qwen3-ASR 0.6B']);
    const qwen = speech[1];
    expect(qwen.key).toBe('speech:qwen3-asr-0.6b-4bit');
    expect(qwen.speechModel).toBe('qwen3-asr-0.6b-4bit');
    expect(qwen.runtime).toBe('local');
    expect(qwen.details).toContain('Mandarin');
  });

  it('keeps an installed selection of Qwen3 and rejects unknown speech keys', () => {
    expect(parseSpeechModelKey('speech:qwen3-asr-0.6b-4bit')).toBe('speech:qwen3-asr-0.6b-4bit');
    expect(parseSpeechModelKey('speech:whisper-large-v3')).toBe(DEFAULT_SPEECH_MODEL_KEY);
    expect(speechModelIdFromKey('speech:qwen3-asr-0.6b-4bit')).toBe('qwen3-asr-0.6b-4bit');
    expect(speechModelIdFromKey('garbage')).toBe('parakeet-tdt-0.6b-v3');
  });
});
