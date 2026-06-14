import { EditorSelection } from '@codemirror/state';

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
