/**
 * GFM table detection with Android's tolerance (simple-notes-sync `MarkdownEngine.tableAt`), so the
 * toolbar treats a table exactly like Android does:
 * 1. A delimiter row counts as soon as ONE cell is a real `---` / `:---` / `---:`.
 * 2. The widest row sets the column count.
 * 3. A line without an unescaped `|` ends the table.
 */

const DELIMITER_CELL = /^:?-+:?$/;

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
