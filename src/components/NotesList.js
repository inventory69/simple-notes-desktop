import { dialogService } from '../services/DialogService.js';
import noteService from '../services/noteService.js';
import { colorPicker } from '../utils/ColorPicker.js';
import { getColorPair, NOTE_COLORS } from '../utils/noteColors.js';

/**
 * Notes List Component with Multi-Select support (F6) and Folders (F-FOLDERS)
 */
export class NotesList {
  constructor() {
    this.container = document.getElementById('notes-list');
    this.searchInput = document.getElementById('search-input');
    this.sortBtn = document.getElementById('list-sort-btn');
    this.selectedId = null;
    this.onSelectCallback = null;

    // F6: Multi-Select State
    this.selectionMode = false;
    this.selectedIds = new Set();
    this.lastSelectedId = null;
    this.onSelectionChangeCallback = null;

    // Sortierung: persistiert in localStorage
    this.sortOption = localStorage.getItem('noteListSortOption') || 'UPDATED_AT';
    this.sortDirection = localStorage.getItem('noteListSortDirection') || 'DESC';
    this._sortMenuCloseHandler = null;
    this._renderedFolder = undefined;

    this.init();
  }

  init() {
    // Subscribe to note/folder changes
    noteService.subscribe(() => this.render());

    // Search input
    this.searchInput.addEventListener('input', (e) => {
      this.render(e.target.value);
    });

    // Sort button
    this.sortBtn.addEventListener('click', () => this.showSortMenu());
    this._updateSortBtnState();

    // F6: Keyboard shortcuts
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && this.selectionMode) {
        e.stopPropagation();
        this.exitSelectionMode();
      }
      // Escape inside a folder → go back to root
      if (e.key === 'Escape' && !this.selectionMode && noteService.getCurrentFolder() !== null) {
        noteService.setCurrentFolder(null);
      }
    });

    setInterval(() => this._refreshTimestamps(), 60000);
  }

  _refreshTimestamps() {
    for (const el of this.container.querySelectorAll('.note-item-date[data-ts]')) {
      el.textContent = this.formatDate(Number(el.dataset.ts));
    }
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

    const rawNotes = this.searchInput.value
      ? noteService.searchNotes(this.searchInput.value)
      : noteService.getNotesInCurrentFolder();
    const notes = this.applySortOption(rawNotes);

    const lastIndex = notes.findIndex((n) => n.id === this.lastSelectedId);
    const targetIndex = notes.findIndex((n) => n.id === targetId);

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

  // F6: Pin/Unpin selected notes
  async pinSelected(pinned) {
    const count = this.selectedIds.size;
    if (count === 0) return;
    const ids = this.getSelectedIds();
    try {
      await noteService.pinNotes(ids, pinned);
    } catch (e) {
      console.error('[NotesList] pinSelected failed:', e);
      await dialogService.error({ title: 'Pin Failed', message: e.message || 'Could not update notes.' });
    }
    this.exitSelectionMode();
  }

  // Farbe mehrerer Notizen setzen
  colorSelected() {
    if (this.selectedIds.size === 0) return;
    const btn = document.getElementById('batch-color-btn');
    colorPicker.show(btn, null, async (color) => {
      const ids = this.getSelectedIds();
      try {
        await noteService.colorNotes(ids, color);
      } catch (e) {
        console.error('[NotesList] colorSelected failed:', e);
        await dialogService.error({ title: 'Color Failed', message: e.message || 'Could not update notes.' });
      }
      this.exitSelectionMode();
    });
  }

  // F6: Delete selected notes
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
        message: `${results.success.length} notes deleted. ${results.failed.length} notes could not be deleted.`,
      });
    }

    this.exitSelectionMode();
  }

  // Move selected notes to a folder
  async moveSelected() {
    if (this.selectedIds.size === 0) return;
    const ids = this.getSelectedIds();
    const folders = noteService.getFolders();

    const target = await dialogService.chooseFolder({
      folders,
      currentFolder: noteService.getCurrentFolder(),
    });

    if (target === undefined) return; // cancelled

    try {
      await noteService.moveNotes(ids, target);
    } catch (e) {
      console.error('[NotesList] moveSelected failed:', e);
      await dialogService.error({ title: 'Move Failed', message: e.message || 'Could not move notes.' });
    }
    this.exitSelectionMode();
  }

  // F6: Notify parent about selection changes
  notifySelectionChange() {
    if (this.onSelectionChangeCallback) {
      this.onSelectionChangeCallback({
        selectionMode: this.selectionMode,
        count: this.selectedIds.size,
      });
    }
  }

  render(searchQuery = '') {
    const currentFolder = noteService.getCurrentFolder();
    if (currentFolder !== this._renderedFolder) {
      this.container.scrollTop = 0;
      this._renderedFolder = currentFolder;
    }
    const html = [];

    if (searchQuery) {
      // Searching: flat results within current folder
      const rawNotes = noteService.searchNotes(searchQuery);
      const notes = this.applySortOption(rawNotes);
      if (notes.length === 0) {
        this.container.innerHTML = '<div style="padding: 1rem; text-align: center; color: #999;">No notes found</div>';
        return;
      }
      for (const note of notes) {
        html.push(this.renderNoteItem(note));
      }
      this.container.innerHTML = html.join('');
      this._attachNoteHandlers();
      return;
    }

    if (currentFolder === null) {
      // Root view: Pinned → Folders → Notes
      this._renderRootView(html);
    } else {
      // Folder view: back header → Pinned → Notes
      this._renderFolderView(html, currentFolder);
    }

    this.container.innerHTML = html.join('');
    this._attachNoteHandlers();
    this._attachFolderHandlers();
  }

  _renderRootView(html) {
    const rootNotes = noteService.getNotesInCurrentFolder();
    const sortedNotes = this.applySortOption(rootNotes);
    const folders = noteService.getFolders();
    const counts = noteService.getFolderNoteCounts();

    const pinnedNotes = sortedNotes.filter((n) => n.isPinned);
    const unpinnedNotes = sortedNotes.filter((n) => !n.isPinned);

    if (pinnedNotes.length > 0) {
      html.push('<div class="notes-section-header">Pinned</div>');
      for (const note of pinnedNotes) {
        html.push(this.renderNoteItem(note));
      }
    }

    if (folders.length > 0) {
      html.push('<div class="notes-section-header">Folders</div>');
      for (const folder of folders) {
        html.push(this.renderFolderCard(folder, counts.get(folder.name) ?? 0));
      }
    }

    if (unpinnedNotes.length > 0) {
      if (pinnedNotes.length > 0 || folders.length > 0) {
        html.push('<div class="notes-section-header">Notes</div>');
      }
      for (const note of unpinnedNotes) {
        html.push(this.renderNoteItem(note));
      }
    }

    if (pinnedNotes.length === 0 && folders.length === 0 && unpinnedNotes.length === 0) {
      html.push('<div style="padding: 1rem; text-align: center; color: #999;">No notes found</div>');
    }
  }

  _renderFolderView(html, folderName) {
    const folderNotes = noteService.getNotesInCurrentFolder();
    const sortedNotes = this.applySortOption(folderNotes);

    // Back header
    html.push(`
      <div class="folder-back-header" id="folder-back-btn">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <polyline points="15 18 9 12 15 6"></polyline>
        </svg>
        <span>${this.escapeHtml(folderName)}</span>
      </div>
    `);

    const pinnedNotes = sortedNotes.filter((n) => n.isPinned);
    const unpinnedNotes = sortedNotes.filter((n) => !n.isPinned);

    if (pinnedNotes.length > 0) {
      html.push('<div class="notes-section-header">Pinned</div>');
      for (const note of pinnedNotes) {
        html.push(this.renderNoteItem(note));
      }
    }

    if (pinnedNotes.length > 0 && unpinnedNotes.length > 0) {
      html.push('<div class="notes-section-header">Notes</div>');
    }

    for (const note of unpinnedNotes) {
      html.push(this.renderNoteItem(note));
    }

    if (pinnedNotes.length === 0 && unpinnedNotes.length === 0) {
      html.push('<div style="padding: 1rem; text-align: center; color: #999;">This folder is empty</div>');
    }
  }

  renderFolderCard(folder, count) {
    const colorPair = folder.color ? getColorPair(folder.color) : null;
    const colorStyle = colorPair ? `style="--nc-l:${colorPair.light};--nc-d:${colorPair.dark}"` : '';
    const colorClass = colorPair ? ' has-color' : '';

    const colorSwatch = folder.color
      ? `<span class="folder-color-swatch" style="background:${folder.color}"></span>`
      : '';

    return `
      <div class="folder-card${colorClass}" data-folder="${this.escapeHtml(folder.name)}" ${colorStyle}>
        <div class="folder-card-main">
          <svg class="folder-icon" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path>
          </svg>
          ${colorSwatch}
          <span class="folder-card-name">${this.escapeHtml(folder.name)}</span>
          <span class="folder-card-count">${count}</span>
        </div>
        <button class="folder-menu-btn btn-icon-small" data-folder="${this.escapeHtml(folder.name)}" title="Folder options" type="button" aria-label="Folder options">⋯</button>
      </div>
    `;
  }

  _attachFolderHandlers() {
    // Back button
    const backBtn = this.container.querySelector('#folder-back-btn');
    if (backBtn) {
      backBtn.addEventListener('click', () => noteService.setCurrentFolder(null));
    }

    // Folder cards — click to enter, menu button for options
    this.container.querySelectorAll('.folder-card').forEach((card) => {
      card.addEventListener('click', (e) => {
        if (e.target.closest('.folder-menu-btn')) return;
        const folderName = card.dataset.folder;
        noteService.setCurrentFolder(folderName);
      });
    });

    this.container.querySelectorAll('.folder-menu-btn').forEach((btn) => {
      btn.addEventListener('click', (e) => {
        e.stopPropagation();
        this._showFolderMenu(btn, btn.dataset.folder);
      });
    });
  }

  _showFolderMenu(anchorBtn, folderName) {
    // Remove any existing menu
    for (const m of document.querySelectorAll('.folder-options-menu')) m.remove();

    const menu = document.createElement('div');
    menu.className = 'sort-menu folder-options-menu';
    menu.innerHTML = `
      <div class="sort-menu-item" data-action="rename">Rename</div>
      <div class="sort-menu-item" data-action="color">Set color</div>
      <div class="sort-menu-separator"></div>
      <div class="sort-menu-item sort-menu-item-danger" data-action="delete">Delete folder</div>
    `;

    const rect = anchorBtn.getBoundingClientRect();
    menu.style.position = 'fixed';
    menu.style.top = `${rect.bottom + 4}px`;
    menu.style.right = `${window.innerWidth - rect.right}px`;
    document.body.appendChild(menu);

    const folder = noteService.getFolders().find((f) => f.name === folderName);

    menu.addEventListener('click', async (e) => {
      const item = e.target.closest('.sort-menu-item');
      if (!item) return;
      menu.remove();
      closeHandler && document.removeEventListener('click', closeHandler);

      switch (item.dataset.action) {
        case 'rename':
          await this._handleRenameFolder(folderName);
          break;
        case 'color':
          await this._handleSetFolderColor(folderName, anchorBtn, folder?.color ?? null);
          break;
        case 'delete':
          await this._handleDeleteFolder(folderName);
          break;
      }
    });

    let closeHandler;
    closeHandler = (e) => {
      if (!menu.contains(e.target) && e.target !== anchorBtn) {
        menu.remove();
        document.removeEventListener('click', closeHandler);
      }
    };
    setTimeout(() => document.addEventListener('click', closeHandler), 0);
  }

  async _handleRenameFolder(oldName) {
    const newName = await dialogService.promptFolderName({
      title: 'Rename Folder',
      defaultValue: oldName,
    });
    if (!newName || newName === oldName) return;
    try {
      await noteService.renameFolder(oldName, newName);
    } catch (e) {
      await dialogService.error({ title: 'Rename Failed', message: e.message || String(e) });
    }
  }

  async _handleSetFolderColor(folderName, anchorBtn, currentColor) {
    colorPicker.show(anchorBtn, currentColor, async (color) => {
      try {
        await noteService.setFolderColor(folderName, color);
      } catch (e) {
        await dialogService.error({ title: 'Color Failed', message: e.message || String(e) });
      }
    });
  }

  async _handleDeleteFolder(folderName) {
    const counts = noteService.getFolderNoteCounts();
    const noteCount = counts.get(folderName) ?? 0;
    const result = await dialogService.confirmFolderDelete({ folderName, isNotEmpty: noteCount > 0 });
    if (!result.confirmed) return;
    try {
      await noteService.deleteFolder(folderName, result.keepNotes);
    } catch (e) {
      await dialogService.error({ title: 'Delete Failed', message: e.message || String(e) });
    }
  }

  _attachNoteHandlers() {
    this.container.querySelectorAll('.note-item').forEach((item) => {
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
        if (pressTimer) {
          clearTimeout(pressTimer);
          pressTimer = null;
        }
      });
      item.addEventListener('mouseleave', () => {
        if (pressTimer) {
          clearTimeout(pressTimer);
          pressTimer = null;
        }
      });
    });
  }

  // Sortieroptionen für die Notizliste (Android-Parität)
  static get SORT_OPTIONS() {
    return [
      { value: 'UPDATED_AT', label: 'Last modified' },
      { value: 'CREATED_AT', label: 'Last created' },
      { value: 'TITLE', label: 'Alphabetical (A→Z)' },
      { value: 'NOTE_TYPE', label: 'By note type' },
      { value: 'COLOR', label: 'By color' },
    ];
  }

  applySortOption(notes) {
    const pinned = notes.filter((n) => n.isPinned);
    const rest = notes.filter((n) => !n.isPinned);
    const asc = this.sortDirection === 'ASC';

    switch (this.sortOption) {
      case 'CREATED_AT':
        rest.sort((a, b) => (asc ? a.createdAt - b.createdAt : b.createdAt - a.createdAt));
        break;
      case 'TITLE':
        rest.sort((a, b) => {
          const cmp = a.title.localeCompare(b.title);
          return asc ? cmp : -cmp;
        });
        break;
      case 'NOTE_TYPE':
        rest.sort((a, b) => {
          const ta = a.noteType === 'TEXT' ? 0 : 1;
          const tb = b.noteType === 'TEXT' ? 0 : 1;
          if (ta !== tb) return asc ? ta - tb : tb - ta;
          return b.updatedAt - a.updatedAt;
        });
        break;
      case 'COLOR': {
        const colorIdx = (hex) => {
          if (!hex) return NOTE_COLORS.length;
          const i = NOTE_COLORS.findIndex((c) => c.light.toLowerCase() === hex.toLowerCase());
          return i === -1 ? NOTE_COLORS.length : i;
        };
        rest.sort((a, b) => {
          const diff = colorIdx(a.color) - colorIdx(b.color);
          if (diff !== 0) return asc ? diff : -diff;
          return b.updatedAt - a.updatedAt;
        });
        break;
      }
      default: // UPDATED_AT
        rest.sort((a, b) => (asc ? a.updatedAt - b.updatedAt : b.updatedAt - a.updatedAt));
    }

    return [...pinned, ...rest];
  }

  showSortMenu() {
    if (this._sortMenuCloseHandler) {
      document.removeEventListener('click', this._sortMenuCloseHandler);
      this._sortMenuCloseHandler = null;
    }

    const existing = document.querySelector('.list-sort-menu');
    if (existing) {
      existing.remove();
      return;
    }

    const menu = document.createElement('div');
    menu.className = 'sort-menu list-sort-menu';
    const dirOptions = [
      { value: 'DESC', label: '↓ Descending' },
      { value: 'ASC', label: '↑ Ascending' },
    ];
    menu.innerHTML =
      NotesList.SORT_OPTIONS.map(
        (opt) => `
        <div class="sort-menu-item ${opt.value === this.sortOption ? 'active' : ''}" data-value="${opt.value}">
          ${opt.value === this.sortOption ? '● ' : '○ '}${opt.label}
        </div>
      `,
      ).join('') +
      '<div class="sort-menu-separator"></div>' +
      dirOptions
        .map(
          (d) => `
        <div class="sort-menu-item ${d.value === this.sortDirection ? 'active' : ''}" data-direction="${d.value}">
          ${d.value === this.sortDirection ? '● ' : '○ '}${d.label}
        </div>
      `,
        )
        .join('');

    const rect = this.sortBtn.getBoundingClientRect();
    menu.style.position = 'fixed';
    menu.style.top = `${rect.bottom + 4}px`;
    menu.style.right = `${window.innerWidth - rect.right}px`;

    document.body.appendChild(menu);

    menu.addEventListener('click', (e) => {
      const item = e.target.closest('.sort-menu-item');
      if (!item) return;
      if (item.dataset.direction) {
        this.sortDirection = item.dataset.direction;
        localStorage.setItem('noteListSortDirection', this.sortDirection);
      } else {
        this.sortOption = item.dataset.value;
        localStorage.setItem('noteListSortOption', this.sortOption);
      }
      this._updateSortBtnState();
      this.render(this.searchInput.value);
      menu.remove();
      if (this._sortMenuCloseHandler) {
        document.removeEventListener('click', this._sortMenuCloseHandler);
        this._sortMenuCloseHandler = null;
      }
    });

    const closeHandler = (e) => {
      if (!menu.contains(e.target) && e.target !== this.sortBtn) {
        menu.remove();
        document.removeEventListener('click', closeHandler);
        this._sortMenuCloseHandler = null;
      }
    };
    this._sortMenuCloseHandler = closeHandler;
    setTimeout(() => document.addEventListener('click', closeHandler), 0);
  }

  _updateSortBtnState() {
    const isDefault = this.sortOption === 'UPDATED_AT' && this.sortDirection === 'DESC';
    this.sortBtn.classList.toggle('active', !isDefault);
    const label = NotesList.SORT_OPTIONS.find((o) => o.value === this.sortOption)?.label ?? 'Last modified';
    const dirLabel = this.sortDirection === 'ASC' ? '↑' : '↓';
    this.sortBtn.title = `Sort: ${label} ${dirLabel}`;
  }

  renderNoteItem(note) {
    const previewLines = this.getPreviewLines(note);
    const date = this.formatDate(note.updatedAt);
    const isActive = note.id === this.selectedId && !this.selectionMode;
    const isSelected = this.selectedIds.has(note.id);
    const pinIcon = note.isPinned
      ? `<svg class="pin-icon" width="12" height="12" viewBox="0 0 24 24" fill="currentColor" stroke="none" aria-label="Pinned">
           <path d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24Z"/>
           <line x1="12" y1="17" x2="12" y2="22" stroke="currentColor" stroke-width="2"/>
         </svg>`
      : '';

    const colorPair = getColorPair(note.color);
    const colorStyle = colorPair ? `style="--nc-l:${colorPair.light};--nc-d:${colorPair.dark}"` : '';

    let classes = 'note-item';
    if (isActive) classes += ' selected';
    if (isSelected) classes += ' multi-selected';
    if (colorPair) classes += ' has-color';

    const typeIcon =
      note.noteType === 'CHECKLIST'
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
      <div class="${classes}" data-id="${note.id}" ${colorStyle}>
        ${
          this.selectionMode
            ? `<div class="note-item-checkbox">
            ${
              isSelected
                ? `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                   <rect x="3" y="3" width="18" height="18" rx="3" ry="3" fill="currentColor" stroke="currentColor"/>
                   <polyline points="9 12 11 14 15 10" stroke="white" stroke-width="2.5"/>
                 </svg>`
                : `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                   <rect x="3" y="3" width="18" height="18" rx="3" ry="3"/>
                 </svg>`
            }
          </div>`
            : ''
        }
        <div class="note-item-content">
          <div class="note-item-header">
            ${typeIcon}
            <div class="note-item-title">${this.escapeHtml(note.title)}</div>
          </div>
          <div class="note-item-preview">${previewLines.map((line) => `<div class="preview-line">${this.escapeHtml(line)}</div>`).join('')}</div>
          <div class="note-item-meta">${pinIcon}<span class="note-item-date" data-ts="${note.updatedAt}">${date}</span></div>
        </div>
      </div>
    `;
  }

  /**
   * Generate up to 3 preview lines for a note.
   */
  getPreviewLines(note) {
    if (note.noteType === 'CHECKLIST' && note.checklistItems) {
      return this.getChecklistPreviewLines(note.checklistItems, note.checklistSortOption);
    }

    if (!note.content || !note.content.trim()) {
      return ['Empty note'];
    }

    const lines = note.content
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line.length > 0);

    return lines.slice(0, 3).map((line) => (line.length > 120 ? `${line.substring(0, 120)}…` : line));
  }

  getChecklistPreviewLines(items, sortOption) {
    if (!items || items.length === 0) {
      return ['Empty checklist'];
    }

    const sorted = [...items];
    switch (sortOption) {
      case 'ALPHABETICAL_ASC':
        sorted.sort((a, b) => a.text.localeCompare(b.text));
        break;
      case 'ALPHABETICAL_DESC':
        sorted.sort((a, b) => b.text.localeCompare(a.text));
        break;
      case 'CHECKED_FIRST':
        sorted.sort((a, b) => {
          if (a.isChecked !== b.isChecked) return a.isChecked ? -1 : 1;
          return a.order - b.order;
        });
        break;
      default:
        sorted.sort((a, b) => {
          if (a.isChecked !== b.isChecked) return a.isChecked ? 1 : -1;
          return (a.originalOrder ?? a.order) - (b.originalOrder ?? b.order);
        });
    }

    const total = sorted.length;
    const checked = sorted.filter((item) => item.isChecked).length;

    const previewItems = sorted.slice(0, 3).map((item) => {
      const icon = item.isChecked ? '☑' : '☐';
      const text = item.text.length > 100 ? `${item.text.substring(0, 100)}…` : item.text;
      return `${icon} ${text}`;
    });

    if (total > 3) {
      previewItems.push(`${checked}/${total} completed`);
    }

    return previewItems;
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
      year: 'numeric',
    });
  }

  escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }

  clearSelection() {
    this.selectedId = null;
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
