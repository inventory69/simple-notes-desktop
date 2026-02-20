import { beforeEach, describe, expect, it, vi } from 'vitest';
import noteService from '../services/noteService.js';
import * as tauri from '../services/tauri.js';

vi.mock('../services/tauri.js');

describe('NoteService', () => {
  beforeEach(() => {
    noteService.notes = [];
    noteService.currentNote = null;
    vi.clearAllMocks();
  });

  describe('loadNotes', () => {
    it('should load notes from server', async () => {
      const mockNotes = [
        { id: '1', title: 'Note 1', content: 'Content 1', updated_at: 1234567890 },
        { id: '2', title: 'Note 2', content: 'Content 2', updated_at: 1234567891 },
      ];

      tauri.listNotes.mockResolvedValue(mockNotes);

      const notes = await noteService.loadNotes();

      expect(notes).toEqual(mockNotes);
      expect(noteService.notes).toEqual(mockNotes);
      expect(tauri.listNotes).toHaveBeenCalledOnce();
    });

    it('should throw error if loading fails', async () => {
      tauri.listNotes.mockRejectedValue(new Error('Network error'));

      await expect(noteService.loadNotes()).rejects.toThrow('Network error');
    });
  });

  describe('createNote', () => {
    it('should create a text note', async () => {
      const mockNote = {
        id: 'new-1',
        title: 'New Note',
        content: '',
        note_type: 'TEXT',
        updated_at: Date.now(),
      };

      tauri.createNote.mockResolvedValue(mockNote);

      const note = await noteService.createNote('New Note', 'TEXT');

      expect(note).toEqual(mockNote);
      expect(noteService.currentNote).toEqual(mockNote);
      expect(noteService.notes[0]).toEqual(mockNote);
      expect(tauri.createNote).toHaveBeenCalledWith('New Note', 'TEXT');
    });

    it('should create a checklist note', async () => {
      const mockNote = {
        id: 'new-2',
        title: 'New Checklist',
        content: '',
        note_type: 'CHECKLIST',
        checklist_items: [],
        updated_at: Date.now(),
      };

      tauri.createNote.mockResolvedValue(mockNote);

      const note = await noteService.createNote('New Checklist', 'CHECKLIST');

      expect(note.note_type).toBe('CHECKLIST');
      expect(tauri.createNote).toHaveBeenCalledWith('New Checklist', 'CHECKLIST');
    });
  });

  describe('saveNote', () => {
    it('should save note and update cache with returned note', async () => {
      const originalNote = {
        id: '1',
        title: 'Updated Note',
        content: 'Updated content',
        updatedAt: 1000,
      };

      // Backend returns note with updated timestamp
      const updatedNote = {
        ...originalNote,
        updatedAt: 2000,
      };

      tauri.saveNote.mockResolvedValue(updatedNote);

      const result = await noteService.saveNote(originalNote);

      expect(tauri.saveNote).toHaveBeenCalledWith(originalNote);
      expect(result).toEqual(updatedNote);
      expect(noteService.currentNote).toEqual(updatedNote);
      expect(noteService.notes[0]).toEqual(updatedNote);
    });

    it('should update existing note in cache', async () => {
      noteService.notes = [
        { id: '1', title: 'Old Title', updatedAt: 1000 },
        { id: '2', title: 'Other Note', updatedAt: 1000 },
      ];

      const originalNote = { id: '1', title: 'New Title', updatedAt: 1000 };
      const updatedNote = { id: '1', title: 'New Title', updatedAt: 2000 };

      tauri.saveNote.mockResolvedValue(updatedNote);

      await noteService.saveNote(originalNote);

      expect(noteService.notes[0].title).toBe('New Title');
      expect(noteService.notes[0].updatedAt).toBe(2000);
      expect(noteService.notes.length).toBe(2);
    });
  });

  describe('deleteNote', () => {
    beforeEach(() => {
      noteService.notes = [
        { id: '1', title: 'Note 1' },
        { id: '2', title: 'Note 2' },
      ];
      noteService.currentNote = { id: '1', title: 'Note 1' };
    });

    it('should delete note and remove from cache', async () => {
      tauri.deleteNote.mockResolvedValue();

      await noteService.deleteNote('1');

      expect(tauri.deleteNote).toHaveBeenCalledWith('1');
      expect(noteService.notes.length).toBe(1);
      expect(noteService.notes[0].id).toBe('2');
      expect(noteService.currentNote).toBeNull();
    });

    it('should not clear currentNote if different note deleted', async () => {
      tauri.deleteNote.mockResolvedValue();

      await noteService.deleteNote('2');

      expect(noteService.currentNote.id).toBe('1');
    });
  });

  describe('searchNotes', () => {
    beforeEach(() => {
      noteService.notes = [
        { id: '1', title: 'JavaScript Tutorial', content: 'Learn JS' },
        { id: '2', title: 'Python Guide', content: 'Learn Python' },
        { id: '3', title: 'Learning Rust', content: 'Systems programming' },
      ];
    });

    it('should return all notes for empty query', () => {
      const results = noteService.searchNotes('');
      expect(results.length).toBe(3);
    });

    it('should search by title', () => {
      const results = noteService.searchNotes('Python');
      expect(results.length).toBe(1);
      expect(results[0].title).toBe('Python Guide');
    });

    it('should search by content', () => {
      const results = noteService.searchNotes('programming');
      expect(results.length).toBe(1);
      expect(results[0].title).toBe('Learning Rust');
    });

    it('should be case insensitive', () => {
      const results = noteService.searchNotes('javascript');
      expect(results.length).toBe(1);
      expect(results[0].title).toBe('JavaScript Tutorial');
    });

    it('should return multiple matches', () => {
      const results = noteService.searchNotes('Learn');
      expect(results.length).toBe(3);
    });
  });

  describe('subscribe', () => {
    it('should notify listeners on changes', async () => {
      const listener = vi.fn();
      noteService.subscribe(listener);

      tauri.listNotes.mockResolvedValue([]);
      await noteService.loadNotes();

      expect(listener).toHaveBeenCalled();
    });

    it('should allow unsubscribing', async () => {
      const listener = vi.fn();
      const unsubscribe = noteService.subscribe(listener);

      unsubscribe();

      tauri.listNotes.mockResolvedValue([]);
      await noteService.loadNotes();

      expect(listener).not.toHaveBeenCalled();
    });
  });
});
