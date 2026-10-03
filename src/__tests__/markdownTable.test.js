import { EditorSelection, EditorState } from '@codemirror/state';
import { describe, expect, it } from 'vitest';
import { tableAt } from '../utils/markdownTable.js';
import { applyTable } from '../utils/markdownToolbar.js';

/** Minimal stand-in for an EditorView: applyTable only uses state, dispatch and focus. */
function fakeView(doc, cursor = doc.length) {
  return {
    state: EditorState.create({ doc, selection: EditorSelection.cursor(cursor) }),
    dispatch(spec) {
      this.state = this.state.update(spec).state;
    },
    focus() {},
  };
}

const selected = (view) => {
  const { from, to } = view.state.selection.main;
  return view.state.sliceDoc(from, to);
};

describe('tableAt (Android MarkdownEngineTableTest parity)', () => {
  it('reports the end and the widest row of the table', () => {
    const lines = 'davor\n| a | b |\n| --- | --- |\n| 1 | 2 | 3 |\ndanach'.split('\n');
    expect(tableAt(lines, 2)).toEqual({ lastLine: 3, columns: 3 });
  });

  it('returns null outside a table', () => {
    const lines = 'nur Text\n| a | b |'.split('\n');
    expect(tableAt(lines, 0)).toBeNull();
    expect(tableAt(lines, 1)).toBeNull();
  });

  it('accepts a half-typed delimiter row like Android', () => {
    const lines = '| a | b |\n| -lblblb-- | --- |\n| 1 | 2 |'.split('\n');
    expect(tableAt(lines, 2)).toEqual({ lastLine: 2, columns: 2 });
  });

  it('ignores escaped pipes', () => {
    expect(tableAt(['a \\| b', '--- \\| ---'], 0)).toBeNull();
  });
});

describe('applyTable', () => {
  it('inserts a skeleton into an empty note and selects the first header', () => {
    const view = fakeView('');
    applyTable(view);
    expect(view.state.doc.toString()).toBe('| Header | Header |\n| --- | --- |\n| Cell | Cell |\n');
    expect(view.state.selection.main.from).toBe(2);
    expect(selected(view)).toBe('Header');
  });

  it('separates the skeleton from text with a blank line', () => {
    const view = fakeView('Text');
    applyTable(view);
    expect(view.state.doc.toString()).toBe('Text\n\n| Header | Header |\n| --- | --- |\n| Cell | Cell |\n');
    expect(selected(view)).toBe('Header');
  });

  it('adds only the missing newline after a line break', () => {
    const view = fakeView('Text\n');
    applyTable(view);
    expect(view.state.doc.toString()).toBe('Text\n\n| Header | Header |\n| --- | --- |\n| Cell | Cell |\n');
  });

  it('appends a row at the end of the table when the cursor is inside it', () => {
    const doc = 'davor\n| a | b |\n| --- | --- |\n| 1 | 2 | 3 |\ndanach';
    const view = fakeView(doc, doc.indexOf('a |'));
    applyTable(view);
    expect(view.state.doc.toString()).toBe(
      'davor\n| a | b |\n| --- | --- |\n| 1 | 2 | 3 |\n| Cell | Cell | Cell |\ndanach',
    );
    expect(selected(view)).toBe('Cell');
    expect(view.state.selection.main.from).toBe(doc.indexOf('\ndanach') + 3);
  });

  it('pressing twice gives a table plus a second row', () => {
    const view = fakeView('');
    applyTable(view);
    applyTable(view);
    expect(view.state.doc.toString()).toBe('| Header | Header |\n| --- | --- |\n| Cell | Cell |\n| Cell | Cell |\n');
  });
});
