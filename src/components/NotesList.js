import noteService from '../services/noteService.js';

/**
 * Notes List Component
 */
export class NotesList {
  constructor() {
    this.container = document.getElementById('notes-list');
    this.searchInput = document.getElementById('search-input');
    this.selectedId = null;
    this.onSelectCallback = null;
    
    this.init();
  }

  init() {
    // Subscribe to note changes
    noteService.subscribe(() => this.render());
    
    // Search input
    this.searchInput.addEventListener('input', (e) => {
      this.render(e.target.value);
    });
  }

  render(searchQuery = '') {
    const notes = searchQuery
      ? noteService.searchNotes(searchQuery)
      : noteService.getNotes();
    
    if (notes.length === 0) {
      this.container.innerHTML = '<div style="padding: 1rem; text-align: center; color: #999;">No notes found</div>';
      return;
    }
    
    this.container.innerHTML = notes.map(note => this.renderNoteItem(note)).join('');
    
    // Add click handlers
    this.container.querySelectorAll('.note-item').forEach(item => {
      item.addEventListener('click', () => {
        const id = item.dataset.id;
        this.selectNote(id);
      });
    });
  }

  renderNoteItem(note) {
    const preview = this.getPreview(note);
    const date = this.formatDate(note.updatedAt);
    const selected = note.id === this.selectedId ? 'selected' : '';
    
    return `
      <div class="note-item ${selected}" data-id="${note.id}">
        <div class="note-item-title">${this.escapeHtml(note.title)}</div>
        <div class="note-item-preview">${this.escapeHtml(preview)}</div>
        <div class="note-item-meta">${date}</div>
      </div>
    `;
  }

  getPreview(note) {
    if (note.noteType === 'CHECKLIST' && note.checklistItems) {
      const total = note.checklistItems.length;
      const checked = note.checklistItems.filter(item => item.isChecked).length;
      return `☑ ${checked}/${total} completed`;
    }
    
    return note.content
      ? note.content.substring(0, 100).replace(/\n/g, ' ')
      : 'Empty note';
  }

  formatDate(timestamp) {
    if (!timestamp) return '';
    
    // Timestamp is in milliseconds
    const date = new Date(timestamp);
    const now = new Date();
    const diffMs = now - date;
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);
    
    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays < 7) return `${diffDays}d ago`;
    
    return date.toLocaleDateString('en-US', {
      day: '2-digit',
      month: '2-digit',
      year: 'numeric'
    });
  }

  escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }

  async selectNote(id) {
    this.selectedId = id;
    this.render(this.searchInput.value);
    
    if (this.onSelectCallback) {
      try {
        const note = await noteService.getNote(id);
        this.onSelectCallback(note);
      } catch (error) {
        console.error('Failed to load note:', error);
      }
    }
  }

  onSelect(callback) {
    this.onSelectCallback = callback;
  }

  async refresh() {
    try {
      await noteService.loadNotes();
    } catch (error) {
      console.error('Failed to refresh notes:', error);
    }
  }
}
