import * as tauri from './tauri.js';

/**
 * Note Service - Manages notes and syncing with conflict detection
 */
class NoteService {
  constructor() {
    this.notes = [];
    this.currentNote = null;
    this.listeners = new Set();
    this.conflictHandler = null;
  }

  /**
   * Set conflict handler callback
   * @param {Function} handler - Callback when conflict is detected
   */
  setConflictHandler(handler) {
    this.conflictHandler = handler;
  }

  /**
   * Subscribe to note changes
   * @param {Function} listener - Callback function
   */
  subscribe(listener) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /**
   * Notify all listeners
   */
  notify() {
    this.listeners.forEach((listener) => {
      listener();
    });
  }

  /**
   * Load all notes from server
   */
  async loadNotes() {
    try {
      this.notes = await tauri.listNotes();
      this.notify();
      return this.notes;
    } catch (error) {
      console.error('Failed to load notes:', error);
      throw error;
    }
  }

  /**
   * Get a specific note
   * @param {string} id - Note ID
   */
  async getNote(id) {
    try {
      const note = await tauri.getNote(id);
      // Update sync base after loading from server
      try {
        await tauri.updateSyncBaseAfterLoad(note);
      } catch (syncError) {
        console.warn('Failed to update sync base:', syncError);
      }
      this.currentNote = note;
      this.notify();
      return note;
    } catch (error) {
      console.error('Failed to get note:', error);
      throw error;
    }
  }

  /**
   * Create a new note
   * @param {string} title - Note title
   * @param {string} type - Note type: "TEXT" or "CHECKLIST"
   */
  async createNote(title, type = 'TEXT') {
    try {
      const note = await tauri.createNote(title, type);
      this.currentNote = note;
      this.notes.unshift(note);
      this.notify();
      return note;
    } catch (error) {
      console.error('Failed to create note:', error);
      throw error;
    }
  }

  /**
   * Check if error is a sync conflict
   * @param {Error} error - The error to check
   * @returns {boolean}
   */
  isSyncConflict(error) {
    if (!error) return false;
    const message = error.message || String(error);
    return message.includes('Sync conflict') || message.includes('SyncConflict');
  }

  /**
   * Save current note with conflict detection
   * @param {Object} note - Note object to save
   * @returns {Promise<Object>} Updated note with new timestamp
   * @throws {Error} If save fails or conflict is cancelled
   */
  async saveNote(note) {
    try {
      // Backend updates timestamp and returns updated note
      const updatedNote = await tauri.saveNote(note);

      // Update local cache with server response
      const index = this.notes.findIndex((n) => n.id === updatedNote.id);
      if (index >= 0) {
        this.notes[index] = { ...updatedNote };
      } else {
        this.notes.unshift(updatedNote);
      }

      this.currentNote = updatedNote;
      this.notify();

      return updatedNote;
    } catch (error) {
      console.error('Failed to save note:', error);

      // Check if this is a sync conflict
      if (this.isSyncConflict(error)) {
        console.log('Sync conflict detected for note:', note.id);

        // If conflict handler is set, let it handle the conflict
        if (this.conflictHandler) {
          try {
            const resolvedNote = await this.conflictHandler(note);

            // If resolvedNote is null, user cancelled the conflict dialog
            // We should throw an error to indicate save was not completed
            if (!resolvedNote) {
              const cancelError = new Error('Conflict resolution cancelled by user');
              cancelError.name = 'ConflictCancelledError';
              throw cancelError;
            }

            // Update local cache with resolved note
            const idx = this.notes.findIndex((n) => n.id === resolvedNote.id);
            if (idx >= 0) {
              this.notes[idx] = { ...resolvedNote };
            } else {
              this.notes.unshift(resolvedNote);
            }

            this.currentNote = resolvedNote;
            this.notify();

            return resolvedNote;
          } catch (resolveError) {
            console.error('Conflict resolution failed:', resolveError);
            throw resolveError;
          }
        }

        // Re-throw the conflict error for the caller to handle
        throw error;
      }

      throw error;
    }
  }

  /**
   * Update a note in the local cache immediately (without server save).
   * Used for real-time sidebar updates while editing.
   * @param {Object} note - Note object with current changes
   */
  updateNoteLocally(note) {
    const index = this.notes.findIndex((n) => n.id === note.id);
    if (index >= 0) {
      this.notes[index] = { ...this.notes[index], ...note };
    }
    this.notify();
  }

  /**
   * Delete a note
   * @param {string} id - Note ID
   */
  async deleteNote(id) {
    try {
      await tauri.deleteNote(id);

      // Remove from local cache
      this.notes = this.notes.filter((n) => n.id !== id);

      if (this.currentNote?.id === id) {
        this.currentNote = null;
      }

      this.notify();
    } catch (error) {
      console.error('Failed to delete note:', error);
      throw error;
    }
  }

  /**
   * F6: Delete multiple notes
   * @param {string[]} ids - Array of note IDs
   * @returns {Object} - {success: string[], failed: string[]}
   */
  async deleteNotes(ids) {
    const results = { success: [], failed: [] };

    for (const id of ids) {
      try {
        await tauri.deleteNote(id);
        results.success.push(id);
      } catch (error) {
        console.error(`Failed to delete note ${id}:`, error);
        results.failed.push(id);
      }
    }

    // Update local state for successful deletions
    this.notes = this.notes.filter((n) => !results.success.includes(n.id));

    if (this.currentNote && results.success.includes(this.currentNote.id)) {
      this.currentNote = null;
    }

    this.notify();
    return results;
  }

  /**
   * Search notes by query
   * @param {string} query - Search query
   */
  searchNotes(query) {
    if (!query.trim()) {
      return this.notes;
    }

    const lowerQuery = query.toLowerCase();
    return this.notes.filter((note) => {
      // Title match
      if (note.title.toLowerCase().includes(lowerQuery)) return true;

      // Content match (TEXT notes)
      if (note.content?.toLowerCase().includes(lowerQuery)) return true;

      // F1: Checklist items match
      if (note.checklistItems?.some((item) => item.text.toLowerCase().includes(lowerQuery))) return true;

      return false;
    });
  }

  /**
   * Get current note
   */
  getCurrentNote() {
    return this.currentNote;
  }

  /**
   * Get all notes
   */
  getNotes() {
    return this.notes;
  }
}

export default new NoteService();
