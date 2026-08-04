import { describe, expect, it } from 'vitest';
import { collectAssetNames, markdownToShareHtml, noteToMarkdown, noteToPlainText } from '../utils/noteShare.js';

describe('noteToMarkdown', () => {
  it('renders title as a heading, then a blank line, then the body', () => {
    const note = { title: 'My Note', noteType: 'TEXT', content: 'Hello world' };
    expect(noteToMarkdown(note)).toBe('# My Note\n\nHello world');
  });

  it('omits the heading for an untitled note', () => {
    const note = { title: '', noteType: 'TEXT', content: 'Hello world' };
    expect(noteToMarkdown(note)).toBe('Hello world');
  });

  it('renders checklist items sorted by order as Markdown checkboxes', () => {
    const note = {
      title: 'Shopping',
      noteType: 'CHECKLIST',
      checklistItems: [
        { id: '2', text: 'Bread', isChecked: false, order: 1 },
        { id: '1', text: 'Milk', isChecked: true, order: 0 },
      ],
    };
    expect(noteToMarkdown(note)).toBe('# Shopping\n\n- [x] Milk\n- [ ] Bread');
  });

  it('filters out empty checklist items', () => {
    const note = {
      title: 'List',
      noteType: 'CHECKLIST',
      checklistItems: [
        { id: '1', text: 'Keep', isChecked: false, order: 0 },
        { id: '2', text: '   ', isChecked: false, order: 1 },
      ],
    };
    expect(noteToMarkdown(note)).toBe('# List\n\n- [ ] Keep');
  });

  it('passes TEXT content through as-is (trimmed)', () => {
    const note = { title: 'T', noteType: 'TEXT', content: '  line1\nline2  ' };
    expect(noteToMarkdown(note)).toBe('# T\n\nline1\nline2');
  });
});

describe('noteToPlainText', () => {
  it('uses a plain first line for the title, no "#"', () => {
    const note = { title: 'My Note', noteType: 'TEXT', content: 'Hello' };
    expect(noteToPlainText(note)).toBe('My Note\n\nHello');
  });

  it('replaces images with placeholders', () => {
    const note = { title: 'Pic', noteType: 'TEXT', content: 'See ![Sunset](.assets/x.webp) here' };
    expect(noteToPlainText(note)).toBe('Pic\n\nSee [🖼 Sunset] here');
  });
});

describe('collectAssetNames', () => {
  it('collects distinct asset names, deduped', () => {
    const content = '![a](.assets/x.webp) text ![b](.assets/y.jpg) ![c](.assets/x.webp)';
    expect(collectAssetNames(content)).toEqual(['x.webp', 'y.jpg']);
  });

  it('returns an empty array for content with no images', () => {
    expect(collectAssetNames('plain text')).toEqual([]);
  });

  it('does not leak lastIndex across calls (shared global regex)', () => {
    const content = '![a](.assets/x.webp)';
    expect(collectAssetNames(content)).toEqual(['x.webp']);
    expect(collectAssetNames(content)).toEqual(['x.webp']);
  });
});

describe('markdownToShareHtml', () => {
  it('inlines a data URL for a resolved asset', () => {
    const md = '![Sunset](.assets/x.webp)';
    const html = markdownToShareHtml(md, new Map([['x.webp', 'data:image/webp;base64,AAA=']]));
    expect(html).toContain('<img src="data:image/webp;base64,AAA=" alt="Sunset"');
  });

  it('degrades a missing entry to a text placeholder, no <img>', () => {
    const md = '![Sunset](.assets/missing.webp)';
    const html = markdownToShareHtml(md, new Map([['missing.webp', null]]));
    expect(html).not.toContain('<img');
    expect(html).toContain('[🖼 Sunset]');
  });

  it('degrades an unresolved (never-looked-up) asset the same way', () => {
    const md = '![](.assets/missing.webp)';
    const html = markdownToShareHtml(md, new Map());
    expect(html).not.toContain('<img');
    expect(html).toContain('[🖼]');
  });

  it('leaves external image URLs unchanged', () => {
    const md = '![alt](https://example.com/pic.png)';
    const html = markdownToShareHtml(md, new Map());
    expect(html).toContain('<img src="https://example.com/pic.png"');
  });
});
