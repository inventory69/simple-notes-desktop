import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('codemirror', () => ({ basicSetup: [] }));
vi.mock('@codemirror/state', () => ({ EditorState: { create: vi.fn(() => ({})) } }));
vi.mock('@codemirror/view', () => ({
  EditorView: Object.assign(
    class {
      destroy() {}
      focus() {}
    },
    {
      lineWrapping: {},
      updateListener: { of: vi.fn(() => []) },
    },
  ),
}));
vi.mock('@codemirror/commands', () => ({ undo: vi.fn() }));
vi.mock('@codemirror/lang-markdown', () => ({ markdown: () => [] }));
vi.mock('@codemirror/language', () => ({}));
vi.mock('dompurify', () => ({ default: { sanitize: (h) => h } }));
vi.mock('marked', () => ({ marked: { parse: (c) => c } }));
vi.mock('../services/tauri.js');
vi.mock('../services/noteService.js', () => ({
  default: { subscribe: vi.fn(() => vi.fn()), notify: vi.fn() },
}));
vi.mock('../services/DialogService.js', () => ({
  dialogService: { confirm: vi.fn(), alert: vi.fn(), prompt: vi.fn() },
}));

function setupDOM() {
  document.body.innerHTML = `
    <div id="editor-container" class="hidden"></div>
    <div id="editor"></div>
    <div id="preview" class="hidden"></div>
    <input id="note-title" />
    <span id="sync-status"></span>
    <button id="delete-note-btn"></button>
    <button id="preview-toggle-btn"></button>
    <button id="checklist-sort-btn" class="hidden"></button>
    <div id="checklist-container" class="checklist-container"></div>
    <div id="no-note-selected"></div>
    <button id="undo-btn"></button>
    <button id="add-checklist-item-btn" class="hidden"></button>
  `;
}

describe('NoteEditor — MANUAL sort (Android parity)', () => {
  let NoteEditor;
  let editor;

  beforeEach(async () => {
    setupDOM();
    vi.clearAllMocks();
    const mod = await import('../components/NoteEditor.js');
    NoteEditor = mod.NoteEditor;
    editor = new NoteEditor();
  });

  afterEach(() => {
    document.body.innerHTML = '';
    vi.resetModules();
  });

  // Fixture matching ChecklistSortingTest.kt: items have diverged order/originalOrder.
  function makeItems(defs) {
    return defs.map(({ id, text, isChecked, order, originalOrder }) => ({
      id,
      text,
      isChecked,
      order,
      ...(originalOrder !== undefined ? { originalOrder } : {}),
    }));
  }

  it('MANUAL: sorts unchecked before checked, each group by originalOrder', () => {
    // Android fixture: A(origOrder=2), B(origOrder=0), C(origOrder=3), D(origOrder=1)
    // Expected order: B, D, A, C  (unchecked by originalOrder)
    const items = makeItems([
      { id: 'a', text: 'A', isChecked: false, order: 0, originalOrder: 2 },
      { id: 'b', text: 'B', isChecked: false, order: 1, originalOrder: 0 },
      { id: 'c', text: 'C', isChecked: true, order: 2, originalOrder: 3 },
      { id: 'd', text: 'D', isChecked: false, order: 3, originalOrder: 1 },
    ]);

    const sorted = editor.sortChecklistItems(items, 'MANUAL');

    expect(sorted.map((i) => i.text)).toEqual(['B', 'D', 'A', 'C']);
  });

  it('MANUAL: checked group is also sorted by originalOrder', () => {
    const items = makeItems([
      { id: 'x', text: 'X', isChecked: true, order: 0, originalOrder: 5 },
      { id: 'y', text: 'Y', isChecked: true, order: 1, originalOrder: 2 },
      { id: 'z', text: 'Z', isChecked: false, order: 2, originalOrder: 0 },
    ]);

    const sorted = editor.sortChecklistItems(items, 'MANUAL');

    // unchecked first, then checked sorted by originalOrder (Y before X)
    expect(sorted.map((i) => i.text)).toEqual(['Z', 'Y', 'X']);
  });

  it('MANUAL: falls back to order when originalOrder is absent (pre-F04 note)', () => {
    const items = makeItems([
      { id: '1', text: 'first', isChecked: false, order: 0 },
      { id: '2', text: 'second', isChecked: false, order: 1 },
      { id: '3', text: 'third', isChecked: false, order: 2 },
    ]);

    const sorted = editor.sortChecklistItems(items, 'MANUAL');

    expect(sorted.map((i) => i.text)).toEqual(['first', 'second', 'third']);
  });
});

