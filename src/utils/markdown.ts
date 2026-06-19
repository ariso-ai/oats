// Minimal, dependency-free Markdown → HTML renderer for meeting notes.
//
// The desktop bundle has no markdown library and isn't part of the monorepo
// workspace, so rather than add a dependency we render the small subset of
// Markdown that the notes/digest/transcript content actually uses: headings,
// bold/italic, inline code, links, ordered/unordered lists, blockquotes, and
// paragraphs. All text is HTML-escaped before any markup is emitted, so the
// output is safe to bind with `v-html`.

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

// Inline spans: code first (so its contents aren't re-processed), then links,
// bold, and italic. Operates on already-escaped text.
function renderInline(text: string): string {
  let out = text;
  // `inline code`
  out = out.replace(/`([^`]+)`/g, (_m, code) => `<code>${code}</code>`);
  // [label](url) — only allow http(s) and mailto to avoid javascript: URLs.
  out = out.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_m, label, url) => {
    const safe = /^(https?:|mailto:)/i.test(url);
    return safe
      ? `<a href="${url}" target="_blank" rel="noopener noreferrer">${label}</a>`
      : label;
  });
  // **bold** / __bold__
  out = out.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  out = out.replace(/__([^_]+)__/g, '<strong>$1</strong>');
  // *italic* / _italic_
  out = out.replace(/(^|[^*])\*([^*\n]+)\*/g, '$1<em>$2</em>');
  out = out.replace(/(^|[^_])_([^_\n]+)_/g, '$1<em>$2</em>');
  return out;
}

// Strip a leading YAML front-matter block (`---` … `---`) if the string opens
// with one. Local recordings persist note/transcript markdown with metadata
// front-matter (title/date/duration/participants) that's useful in the exported
// file on disk but is noise when rendering in-app, so callers strip it for
// display only. Returns the input unchanged when there's no leading block.
export function stripFrontmatter(src: string): string {
  if (!src) return src;
  const normalized = src.replace(/\r\n/g, '\n');
  const match = normalized.match(/^---\n[\s\S]*?\n---[ \t]*(?:\n|$)/);
  return match ? normalized.slice(match[0].length).replace(/^\n+/, '') : src;
}

/** Render a Markdown string to a sanitized HTML string. */
export function renderMarkdown(src: string): string {
  if (!src) return '';
  const lines = escapeHtml(src.replace(/\r\n/g, '\n')).split('\n');
  const html: string[] = [];

  // List/paragraph accumulators. We buffer consecutive lines of the same block
  // type and flush on a boundary (blank line, heading, or a different block).
  let listType: 'ul' | 'ol' | null = null;
  let para: string[] = [];
  let quote: string[] = [];

  const flushList = () => {
    if (listType) {
      html.push(`</${listType}>`);
      listType = null;
    }
  };
  const flushPara = () => {
    if (para.length) {
      html.push(`<p>${renderInline(para.join(' '))}</p>`);
      para = [];
    }
  };
  const flushQuote = () => {
    if (quote.length) {
      html.push(`<blockquote>${renderInline(quote.join(' '))}</blockquote>`);
      quote = [];
    }
  };
  const flushAll = () => {
    flushPara();
    flushList();
    flushQuote();
  };

  for (const raw of lines) {
    const line = raw.trimEnd();

    if (!line.trim()) {
      flushAll();
      continue;
    }

    // Headings: #..######
    const heading = line.match(/^(#{1,6})\s+(.*)$/);
    if (heading) {
      flushAll();
      const level = heading[1].length;
      html.push(`<h${level}>${renderInline(heading[2])}</h${level}>`);
      continue;
    }

    // Blockquote
    const bq = line.match(/^&gt;\s?(.*)$/);
    if (bq) {
      flushPara();
      flushList();
      quote.push(bq[1]);
      continue;
    }

    // Unordered list: -, *, +
    const ul = line.match(/^\s*[-*+]\s+(.*)$/);
    if (ul) {
      flushPara();
      flushQuote();
      if (listType !== 'ul') {
        flushList();
        html.push('<ul>');
        listType = 'ul';
      }
      // GFM task list: `[ ]`/`[]` → unchecked, `[x]`/`[X]` → checked. Both are
      // rendered as disabled checkboxes (display only, not interactive).
      const task = ul[1].match(/^\[([ xX]?)\](?:\s+(.*))?$/);
      if (task) {
        const checked = task[1] === 'x' || task[1] === 'X';
        html.push(
          `<li class="task-list-item"><input type="checkbox" disabled${
            checked ? ' checked' : ''
          } />${renderInline(task[2] ?? '')}</li>`,
        );
      } else {
        html.push(`<li>${renderInline(ul[1])}</li>`);
      }
      continue;
    }

    // Ordered list: 1. 2) etc.
    const ol = line.match(/^\s*\d+[.)]\s+(.*)$/);
    if (ol) {
      flushPara();
      flushQuote();
      if (listType !== 'ol') {
        flushList();
        html.push('<ol>');
        listType = 'ol';
      }
      html.push(`<li>${renderInline(ol[1])}</li>`);
      continue;
    }

    // Otherwise: paragraph text (lines accumulate into one <p>).
    flushList();
    flushQuote();
    para.push(line.trim());
  }

  flushAll();
  return html.join('\n');
}
