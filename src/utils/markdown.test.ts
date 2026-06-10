import { describe, it, expect } from 'vitest';
import { renderMarkdown } from './markdown';

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
});