describe('NoteEditor — _renumberOrders', () => {
  let NoteEditor;
  let editor;

  beforeEach(async () => {
    setupDOM();
    vi.clearAllMocks();
    const mod = await import('../components/NoteEditor.js');
    NoteEditor = mod.NoteEditor;
    editor = new NoteEditor();
    editor.currentNote = {
      id: 'n1',
      noteType: 'CHECKLIST',
      checklistItems: [
        { id: 'a', text: 'A', isChecked: false, order: 99, originalOrder: 42 },
        { id: 'b', text: 'B', isChecked: false, order: 7, originalOrder: 0 },
        { id: 'c', text: 'C', isChecked: true, order: 3, originalOrder: 8 },
      ],
    };
  });

  afterEach(() => {
    document.body.innerHTML = '';
    vi.resetModules();
  });

  it('sets order === originalOrder === sequential index for every item', () => {
    editor._renumberOrders();

    editor.currentNote.checklistItems.forEach((item, i) => {
      expect(item.order).toBe(i);
      expect(item.originalOrder).toBe(i);
    });
  });
});

describe('NoteEditor — _fixPreF04Orders', () => {
  let NoteEditor;
  let editor;

  beforeEach(async () => {
    setupDOM();
    vi.clearAllMocks();
    const mod = await import('../components/NoteEditor.js');
    NoteEditor = mod.NoteEditor;
    editor = new NoteEditor();
  });

  afterEach(() => {
    document.body.innerHTML = '';
    vi.resetModules();
  });

  it('derives originalOrder from order when all originalOrder are absent', () => {
    editor.currentNote = {
      id: 'n1',
      noteType: 'CHECKLIST',
      checklistItems: [
        { id: 'a', text: 'A', isChecked: false, order: 0 },
        { id: 'b', text: 'B', isChecked: false, order: 1 },
        { id: 'c', text: 'C', isChecked: false, order: 2 },
      ],
    };

    editor._fixPreF04Orders();

    expect(editor.currentNote.checklistItems[0].originalOrder).toBe(0);
    expect(editor.currentNote.checklistItems[1].originalOrder).toBe(1);
    expect(editor.currentNote.checklistItems[2].originalOrder).toBe(2);
  });

  it('does not overwrite existing originalOrder values', () => {
    editor.currentNote = {
      id: 'n1',
      noteType: 'CHECKLIST',
      checklistItems: [
        { id: 'a', text: 'A', isChecked: false, order: 0, originalOrder: 2 },
        { id: 'b', text: 'B', isChecked: false, order: 1, originalOrder: 0 },
        { id: 'c', text: 'C', isChecked: false, order: 2, originalOrder: 1 },
      ],
    };

    editor._fixPreF04Orders();

    // Not all are 0/null → no change
    expect(editor.currentNote.checklistItems[0].originalOrder).toBe(2);
    expect(editor.currentNote.checklistItems[1].originalOrder).toBe(0);
    expect(editor.currentNote.checklistItems[2].originalOrder).toBe(1);
  });
});

