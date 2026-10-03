import { EditorSelection } from '@codemirror/state';
import { buildImageAlt, DEFAULT_ALIGN, DEFAULT_SIZE_PERCENT } from './imageAltTokens.js';
import { tableAt } from './markdownTable.js';

function wrapSelection(view, prefix, suffix) {
  const { state } = view;
  const sel = state.selection.main;

  let changes, newSel;
  if (sel.empty) {
    changes = { from: sel.from, insert: prefix + suffix };
    newSel = EditorSelection.cursor(sel.from + prefix.length);
  } else {
    const text = state.sliceDoc(sel.from, sel.to);
    changes = { from: sel.from, to: sel.to, insert: prefix + text + suffix };
    newSel = EditorSelection.range(sel.from + prefix.length, sel.from + prefix.length + text.length);
  }

  view.dispatch({ changes, selection: newSel });
  view.focus();
}

export function applyBold(view) {
  wrapSelection(view, '**', '**');
}

export function applyItalic(view) {
  wrapSelection(view, '*', '*');
}

export function applyStrikethrough(view) {
  wrapSelection(view, '~~', '~~');
}

export function applyCode(view) {
  wrapSelection(view, '`', '`');
}

export function applyLink(view) {
  const { state } = view;
  const sel = state.selection.main;

  let changes, newSel;
  if (sel.empty) {
    changes = { from: sel.from, insert: '[](url)' };
    newSel = EditorSelection.cursor(sel.from + 1);
  } else {
    const text = state.sliceDoc(sel.from, sel.to);
    const insert = `[${text}](url)`;
    changes = { from: sel.from, to: sel.to, insert };
    const urlStart = sel.from + text.length + 3;
    newSel = EditorSelection.range(urlStart, urlStart + 3);
  }

  view.dispatch({ changes, selection: newSel });
  view.focus();
}

/**
 * Inserts `![](url)` at the cursor (or `![text](url)` around a selection), mirroring applyLink.
 * `sizePercent` (Settings > default_image_size_percent) is baked into the alt as a size token —
 * see imageAltTokens.js — and omitted when it's the 50% default (identical to pre-setting behavior).
 */
export function applyImage(view, url, sizePercent = DEFAULT_SIZE_PERCENT) {
  const { state } = view;
  const sel = state.selection.main;
  const text = sel.empty ? '' : state.sliceDoc(sel.from, sel.to);
  const alt = buildImageAlt(text, sizePercent, DEFAULT_ALIGN);
  const insert = `![${alt}](${url})`;

  const changes = sel.empty ? { from: sel.from, insert } : { from: sel.from, to: sel.to, insert };
  // No size token and no selection: cursor lands inside the brackets so the user can type a caption.
  const newSel = EditorSelection.cursor(alt ? sel.from + insert.length : sel.from + 2);

  view.dispatch({ changes, selection: newSel });
  view.focus();
}

export function applyHeading(view) {
  const { state } = view;
  const sel = state.selection.main;
  const line = state.doc.lineAt(sel.head);
  const text = line.text;

  let newText, delta;
  if (text.startsWith('### ')) {
    newText = text.slice(4);
    delta = -4;
  } else if (text.startsWith('## ')) {
    newText = `### ${text.slice(3)}`;
    delta = 1;
  } else if (text.startsWith('# ')) {
    newText = `## ${text.slice(2)}`;
    delta = 1;
  } else {
    newText = `# ${text}`;
    delta = 2;
  }

  const cursorOffset = Math.max(0, sel.head - line.from + delta);
  view.dispatch({
    changes: { from: line.from, to: line.to, insert: newText },
    selection: EditorSelection.cursor(line.from + cursorOffset),
  });
  view.focus();
}

export function applyList(view) {
  const { state } = view;
  const sel = state.selection.main;
  const line = state.doc.lineAt(sel.head);
  const text = line.text;

  let newText, delta;
  if (text.startsWith('- ')) {
    newText = text.slice(2);
    delta = -2;
  } else {
    newText = `- ${text}`;
    delta = 2;
  }

  const cursorOffset = Math.max(0, sel.head - line.from + delta);
  view.dispatch({
    changes: { from: line.from, to: line.to, insert: newText },
    selection: EditorSelection.cursor(line.from + cursorOffset),
  });
  view.focus();
}

export function applyChecklist(view) {
  const { state } = view;
  const sel = state.selection.main;
  const line = state.doc.lineAt(sel.head);
  const text = line.text;

  let newText, delta;
  if (text.startsWith('- [x] ') || text.startsWith('- [X] ')) {
    newText = `- [ ] ${text.slice(6)}`;
    delta = 0;
  } else if (text.startsWith('- [ ] ')) {
    newText = `- [x] ${text.slice(6)}`;
    delta = 0;
  } else if (text.startsWith('- ')) {
    newText = `- [ ] ${text.slice(2)}`;
    delta = 4;
  } else {
    newText = `- [ ] ${text}`;
    delta = 6;
  }

  const cursorOffset = Math.max(0, sel.head - line.from + delta);
  view.dispatch({
    changes: { from: line.from, to: line.to, insert: newText },
    selection: EditorSelection.cursor(line.from + cursorOffset),
  });
  view.focus();
}

export function applyHR(view) {
  const { state } = view;
  const sel = state.selection.main;
  const line = state.doc.lineAt(sel.head);
  const isEmpty = line.text.trim() === '';

  const insert = isEmpty ? '---\n' : '\n---\n';
  const insertPos = isEmpty ? line.from : line.to;

  view.dispatch({
    changes: { from: insertPos, insert },
    selection: EditorSelection.cursor(insertPos + insert.length),
  });
  view.focus();
}

const TABLE_HEADER = 'Header';
const TABLE_CELL = 'Cell';

/**
 * Android parity (MarkdownToolbar.insertTable). Cursor inside a table: append a row at the END
 * (between header and delimiter row it would break the table). Otherwise insert a skeleton after a
 * blank line; without it a table right above would swallow header and delimiter as body rows.
 * The first placeholder is selected so typing replaces it.
 */
export function applyTable(view) {
  const { state } = view;
  const pos = state.selection.main.from;
  const table = tableAt(state.doc.toString().split('\n'), state.doc.lineAt(pos).number - 1);

  let from, insert, placeholder, selStart;
  if (table) {
    from = state.doc.line(table.lastLine + 1).to;
    insert = `\n| ${Array(table.columns).fill(TABLE_CELL).join(' | ')} |`;
    placeholder = TABLE_CELL;
    selStart = from + '\n| '.length;
  } else {
    const before = state.sliceDoc(0, pos);
    const prefix = before === '' || before.endsWith('\n\n') ? '' : before.endsWith('\n') ? '\n' : '\n\n';
    from = pos;
    insert = `${prefix}| ${TABLE_HEADER} | ${TABLE_HEADER} |\n| --- | --- |\n| ${TABLE_CELL} | ${TABLE_CELL} |\n`;
    placeholder = TABLE_HEADER;
    selStart = pos + prefix.length + '| '.length;
  }

  view.dispatch({
    changes: { from, insert },
    selection: EditorSelection.range(selStart, selStart + placeholder.length),
  });
  view.focus();
}
