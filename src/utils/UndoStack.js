/**
 * UndoStack – generic undo/redo history for checklist notes and title changes.
 *
 * Text-notes use CodeMirror's built-in history; this stack is only active
 * for checklist notes (title + checklistItems) and the note title in text notes.
 *
 * A snapshot is a plain object:
 *   { title: string, checklistItems: ChecklistItem[] | null }
 *
 * Max capacity: 50 entries.
 */
export class UndoStack {
  constructor(maxSize = 50) {
    this.maxSize = maxSize;
    this._stack = [];   // array of snapshots
    this._ptr = -1;     // points at the "current" snapshot
  }

  /** Deep-clone a snapshot so mutations don't affect history. */
  static _clone(snapshot) {
    return {
      title: snapshot.title,
      checklistItems: snapshot.checklistItems
        ? snapshot.checklistItems.map(item => ({ ...item }))
        : null,
    };
  }

  /**
   * Push a new snapshot.
   * Discards any redo-future (everything after current pointer).
   */
  push(snapshot) {
    // Drop everything after current pointer (kills redo branch)
    this._stack = this._stack.slice(0, this._ptr + 1);

    this._stack.push(UndoStack._clone(snapshot));

    // Trim to max capacity
    if (this._stack.length > this.maxSize) {
      this._stack.shift();
    }

    this._ptr = this._stack.length - 1;
  }

  /**
   * Move one step back in history.
   * Returns the snapshot to restore, or null if nothing to undo.
   */
  undo() {
    if (this._ptr <= 0) return null;
    this._ptr--;
    return UndoStack._clone(this._stack[this._ptr]);
  }

  /**
   * Move one step forward in history.
   * Returns the snapshot to restore, or null if nothing to redo.
   */
  redo() {
    if (this._ptr >= this._stack.length - 1) return null;
    this._ptr++;
    return UndoStack._clone(this._stack[this._ptr]);
  }

  /** True when there is at least one step to undo. */
  get canUndo() {
    return this._ptr > 0;
  }

  /** True when there is at least one step to redo. */
  get canRedo() {
    return this._ptr < this._stack.length - 1;
  }

  /** Reset the stack (call when a different note is loaded). */
  clear() {
    this._stack = [];
    this._ptr = -1;
  }
}
