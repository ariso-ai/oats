import { describe, it, expect } from 'vitest';
import { parseLocalTranscript } from './localTranscript';

// Mirrors src-tauri/src/storage.rs `render_markdown`: YAML front-matter, then one
// `**<label>** [HH:MM:SS]` header per segment followed by its text.
const RENDERED = [
  '---',
  'title: "Recording 2026-06-02 14:30"',
  'date: "2026-06-02T14:30:05Z"',
  'duration: "00:42:13"',
  'participants: ["Speaker 1", "Speaker 2"]',
  '---',
  '',
  '**Speaker 1** [00:00:03]',
  'Hello there',
  '',
  '**Speaker 2** [01:02:09]',
  'Hi back',
  '',
].join('\n');

describe('parseLocalTranscript', () => {
  it('turns each speaker block into an Ariso-shaped chunk', () => {
    expect(parseLocalTranscript(RENDERED)).toEqual([
      { chunk_index: 0, start_ms: 3_000, content: 'Speaker 1: Hello there', transcript_id: 'local' },
      { chunk_index: 1, start_ms: 3_729_000, content: 'Speaker 2: Hi back', transcript_id: 'local' },
    ]);
  });

  it('joins a multi-line segment body into one line', () => {
    const md = '**Speaker 1** [00:00:00]\nfirst line\nsecond line\n\n';
    expect(parseLocalTranscript(md)?.[0].content).toBe('Speaker 1: first line second line');
  });

  it('keeps speaker labels that contain brackets or asterisks', () => {
    const md = '**Alex [PM] *lead*** [00:00:05]\nhey\n';
    expect(parseLocalTranscript(md)?.[0]).toMatchObject({ start_ms: 5_000, content: 'Alex [PM] *lead*: hey' });
  });

  it('drops a speaker block whose body is empty (a silent recording)', () => {
    const md = '---\ntitle: "t"\n---\n\n**Speaker 1** [00:00:00]\n\n\n**Speaker 2** [00:00:04]\nhi\n';
    expect(parseLocalTranscript(md)).toEqual([
      { chunk_index: 0, start_ms: 4_000, content: 'Speaker 2: hi', transcript_id: 'local' },
    ]);
  });

  it('returns an empty list for a transcript with no speech', () => {
    expect(parseLocalTranscript('---\ntitle: "t"\n---\n\n**Speaker 1** [00:00:00]\n\n')).toEqual([]);
    expect(parseLocalTranscript('---\ntitle: "t"\n---\n\n')).toEqual([]);
  });

  it('handles CRLF line endings', () => {
    const md = '**Speaker 1** [00:00:07]\r\nhello\r\n\r\n';
    expect(parseLocalTranscript(md)).toEqual([
      { chunk_index: 0, start_ms: 7_000, content: 'Speaker 1: hello', transcript_id: 'local' },
    ]);
  });

  it('returns null for text that is not in the rendered speaker-block format', () => {
    // Unrecognized body text must not be dropped; the caller falls back to
    // rendering it as markdown.
    expect(parseLocalTranscript('# Transcript\nhi')).toBeNull();
    expect(parseLocalTranscript('intro line\n**Speaker 1** [00:00:00]\nhi\n')).toBeNull();
  });
});
