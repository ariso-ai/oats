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

  it('marks a checked item as done so it can be struck through', () => {
    expect(renderMarkdown('- [x] done item')).toContain('<li class="task-list-item task-done">');
    expect(renderMarkdown('- [ ] open item')).toContain('<li class="task-list-item">');
  });
});

describe('renderMarkdown interactive tasks', () => {
  it('renders enabled checkboxes tagged with their source line', () => {
    const src = '## Action Items\n- [ ] Ship the RFC ➕ 2026-09-09\n- [x] Email legal ✅ 2026-09-10';
    const html = renderMarkdown(src, { interactiveTasks: true });
    expect(html).toContain('<input type="checkbox" data-task-line="1" />');
    expect(html).toContain('<input type="checkbox" data-task-line="2" checked />');
    expect(html).not.toContain('disabled');
  });

  it('counts source lines across CRLF endings and blank lines', () => {
    const html = renderMarkdown('Intro\r\n\r\n- [ ] Task', { interactiveTasks: true });
    expect(html).toContain('data-task-line="2"');
  });

  it('keeps a `[]` item read-only, since it is not an Obsidian task', () => {
    const html = renderMarkdown('- [] not quite a task', { interactiveTasks: true });
    expect(html).toContain('<input type="checkbox" disabled />');
    expect(html).not.toContain('data-task-line');
  });

  it('keeps a task-looking line inside a fenced code block read-only', () => {
    // The backend's set_task_done rejects a line inside a fence as "not a
    // task", so the control here must stay disabled rather than clickable.
    const src = '- [ ] Real task\n```\n- [ ] fenced, not a task\n```\n- [ ] Another real task';
    const html = renderMarkdown(src, { interactiveTasks: true });
    expect(html).toContain('data-task-line="0"');
    expect(html).toContain('data-task-line="4"');
    expect(html).not.toContain('data-task-line="2"');
  });

  it('keeps a task-looking line inside a tilde-fenced code block read-only', () => {
    const src = '~~~\n- [ ] fenced, not a task\n~~~\n- [ ] Real task';
    const html = renderMarkdown(src, { interactiveTasks: true });
    expect(html).not.toContain('data-task-line="1"');
    expect(html).toContain('data-task-line="3"');
  });
});
