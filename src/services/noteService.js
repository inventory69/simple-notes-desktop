import * as tauri from './tauri.js';

/**
 * Note Service - Manages notes and syncing
 */
class NoteService {
  constructor() {
    this.notes = [];
    this.currentNote = null;
    this.listeners = new Set();
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
    this.listeners.forEach(listener => listener());
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
    try {
      // Backend updates timestamp and returns updated note
      const updatedNote = await tauri.saveNote(note);
      
      // Update local cache with server response
      const index = this.notes.findIndex(n => n.id === updatedNote.id);
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
      throw error;
    }
  }

  /**
   * Delete a note
   * @param {string} id - Note ID
   */
  async deleteNote(id) {
    try {
      await tauri.deleteNote(id);
      
      // Remove from local cache
      this.notes = this.notes.filter(n => n.id !== id);
      
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
   * Search notes by query
   * @param {string} query - Search query
   */
  searchNotes(query) {
    if (!query.trim()) {
      return this.notes;
    }
    
    const lowerQuery = query.toLowerCase();
    return this.notes.filter(note => 
      note.title.toLowerCase().includes(lowerQuery) ||
      note.content?.toLowerCase().includes(lowerQuery)
    );
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
