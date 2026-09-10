import { describe, it, expect } from 'vitest';
import { renderMarkdown, stripFrontmatter } from './markdown';

describe('stripFrontmatter', () => {
  it('strips a leading YAML front-matter block', () => {
    const src = '---\ntitle: "My meeting"\ndate: "2026-06-18"\n---\n\n**Speaker 1** [0:00]\nHello';
    expect(stripFrontmatter(src)).toBe('**Speaker 1** [0:00]\nHello');
  });

  it('handles CRLF line endings', () => {
    const src = '---\r\ntitle: "x"\r\n---\r\n\r\nBody';
    expect(stripFrontmatter(src)).toBe('Body');
  });

  it('leaves content without front-matter unchanged', () => {
    const src = '# Notes\n- a point';
    expect(stripFrontmatter(src)).toBe(src);
  });

  it('does not strip a `---` that is not at the very start', () => {
    const src = 'Intro\n\n---\ntitle: x\n---\n';
    expect(stripFrontmatter(src)).toBe(src);
  });

  it('returns empty input unchanged', () => {
    expect(stripFrontmatter('')).toBe('');
  });
});

describe('renderMarkdown task lists', () => {
  it('renders `- [ ]` as a disabled unchecked checkbox', () => {
    const html = renderMarkdown('- [ ] todo item');
    expect(html).toContain('<li class="task-list-item">');
    expect(html).toContain('<input type="checkbox" disabled />');
    expect(html).not.toContain('checked');
    expect(html).toContain('todo item');
  });

  it('renders `- []` (no inner space) as a disabled unchecked checkbox', () => {
    const html = renderMarkdown('- [] todo item');
    expect(html).toContain('<input type="checkbox" disabled />');
    expect(html).not.toContain('checked');
  });

  it('renders `- [x]` as a disabled checked checkbox', () => {
    const html = renderMarkdown('- [x] done item');
    expect(html).toContain('<input type="checkbox" disabled checked />');
    expect(html).toContain('done item');
  });

  it('renders `- [X]` (uppercase) as a disabled checked checkbox', () => {
    const html = renderMarkdown('- [X] done item');
    expect(html).toContain('<input type="checkbox" disabled checked />');
  });

  it('keeps plain list items unchanged', () => {
    const html = renderMarkdown('- regular item');
    expect(html).toContain('<li>regular item</li>');
    expect(html).not.toContain('checkbox');
  });

  it('still renders inline markup inside task items', () => {
    const html = renderMarkdown('- [x] **bold** task');
    expect(html).toContain('<strong>bold</strong>');
  });

  it('hides Obsidian Tasks metadata on a task list item', () => {
    const html = renderMarkdown('- [ ] Ship the RFC ➕ 2026-09-09');
    expect(html).toContain('type="checkbox"');
    expect(html).toContain('Ship the RFC');
    expect(html).not.toContain('➕');
    expect(html).not.toContain('2026-09-09');
  });

  it('hides every Tasks signifier, not just the created date', () => {
    const html = renderMarkdown('- [x] Ship it ➕ 2026-09-01 📅 2026-09-09 ⏫');
    expect(html).toContain('checked');
    expect(html).toContain('Ship it');
    expect(html).not.toContain('📅');
    expect(html).not.toContain('⏫');
  });

  it('leaves emoji in a plain bullet alone', () => {
    const html = renderMarkdown('- Shipped 🎉 on 📅 Friday');
    expect(html).toContain('🎉');
    expect(html).toContain('📅');
  });
});
