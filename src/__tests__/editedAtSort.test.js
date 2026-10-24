import { describe, expect, it, vi } from 'vitest';
import { NotesList } from '../components/NotesList.js';
import { editedAt } from '../services/noteService.js';

vi.mock('../services/tauri.js');

// Android v2.21.0: Liste sortiert und zeigt die Inhaltszeit, nicht die Sync-Uhr updatedAt.
describe('editedAt sorting', () => {
  const sort = (notes, sortOption = 'UPDATED_AT', sortDirection = 'DESC') =>
    NotesList.prototype.applySortOption.call({ sortOption, sortDirection }, notes).map((n) => n.id);

  it('falls back to updatedAt for notes without contentUpdatedAt', () => {
    expect(editedAt({ updatedAt: 5 })).toBe(5);
    expect(editedAt({ updatedAt: 9, contentUpdatedAt: 3 })).toBe(3);
  });

  it('a moved note keeps its position', () => {
    const notes = [
      { id: 'moved', updatedAt: 900, contentUpdatedAt: 100 },
      { id: 'edited', updatedAt: 500, contentUpdatedAt: 500 },
      { id: 'old', updatedAt: 300 },
    ];
    expect(sort(notes)).toEqual(['edited', 'old', 'moved']);
    expect(sort(notes, 'UPDATED_AT', 'ASC')).toEqual(['moved', 'old', 'edited']);
  });

  it('secondary order within a note type uses the edited time', () => {
    const notes = [
      { id: 'a', noteType: 'TEXT', updatedAt: 900, contentUpdatedAt: 100 },
      { id: 'b', noteType: 'TEXT', updatedAt: 200 },
    ];
    expect(sort(notes, 'NOTE_TYPE', 'ASC')).toEqual(['b', 'a']);
  });
});
