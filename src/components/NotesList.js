import noteService from '../services/noteService.js';
import { dialogService } from '../services/DialogService.js';

/**
 * Notes List Component with Multi-Select support (F6)
 */
export class NotesList {
  constructor() {
    this.container = document.getElementById('notes-list');
    this.searchInput = document.getElementById('search-input');
    this.selectedId = null;
    this.onSelectCallback = null;

    // F6: Multi-Select State
    this.selectionMode = false;
    this.selectedIds = new Set();
    this.lastSelectedId = null;
    this.onSelectionChangeCallback = null;

    this.init();
  }

  init() {
    // Subscribe to note changes
    noteService.subscribe(() => this.render());

    // Search input
    this.searchInput.addEventListener('input', (e) => {
      this.render(e.target.value);
    });

    // F6: Keyboard shortcuts
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && this.selectionMode) {
        e.stopPropagation();
        this.exitSelectionMode();
      }
    });
  }

  // F6: Enter selection mode
  enterSelectionMode() {
    this.selectionMode = true;
    this.selectedIds.clear();
    this.lastSelectedId = null;
    this.render(this.searchInput.value);
    this.notifySelectionChange();
  }

  // F6: Exit selection mode
  exitSelectionMode() {
    this.selectionMode = false;
    this.selectedIds.clear();
    this.lastSelectedId = null;
    this.render(this.searchInput.value);
    this.notifySelectionChange();
  }

  // F6: Toggle selection of a note
  toggleSelection(id) {
    if (this.selectedIds.has(id)) {
      this.selectedIds.delete(id);
    } else {
      this.selectedIds.add(id);
    }
    this.lastSelectedId = id;

    // Exit selection mode if no items selected
    if (this.selectedIds.size === 0) {
      this.exitSelectionMode();
      return;
    }

    this.render(this.searchInput.value);
    this.notifySelectionChange();
  }

  // F6: Range selection (Shift+Click)
  selectRange(targetId) {
    if (!this.lastSelectedId) {
      this.toggleSelection(targetId);
      return;
    }

    const notes = this.searchInput.value
      ? noteService.searchNotes(this.searchInput.value)
      : noteService.getNotes();

    const lastIndex = notes.findIndex(n => n.id === this.lastSelectedId);
    const targetIndex = notes.findIndex(n => n.id === targetId);

    if (lastIndex === -1 || targetIndex === -1) {
      this.toggleSelection(targetId);
      return;
    }

    const start = Math.min(lastIndex, targetIndex);
    const end = Math.max(lastIndex, targetIndex);

    for (let i = start; i <= end; i++) {
      this.selectedIds.add(notes[i].id);
    }

    this.lastSelectedId = targetId;
    this.render(this.searchInput.value);
    this.notifySelectionChange();
  }

  // F6: Get selected note IDs
  getSelectedIds() {
    return Array.from(this.selectedIds);
  }

  // F6: Delete selected notes (uses F4's confirmDeletion dialog)
  async deleteSelected() {
    const count = this.selectedIds.size;
    if (count === 0) return;

    const confirmed = await dialogService.confirmDeletion(count);
    if (!confirmed) return;

    const ids = this.getSelectedIds();
    const results = await noteService.deleteNotes(ids);

    if (results.failed.length > 0) {
      await dialogService.warning({
        title: 'Partial Deletion',
        message: `${results.success.length} notes deleted. ${results.failed.length} notes could not be deleted.`
      });
    }

    this.exitSelectionMode();
  }

  // F6: Notify parent about selection changes
  notifySelectionChange() {
    if (this.onSelectionChangeCallback) {
      this.onSelectionChangeCallback({
        selectionMode: this.selectionMode,
        count: this.selectedIds.size
      });
    }
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
      item.addEventListener('click', (e) => {
        const id = item.dataset.id;

        if (this.selectionMode) {
          if (e.shiftKey) {
            this.selectRange(id);
          } else {
            this.toggleSelection(id);
          }
          return;
        }

        // Ctrl+Click enters selection mode
        if (e.ctrlKey || e.metaKey) {
          this.enterSelectionMode();
          this.toggleSelection(id);
          return;
        }

        this.selectNote(id);
      });

      // Long-press to enter selection mode (500ms hold)
      let pressTimer = null;
      item.addEventListener('mousedown', (e) => {
        if (e.button !== 0) return;
        pressTimer = setTimeout(() => {
          const id = item.dataset.id;
          if (!this.selectionMode) {
            this.enterSelectionMode();
          }
          this.toggleSelection(id);
          pressTimer = null;
        }, 500);
      });
      item.addEventListener('mouseup', () => {
        if (pressTimer) { clearTimeout(pressTimer); pressTimer = null; }
      });
      item.addEventListener('mouseleave', () => {
        if (pressTimer) { clearTimeout(pressTimer); pressTimer = null; }
      });
    });
  }

  renderNoteItem(note) {
    const preview = this.getPreview(note);
    const date = this.formatDate(note.updatedAt);
    const isActive = note.id === this.selectedId && !this.selectionMode;
    const isSelected = this.selectedIds.has(note.id);

    let classes = 'note-item';
    if (isActive) classes += ' selected';
    if (isSelected) classes += ' multi-selected';

    // F1: Note Type Icon
    const typeIcon = note.noteType === 'CHECKLIST'
      ? `<svg class="note-type-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
           <polyline points="9 11 12 14 22 4"></polyline>
           <path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11"></path>
         </svg>`
      : `<svg class="note-type-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
           <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path>
           <polyline points="14 2 14 8 20 8"></polyline>
           <line x1="16" y1="13" x2="8" y2="13"></line>
           <line x1="16" y1="17" x2="8" y2="17"></line>
           <polyline points="10 9 9 9 8 9"></polyline>
         </svg>`;

    return `
      <div class="${classes}" data-id="${note.id}">
        ${this.selectionMode ? `<div class="note-item-checkbox">
            ${isSelected
              ? `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                   <rect x="3" y="3" width="18" height="18" rx="3" ry="3" fill="currentColor" stroke="currentColor"/>
                   <polyline points="9 12 11 14 15 10" stroke="white" stroke-width="2.5"/>
                 </svg>`
              : `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                   <rect x="3" y="3" width="18" height="18" rx="3" ry="3"/>
                 </svg>`
            }
          </div>` : ''}
        <div class="note-item-content">
          <div class="note-item-header">
            ${typeIcon}
            <div class="note-item-title">${this.escapeHtml(note.title)}</div>
          </div>
          <div class="note-item-preview">${this.escapeHtml(preview)}</div>
          <div class="note-item-meta">${date}</div>
        </div>
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

  // F6: Selection change callback
  onSelectionChange(callback) {
    this.onSelectionChangeCallback = callback;
  }

  async refresh() {
    try {
      await noteService.loadNotes();
    } catch (error) {
      console.error('Failed to refresh notes:', error);
    }
  }
}
