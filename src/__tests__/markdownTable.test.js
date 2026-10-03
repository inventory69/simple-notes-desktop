import { EditorSelection, EditorState } from '@codemirror/state';
import { Marked } from 'marked';
import { describe, expect, it } from 'vitest';
import { flattenTableRows, tableAt, tolerantTables } from '../utils/markdownTable.js';
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

describe('tolerantTables (Android MarkdownEngine parity)', () => {
  const md = new Marked(tolerantTables);
  const cells = (html, tag) => [...html.matchAll(new RegExp(`<${tag}[^>]*>([^<]*)</${tag}>`, 'g'))].map((m) => m[1]);

  it('renders a regular table like marked, alignment included', () => {
    const src = '| a | b |\n| --- | :-: |\n| 1 | 2 |';
    expect(md.parse(src)).toBe(new Marked().parse(src));
  });

  it('keeps the table when the delimiter row is half-typed', () => {
    const html = md.parse('| a | b |\n| -lblblb-- | --- |\n| 1 | 2 |');
    expect(cells(html, 'th')).toEqual(['a', 'b']);
    expect(cells(html, 'td')).toEqual(['1', '2']);
  });

  it('widens the table to the widest row instead of cutting cells', () => {
    const html = md.parse('| a | b |\n| --- | --- |\n| 1 | 2 | 3 |');
    expect(cells(html, 'th')).toEqual(['a', 'b', '']);
    expect(cells(html, 'td')).toEqual(['1', '2', '3']);
  });

  it('drops a body row made only of delimiter cells, keeps one with content', () => {
    expect(cells(md.parse('| a |\n| --- |\n| --- |\n| 1 |'), 'td')).toEqual(['1']);
    expect(cells(md.parse('| a | b |\n| --- | --- |\n| --- | offen |'), 'td')).toEqual(['---', 'offen']);
  });

  it('ends the table at a line without a pipe', () => {
    const html = md.parse('| a |\n| --- |\n| 1 |\nProsa');
    expect(cells(html, 'td')).toEqual(['1']);
    expect(html).toContain('<p>Prosa</p>');
  });

  it('leaves prose with a pipe alone', () => {
    expect(md.parse('a | b\nnoch Text')).toBe('<p>a | b\nnoch Text</p>\n');
  });
});

describe('flattenTableRows', () => {
  it('turns table rows into "a · b" and drops the delimiter row', () => {
    expect(flattenTableRows('Vorher\n\n| Name | Header |\n| --- | --- |\n| Eins | Cell |')).toBe(
      'Vorher\n\nName · Header\nEins · Cell',
    );
  });

  it('leaves prose with a pipe untouched', () => {
    expect(flattenTableRows('a | b\nText')).toBe('a | b\nText');
  });

  it('drops a body row made only of delimiter cells', () => {
    expect(flattenTableRows('| a | b |\n| --- | --- |\n| --- | --- |\n| 1 | 2 |')).toBe('a · b\n1 · 2');
  });
});
