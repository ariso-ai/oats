import type { TranscriptChunk } from '../composables/useMeetingApi';
import { stripFrontmatter } from '../utils/markdown';

// One `render_markdown` speaker header: `**Speaker 1** [00:00:03]`. The label is
// matched greedily so a label that itself holds `**` or `] [` still resolves to
// the last `** [HH:MM:SS]` on the line.
const HEADER = /^\*\*(.+)\*\* \[(\d+):(\d{2}):(\d{2})\]$/;

/**
 * Parse a local recording's `transcript.md` (rendered by
 * `src-tauri/src/storage.rs::render_markdown`) into the chunk shape the Ariso
 * API serves, so both backends render through the same transcript list. Each
 * speaker block becomes one chunk whose content is `"<label>: <text>"`, as in
 * Ariso chunks. Blocks with no text (a silent recording) are dropped.
 *
 * Returns null when the body isn't in the rendered speaker-block format, so the
 * caller can fall back to plain markdown rather than lose the text.
 */
export function parseLocalTranscript(md: string): TranscriptChunk[] | null {
  const lines = stripFrontmatter(md.replace(/\r\n/g, '\n')).split('\n');
  const chunks: TranscriptChunk[] = [];
  let current: { label: string; startMs: number; text: string[] } | null = null;

  const flush = () => {
    const text = current?.text.join(' ').trim();
    if (current && text) {
      chunks.push({
        chunk_index: chunks.length,
        start_ms: current.startMs,
        content: `${current.label}: ${text}`,
        transcript_id: 'local',
      });
    }
  };

  for (const raw of lines) {
    const line = raw.trim();
    const header = HEADER.exec(line);
    if (header) {
      flush();
      const [, label, h, m, s] = header;
      current = { label, startMs: (Number(h) * 3600 + Number(m) * 60 + Number(s)) * 1000, text: [] };
    } else if (line) {
      // Text before the first header means this isn't a rendered transcript.
      if (!current) return null;
      current.text.push(line);
    }
  }
  flush();
  return chunks;
}
