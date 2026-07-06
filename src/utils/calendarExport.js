/** Title fallback length when the note has no title (matches Android's CALENDAR_TITLE_FALLBACK_MAX_LENGTH) */
const TITLE_FALLBACK_MAX_LENGTH = 50;

/**
 * Formats a note's content as calendar event description text.
 * Checklists become one "✓ "/"• " bulleted line per non-blank item, sorted by order —
 * same bullets Android's NoteShareHelper uses. Text notes are used as-is (trimmed).
 * @param {object} note
 * @returns {string}
 */
function formatContent(note) {
  if (note.noteType === 'CHECKLIST' && Array.isArray(note.checklistItems)) {
    return note.checklistItems
      .slice()
      .sort((a, b) => a.order - b.order)
      .filter((item) => item.text.trim() !== '')
      .map((item) => `${item.isChecked ? '✓' : '•'} ${item.text.trim()}`)
      .join('\n');
  }
  return (note.content ?? '').trim();
}

/**
 * Builds the { title, description } payload for "add whole note to calendar",
 * mirroring Android's NoteEditorViewModel.openInCalendar() using live in-memory note state.
 * @param {object} note
 * @returns {{title: string, description: string}|null} null if the note is empty
 */
export function buildCalendarPayload(note) {
  const title = (note.title ?? '').trim();
  const description = formatContent(note);
  if (title === '' && description === '') return null;
  return {
    title: title || description.slice(0, TITLE_FALLBACK_MAX_LENGTH),
    description,
  };
}

/**
 * Builds the { title, description } payload for "add single checklist item to calendar",
 * mirroring Android's NoteEditorViewModel.openChecklistItemInCalendar().
 * @param {object} item - Checklist item ({ text })
 * @param {string} noteTitle - Parent note's title
 * @returns {{title: string, description: string}}
 */
export function buildCalendarPayloadForItem(item, noteTitle) {
  const trimmedNoteTitle = (noteTitle ?? '').trim();
  return {
    title: item.text.trim(),
    description: trimmedNoteTitle ? `(${trimmedNoteTitle})` : '',
  };
}
