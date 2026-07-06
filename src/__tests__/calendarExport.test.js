import { describe, expect, it } from 'vitest';
import { buildCalendarPayload, buildCalendarPayloadForItem } from '../utils/calendarExport.js';

describe('buildCalendarPayload', () => {
  it('formats a text note', () => {
    const note = { title: 'My Note', content: '  Some content  ', noteType: 'TEXT' };
    expect(buildCalendarPayload(note)).toEqual({ title: 'My Note', description: 'Some content' });
  });

  it('formats a checklist with checked/unchecked bullets, sorted by order', () => {
    const note = {
      title: 'Shopping',
      noteType: 'CHECKLIST',
      checklistItems: [
        { text: 'Milk', isChecked: true, order: 1 },
        { text: 'Eggs', isChecked: false, order: 0 },
      ],
    };
    expect(buildCalendarPayload(note)).toEqual({
      title: 'Shopping',
      description: '• Eggs\n✓ Milk',
    });
  });

  it('skips blank checklist items', () => {
    const note = {
      title: 'List',
      noteType: 'CHECKLIST',
      checklistItems: [
        { text: '  ', isChecked: false, order: 0 },
        { text: 'Real item', isChecked: false, order: 1 },
      ],
    };
    expect(buildCalendarPayload(note).description).toBe('• Real item');
  });

  it('falls back to first 50 chars of content when title is blank', () => {
    const longContent = 'x'.repeat(80);
    const note = { title: '  ', content: longContent, noteType: 'TEXT' };
    const result = buildCalendarPayload(note);
    expect(result.title).toBe('x'.repeat(50));
    expect(result.description).toBe(longContent);
  });

  it('returns null for an empty note', () => {
    expect(buildCalendarPayload({ title: '', content: '  ', noteType: 'TEXT' })).toBeNull();
  });
});

describe('buildCalendarPayloadForItem', () => {
  it('uses item text as title and parenthesized note title as description', () => {
    expect(buildCalendarPayloadForItem({ text: '  Buy milk  ' }, 'Groceries')).toEqual({
      title: 'Buy milk',
      description: '(Groceries)',
    });
  });

  it('uses empty description when the note has no title', () => {
    expect(buildCalendarPayloadForItem({ text: 'Buy milk' }, '  ')).toEqual({
      title: 'Buy milk',
      description: '',
    });
  });
});
