import * as tauri from './tauri.js';

/**
 * Note Service - Manages notes, folders and syncing
 */
class NoteService {
  constructor() {
    this.notes = [];
    this.currentNote = null;
    this.folders = [];
    this.currentFolder = null; // null = root view
    this.listeners = new Set();
    this._saveQueues = new Map(); // note id → last in-flight save Promise
    this.trashedNotes = [];
    this.trashMode = false;
  }

  /**
   * Subscribe to note/folder changes
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
   * Load all folders from server
   */
  async loadFolders() {
    try {
      this.folders = await tauri.listFolders();
      this.notify();
      return this.folders;
    } catch (error) {
      console.error('Failed to load folders:', error);
      throw error;
    }
  }

  /** Get the current folder list */
  getFolders() {
    return this.folders;
  }

  /** Get the currently viewed folder name (null = root) */
  getCurrentFolder() {
    return this.currentFolder;
  }

  /**
   * Set the current folder view and notify.
   * @param {string|null} name - Folder name or null for root
   */
  setCurrentFolder(name) {
    this.currentFolder = name ?? null;
    this.notify();
  }

  /**
   * Returns a map of folderName → note count from the cached note list.
   * null key = root notes.
   * @returns {Map<string|null, number>}
   */
  getFolderNoteCounts() {
    const counts = new Map();
    for (const note of this.notes) {
      const key = note.folderName ?? null;
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    return counts;
  }

  /**
   * Get a specific note
   * @param {string} id - Note ID
   */
  async getNote(id) {
    try {
      // Look up cached note's folderName so we query the right server path
      const cached = this.notes.find((n) => n.id === id);
      const folderName = cached?.folderName ?? null;
      const note = await tauri.getNote(id, folderName);
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
   * Save current note
   * @param {Object} note - Note object to save
   * @returns {Promise<Object>} Updated note with new timestamp
   */
  async saveNote(note) {
    // Chain saves for the same note id so they execute in submission order.
    // Saves for different ids run concurrently.
    const prev = this._saveQueues.get(note.id) ?? Promise.resolve();
    const next = prev
      .catch(() => {}) // don't let a prior failure poison the chain
      .then(async () => {
        const updatedNote = await tauri.saveNote(note);

        // Update local cache with server response
        const index = this.notes.findIndex((n) => n.id === updatedNote.id);
        if (index >= 0) {
          this.notes[index] = { ...updatedNote };
        } else {
          this.notes.unshift(updatedNote);
        }

        // Re-sort to match server ordering: pinned first, then updatedAt desc.
        this.notes.sort((a, b) => {
          const aPin = a.isPinned ? 1 : 0;
          const bPin = b.isPinned ? 1 : 0;
          if (bPin !== aPin) return bPin - aPin;
          return b.updatedAt - a.updatedAt;
        });

        this.currentNote = updatedNote;
        this.notify();

        return updatedNote;
      });

    this._saveQueues.set(note.id, next);

    try {
      return await next;
    } finally {
      if (this._saveQueues.get(note.id) === next) {
        this._saveQueues.delete(note.id);
      }
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
      const cached = this.notes.find((n) => n.id === id);
      await tauri.deleteNote(id, cached?.folderName ?? null);

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
        const cached = this.notes.find((n) => n.id === id);
        await tauri.deleteNote(id, cached?.folderName ?? null);
        results.success.push(id);
      } catch (error) {
        console.error(`Failed to delete note ${id}:`, error);
        results.failed.push(id);
      }
    }

    this.notes = this.notes.filter((n) => !results.success.includes(n.id));

    if (this.currentNote && results.success.includes(this.currentNote.id)) {
      this.currentNote = null;
    }

    this.notify();
    return results;
  }

  /**
   * Pin or unpin multiple notes
   * @param {string[]} ids - Note IDs to update
   * @param {boolean} pinned - true = pin, false = unpin
   */
  async pinNotes(ids, pinned) {
    await tauri.pinNotes(ids, pinned, this.currentFolder);
    await this.loadNotes();
    this.notify();
  }

  /**
   * Set or remove the background color of multiple notes
   * @param {string[]} ids - Note IDs to update
   * @param {string|null} color - Hex color string or null to remove
   */
  async colorNotes(ids, color) {
    await tauri.colorNotes(ids, color, this.currentFolder);
    await this.loadNotes();
    this.notify();
  }

  /**
   * Create a folder
   * @param {string} name - Folder name
   * @param {string|null} color - Optional hex color
   * @param {boolean} localOnly - If true, never synced to server
   */
  async createFolder(name, color = null, localOnly = false) {
    this.folders = await tauri.createFolder(name, color, localOnly);
    await this.loadNotes();
    this.notify();
  }

  /**
   * Toggle a folder between server-synced and local-only.
   * @param {string} name
   * @param {boolean} localOnly
   * @param {boolean} removeFromServer - Phase 3: delete server copies or keep them
   */
  async setFolderLocalOnly(name, localOnly, removeFromServer = true) {
    this.folders = await tauri.setFolderLocalOnly(name, localOnly, removeFromServer);
    await this.loadNotes();
    this.notify();
  }

  /**
   * Run server sync and refresh local data.
   */
  async sync() {
    await tauri.sync();
    await this.loadNotes();
    await this.loadFolders();
    this.notify();
  }

  /**
   * Resolve a sync conflict.
   * @param {string} id - Note ID
   * @param {'keep_mine'|'use_server'} resolution
   */
  async resolveConflict(id, resolution) {
    const cached = this.notes.find((n) => n.id === id);
    await tauri.resolveConflict(id, resolution, cached?.folderName ?? null);
    await this.loadNotes();
    this.notify();
  }

  /**
   * Rename a folder
   * @param {string} oldName
   * @param {string} newName
   */
  async renameFolder(oldName, newName) {
    this.folders = await tauri.renameFolder(oldName, newName);
    // If we were viewing the renamed folder, follow it
    if (this.currentFolder?.toLowerCase() === oldName.toLowerCase()) {
      this.currentFolder = newName;
    }
    await this.loadNotes();
    this.notify();
  }

  /**
   * Delete a folder
   * @param {string} name
   * @param {boolean} keepNotes - true = move to root, false = delete
   */
  async deleteFolder(name, keepNotes) {
    this.folders = await tauri.deleteFolder(name, keepNotes);
    // If we were viewing the deleted folder, go back to root
    if (this.currentFolder?.toLowerCase() === name.toLowerCase()) {
      this.currentFolder = null;
    }
    await this.loadNotes();
    this.notify();
  }

  /**
   * Set or remove folder color
   * @param {string} name
   * @param {string|null} color
   */
  async setFolderColor(name, color) {
    this.folders = await tauri.setFolderColor(name, color);
    this.notify();
  }

  /**
   * Move notes to a different folder
   * @param {string[]} ids - Note IDs
   * @param {string|null} targetFolder - Target folder (null = root)
   */
  async moveNotes(ids, targetFolder) {
    await tauri.moveNotes(ids, this.currentFolder, targetFolder);
    await this.loadNotes();
    await this.loadFolders();
    this.notify();
  }

  isTrashMode() {
    return this.trashMode;
  }

  setTrashMode(on) {
    this.trashMode = on;
    this.notify();
  }

  getTrashedNotes() {
    return this.trashedNotes;
  }

  async loadTrash() {
    this.trashedNotes = await tauri.listTrash();
    this.notify();
  }

  async restoreNote(id, folderName) {
    await tauri.restoreNote(id, folderName ?? null);
    await Promise.all([this.loadTrash(), this.loadNotes()]);
  }

  async deleteNotePermanent(id, folderName) {
    await tauri.deleteNotePermanent(id, folderName ?? null);
    await this.loadTrash();
  }

  async emptyTrash() {
    await tauri.emptyTrash();
    await this.loadTrash();
  }

  /**
   * Search notes by query within the current folder view.
   * @param {string} query - Search query
   */
  searchNotes(query) {
    if (!query.trim()) {
      return this.getNotesInCurrentFolder();
    }

    const lowerQuery = query.toLowerCase();
    return this.getNotesInCurrentFolder().filter((note) => {
      if (note.title.toLowerCase().includes(lowerQuery)) return true;
      if (note.content?.toLowerCase().includes(lowerQuery)) return true;
      if (note.checklistItems?.some((item) => item.text.toLowerCase().includes(lowerQuery))) return true;
      return false;
    });
  }

  /**
   * Returns notes matching the current folder view.
   * currentFolder === null → root notes (folderName is null/undefined)
   * currentFolder !== null → notes with that folderName
   */
  getNotesInCurrentFolder() {
    const current = this.currentFolder;
    if (current === null) {
      return this.notes.filter((n) => !n.folderName);
    }
    return this.notes.filter((n) => n.folderName?.toLowerCase() === current.toLowerCase());
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
