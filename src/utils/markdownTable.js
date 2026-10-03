/**
 * GFM tables with Android's tolerance (simple-notes-sync `MarkdownEngine`): toolbar, preview,
 * share HTML and the sidebar all use the same rules, so a note renders the same on both clients.
 *
 * Deliberately looser than GFM, decided on Android on 2026-09-19:
 * 1. A delimiter row counts as soon as ONE cell is a real `---` / `:---` / `---:`. Whoever types
 *    into the delimiter row (`| -abc-- | --- |`) otherwise lost the whole table to pipe salad.
 * 2. The widest row sets the column count; an extra `| cell |` widens the table instead of being cut.
 * 3. A body row made only of delimiter cells (skeleton inserted twice) is dropped.
 * 4. A line without an unescaped `|` ends the table (GFM would keep going until a blank line).
 */

const DELIMITER_CELL = /^:?-+:?$/;

/** Separator for every surface that shows a table as plain text (Android `CELL_SEPARATOR`). */
const CELL_SEPARATOR = ' · ';

function hasUnescapedPipe(line) {
  for (let i = 0; i < line.length; i++) {
    if (line[i] === '\\') i++;
    else if (line[i] === '|') return true;
  }
  return false;
}

function splitCells(line) {
  const cells = [];
  let current = '';
  for (let i = 0; i < line.length; i++) {
    if (line[i] === '\\' && line[i + 1] === '|') {
      current += '|';
      i++;
    } else if (line[i] === '|') {
      cells.push(current.trim());
      current = '';
    } else {
      current += line[i];
    }
  }
  cells.push(current.trim());
  const trimmed = line.trim();
  if (cells[0] === '' && trimmed.startsWith('|')) cells.shift();
  if (cells.at(-1) === '' && trimmed.endsWith('|') && !trimmed.endsWith('\\|')) cells.pop();
  return cells;
}

/** Rule 3: a row of only delimiter cells carries no content. */
function isDelimiterOnly(cells) {
  return cells.length > 0 && cells.every((c) => DELIMITER_CELL.test(c));
}

function isDelimiterRow(line) {
  return hasUnescapedPipe(line) && splitCells(line).some((c) => DELIMITER_CELL.test(c));
}

/** Header with a pipe whose NEXT line is a delimiter row. The lookahead keeps `a | b` in prose out. */
function isTableStart(lines, index) {
  return (
    index + 1 < lines.length &&
    hasUnescapedPipe(lines[index]) &&
    isDelimiterRow(lines[index + 1]) &&
    splitCells(lines[index]).length > 0
  );
}

/** Broken delimiter cells fall back to the default alignment (Android: LEFT, marked: null). */
function alignOf(cell = '') {
  if (cell.startsWith(':') && cell.endsWith(':')) return 'center';
  if (cell.endsWith(':')) return 'right';
  if (cell.startsWith(':')) return 'left';
  return null;
}

/** The table that line `index` (0-based) belongs to: `{ lastLine, columns }` or `null`. */
export function tableAt(lines, index) {
  if (!hasUnescapedPipe(lines[index] ?? '')) return null;
  let start = index;
  while (start > 0 && hasUnescapedPipe(lines[start - 1])) start--;
  if (!isTableStart(lines, start)) return null;
  let last = index;
  while (last + 1 < lines.length && hasUnescapedPipe(lines[last + 1])) last++;
  const columns = Math.max(...lines.slice(start, last + 1).map((l) => splitCells(l).length));
  return { lastLine: last, columns };
}

/** Sidebar text: table rows become `a · b`, delimiter rows disappear (like Android's grid card). Prose `a | b` stays. */
export function flattenTableRows(text) {
  if (!text.includes('|')) return text;
  const lines = text.split('\n');
  const out = [];
  let i = 0;
  while (i < lines.length) {
    if (!isTableStart(lines, i)) {
      out.push(lines[i++]);
      continue;
    }
    out.push(splitCells(lines[i]).join(CELL_SEPARATOR));
    i += 2;
    for (; i < lines.length && hasUnescapedPipe(lines[i]); i++) {
      const cells = splitCells(lines[i]);
      if (!isDelimiterOnly(cells)) out.push(cells.join(CELL_SEPARATOR));
    }
  }
  return out.join('\n');
}

/**
 * Marked extension replacing the GFM table tokenizer with the rules above: `.use(tolerantTables)`.
 * ponytail: only a table at the start of a block is seen. Directly under a paragraph (no blank line)
 * with a half-typed delimiter row, marked's paragraph swallows it, Android splits it. The toolbar
 * always inserts the blank line; override the paragraph tokenizer too if that case shows up.
 */
export const tolerantTables = {
  tokenizer: {
    table(src) {
      const [head, delimiter] = src.split('\n', 2);
      if (delimiter === undefined || !isTableStart([head, delimiter], 0)) return false;

      const lines = src.split('\n');
      let end = 2;
      while (end < lines.length && hasUnescapedPipe(lines[end])) end++;

      const header = splitCells(head);
      const rows = lines
        .slice(2, end)
        .map(splitCells)
        .filter((cells) => !isDelimiterOnly(cells));
      const width = Math.max(header.length, ...rows.map((r) => r.length));
      const delimiterCells = splitCells(delimiter);
      const align = Array.from({ length: width }, (_, i) => alignOf(delimiterCells[i]));
      const toCells = (cells, isHeader) =>
        Array.from({ length: width }, (_, i) => {
          const text = cells[i] ?? '';
          return { text, tokens: this.lexer.inline(text), header: isHeader, align: align[i] };
        });

      return {
        type: 'table',
        raw: lines.slice(0, end).join('\n') + (end < lines.length ? '\n' : ''),
        header: toCells(header, true),
        align,
        rows: rows.map((r) => toCells(r, false)),
      };
    },
  },
};