describe('NoteEditor — addChecklistItem', () => {
  let NoteEditor;
  let editor;

  beforeEach(async () => {
    setupDOM();
    vi.clearAllMocks();
    const mod = await import('../components/NoteEditor.js');
    NoteEditor = mod.NoteEditor;
    editor = new NoteEditor();
  });

  afterEach(() => {
    document.body.innerHTML = '';
    vi.resetModules();
  });

  function makeNote(items, sort = 'UNCHECKED_FIRST') {
    return { id: 'n1', title: 'T', noteType: 'CHECKLIST', checklistItems: items, checklistSortOption: sort };
  }

  it('inserts before first checked item when no afterItemId (UNCHECKED_FIRST)', () => {
    editor.currentNote = makeNote([
      { id: 'a', text: 'Apple', isChecked: false, order: 0, originalOrder: 0 },
      { id: 'b', text: 'Banana', isChecked: true, order: 1, originalOrder: 1 },
    ]);

    const newItem = editor.addChecklistItem();

    expect(editor.currentNote.checklistItems.length).toBe(3);
    expect(editor.currentNote.checklistItems[1]).toMatchObject({ text: '', isChecked: false });
    expect(newItem).toMatchObject({ text: '', isChecked: false });
  });

  it('appends to end when no checked items exist', () => {
    editor.currentNote = makeNote([
      { id: 'a', text: 'Apple', isChecked: false, order: 0, originalOrder: 0 },
      { id: 'b', text: 'Banana', isChecked: false, order: 1, originalOrder: 1 },
    ]);

    editor.addChecklistItem();

    expect(editor.currentNote.checklistItems.length).toBe(3);
    expect(editor.currentNote.checklistItems[2]).toMatchObject({ text: '', isChecked: false });
  });

  it('inserts after the given unchecked item when afterItemId provided', () => {
    editor.currentNote = makeNote([
      { id: 'a', text: 'Apple', isChecked: false, order: 0, originalOrder: 0 },
      { id: 'b', text: 'Banana', isChecked: false, order: 1, originalOrder: 1 },
    ]);

    editor.addChecklistItem('a');

    expect(editor.currentNote.checklistItems.length).toBe(3);
    expect(editor.currentNote.checklistItems[1]).toMatchObject({ text: '', isChecked: false });
  });

  it('redirects to before first checked item when afterItemId is a checked item (UNCHECKED_FIRST)', () => {
    editor.currentNote = makeNote([
      { id: 'a', text: 'Apple', isChecked: false, order: 0, originalOrder: 0 },
      { id: 'b', text: 'Banana', isChecked: true, order: 1, originalOrder: 1 },
    ]);

    editor.addChecklistItem('b');

    expect(editor.currentNote.checklistItems.length).toBe(3);
    // New item must land before 'b' (index 1), not after it
    expect(editor.currentNote.checklistItems[1]).toMatchObject({ text: '', isChecked: false });
    expect(editor.currentNote.checklistItems[2].id).toBe('b');
  });

  it('appends to end regardless of checked items for ALPHABETICAL_ASC sort', () => {
    editor.currentNote = makeNote(
      [
        { id: 'a', text: 'Apple', isChecked: false, order: 0, originalOrder: 0 },
        { id: 'b', text: 'Banana', isChecked: true, order: 1, originalOrder: 1 },
      ],
      'ALPHABETICAL_ASC',
    );

    editor.addChecklistItem();

    expect(editor.currentNote.checklistItems.length).toBe(3);
    expect(editor.currentNote.checklistItems[2]).toMatchObject({ text: '', isChecked: false });
  });

  it('returns null and does not mutate for TEXT notes', () => {
    editor.currentNote = { id: 'n2', noteType: 'TEXT', title: 'text', content: '' };

    const result = editor.addChecklistItem();

    expect(result).toBeNull();
  });

  it('returns null when no note is loaded', () => {
    editor.currentNote = null;

    const result = editor.addChecklistItem();

    expect(result).toBeNull();
  });
});

describe('NoteEditor — addChecklistItem shortcut & header button', () => {
  let NoteEditor;
  let editor;

  beforeEach(async () => {
    setupDOM();
    vi.clearAllMocks();
    const mod = await import('../components/NoteEditor.js');
    NoteEditor = mod.NoteEditor;
    editor = new NoteEditor();
    editor.currentNote = {
      id: 'note-1',
      title: 'Test',
      noteType: 'CHECKLIST',
      checklistItems: [{ id: 'a', text: 'first', isChecked: false, order: 0, originalOrder: 0 }],
      checklistSortOption: 'UNCHECKED_FIRST',
    };
  });

  afterEach(() => {
    document.body.innerHTML = '';
    vi.resetModules();
  });

  it('Ctrl+Enter adds an item when focus is inside checklist-container', () => {
    editor.renderChecklist();
    const checklist = document.getElementById('checklist-container');

    checklist.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true, bubbles: true }));

    expect(editor.currentNote.checklistItems.length).toBe(2);
  });

  it('Ctrl+Enter does nothing when focus is outside checklist and title', () => {
    editor.renderChecklist();

    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true, bubbles: true }));

    expect(editor.currentNote.checklistItems.length).toBe(1);
  });

  it('Ctrl+Enter does nothing for TEXT notes', () => {
    editor.currentNote = { id: 'n2', noteType: 'TEXT', title: 'text', content: '' };
    const checklist = document.getElementById('checklist-container');

    checklist.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true, bubbles: true }));

    expect(editor.currentNote.noteType).toBe('TEXT');
  });

  it('header button click adds an item', () => {
    const btn = document.getElementById('add-checklist-item-btn');

    btn.click();

    expect(editor.currentNote.checklistItems.length).toBe(2);
  });
});
