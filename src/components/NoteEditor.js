import { EditorView } from '@codemirror/view';
import { EditorState } from '@codemirror/state';
import { undo as cmUndo, redo as cmRedo } from '@codemirror/commands';
import { basicSetup } from 'codemirror';
import { markdown } from '@codemirror/lang-markdown';
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import noteService from '../services/noteService.js';
import { dialogService } from '../services/DialogService.js';
import { UndoStack } from '../utils/UndoStack.js';

/**
 * Note Editor Component
 */
export class NoteEditor {
  constructor() {
    this.container = document.getElementById('editor-container');
    this.editorDiv = document.getElementById('editor');
    this.previewDiv = document.getElementById('preview');
    this.titleInput = document.getElementById('note-title');
    this.syncStatus = document.getElementById('sync-status');
    this.deleteBtn = document.getElementById('delete-note-btn');
    this.previewToggleBtn = document.getElementById('preview-toggle-btn');
    this.sortBtn = document.getElementById('checklist-sort-btn');
    this.checklistContainer = document.getElementById('checklist-container');
    this.placeholderDiv = document.getElementById('no-note-selected');
    
    this.undoBtn = document.getElementById('undo-btn');

    this.editorView = null;
    this.currentNote = null;
    this.saveTimeout = null;
    this.autosave = true;
    this.onDeleteCallback = null;
    this.showPreview = false;

    // Undo/Redo stack (used for checklist notes and title changes)
    this._undoStack = new UndoStack(50);
    // Debounce timer for checklist text – we don't want a snapshot per keystroke
    this._undoPushTimer = null;

    this.init();
  }

  init() {
    // Title input handler
    this.titleInput.addEventListener('input', () => {
      if (this.currentNote) {
        this.currentNote.title = this.titleInput.value;
        this._schedulePushSnapshot();
        this.scheduleSave();
      }
    });

    // F1: Enter im Title → Fokus auf Content / erstes Checklist-Item
    this.titleInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        if (this.currentNote?.noteType === 'CHECKLIST') {
          const firstItem = this.checklistContainer.querySelector('.checklist-item-text');
          firstItem?.focus();
        } else if (this.editorView) {
          this.editorView.focus();
        }
      }
    });
    
    // Delete button
    this.deleteBtn.addEventListener('click', () => this.handleDelete());
    
    // Preview toggle
    this.previewToggleBtn.addEventListener('click', () => this.togglePreview());

    // F2: Sort button with dropdown
    this.sortBtn.addEventListener('click', () => this.showSortMenu());

    // Undo button
    this.undoBtn.addEventListener('click', () => this._handleUndo());

    // Ctrl+Z / Ctrl+Shift+Z keyboard shortcut
    document.addEventListener('keydown', (e) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      if (e.key === 'z' && !e.shiftKey) {
        // Only intercept when a note is loaded
        if (!this.currentNote) return;
        if (this.currentNote.noteType === 'CHECKLIST') {
          e.preventDefault();
          this._handleUndo();
        }
        // For text notes: let CodeMirror handle it natively (its own Ctrl+Z)
      }
    });
  }

  initEditor() {
    if (this.editorView) {
      this.editorView.destroy();
    }
    
    const startState = EditorState.create({
      doc: this.currentNote?.content || '',
      extensions: [
        basicSetup,
        markdown(),
        EditorView.lineWrapping,
        EditorView.updateListener.of((update) => {
          if (update.docChanged && this.currentNote) {
            this.currentNote.content = update.state.doc.toString();
            this.scheduleSave();
            if (this.showPreview) {
              this.updatePreview();
            }
          }
          // Keep undo-button state in sync with CodeMirror's own history
          if (update.docChanged || update.selectionSet) {
            this._updateUndoButton();
          }
        })
      ]
    });
    
    this.editorView = new EditorView({
      state: startState,
      parent: this.editorDiv
    });
  }

  togglePreview() {
    this.showPreview = !this.showPreview;
    
    if (this.showPreview) {
      this.previewDiv.classList.remove('hidden');
      this.previewToggleBtn.classList.add('active');
      this.updatePreview();
    } else {
      this.previewDiv.classList.add('hidden');
      this.previewToggleBtn.classList.remove('active');
    }
  }

  updatePreview() {
    if (!this.currentNote?.content) {
      this.previewDiv.innerHTML = '<div style="padding: 1rem; color: #999;">No content to preview</div>';
      return;
    }
    
    const html = marked.parse(this.currentNote.content);
    this.previewDiv.innerHTML = DOMPurify.sanitize(html);
  }

  // F2: Render checklist with sorting and separator
  renderChecklist() {
    if (!this.currentNote?.checklistItems) {
      return;
    }

    const sortOption = this.currentNote.checklistSortOption || 'UNCHECKED_FIRST';
    const items = this.sortChecklistItems(this.currentNote.checklistItems, sortOption);

    // F2: Separator-Position berechnen
    let separatorIndex = -1;
    if (sortOption === 'UNCHECKED_FIRST' || sortOption === 'MANUAL') {
      const firstCheckedIndex = items.findIndex(item => item.isChecked);
      if (firstCheckedIndex > 0) {
        separatorIndex = firstCheckedIndex;
      }
    } else if (sortOption === 'CHECKED_FIRST') {
      const firstUncheckedIndex = items.findIndex(item => !item.isChecked);
      if (firstUncheckedIndex > 0) {
        separatorIndex = firstUncheckedIndex;
      }
    }

    const checkedCount = items.filter(item => item.isChecked).length;

    // Render items with separator
    let html = '';
    items.forEach((item, index) => {
      if (index === separatorIndex) {
        html += this.renderSeparator(checkedCount);
      }
      html += this.renderChecklistItem(item, index);
    });

    // Separator at end if all unchecked and sort is UNCHECKED_FIRST
    if (separatorIndex === -1 && checkedCount === 0 &&
        (sortOption === 'UNCHECKED_FIRST' || sortOption === 'MANUAL')) {
      html += this.renderSeparator(0);
    }

    html += `<button class="checklist-add-btn" id="add-checklist-item">+ Add Item</button>`;

    this.checklistContainer.innerHTML = html;

    // F2: Attach event listeners
    this.attachChecklistListeners(items);
  }

  // F2: Sort checklist items according to sort option
  sortChecklistItems(items, sortOption) {
    const sorted = [...items];

    switch (sortOption) {
      case 'ALPHABETICAL_ASC':
        sorted.sort((a, b) => a.text.localeCompare(b.text));
        break;
      case 'ALPHABETICAL_DESC':
        sorted.sort((a, b) => b.text.localeCompare(a.text));
        break;
      case 'UNCHECKED_FIRST':
        sorted.sort((a, b) => {
          if (a.isChecked !== b.isChecked) return a.isChecked ? 1 : -1;
          return a.order - b.order;
        });
        break;
      case 'CHECKED_FIRST':
        sorted.sort((a, b) => {
          if (a.isChecked !== b.isChecked) return a.isChecked ? -1 : 1;
          return a.order - b.order;
        });
        break;
      case 'MANUAL':
      default:
        sorted.sort((a, b) => a.order - b.order);
        break;
    }

    return sorted;
  }

  // F2: Render separator between checked/unchecked groups
  renderSeparator(checkedCount) {
    const label = checkedCount === 1
      ? '1 completed'
      : `${checkedCount} completed`;
    return `
      <div class="checklist-separator" data-separator="true">
        <div class="checklist-separator-line"></div>
        <span class="checklist-separator-text">${label}</span>
        <div class="checklist-separator-line"></div>
      </div>
    `;
  }

  // F2: Attach all event listeners for checklist items
  attachChecklistListeners(sortedItems) {
    const itemElements = this.checklistContainer.querySelectorAll('.checklist-item');

    itemElements.forEach((el, displayIndex) => {
      const item = sortedItems[displayIndex];
      if (!item) return;

      // Find the actual index in the original array
      const originalIndex = this.currentNote.checklistItems.findIndex(i => i.id === item.id);
      if (originalIndex === -1) return;

      const checkbox = el.querySelector('input[type="checkbox"]');
      const textInput = el.querySelector('.checklist-item-text');
      const deleteBtn = el.querySelector('.checklist-item-delete');
      const dragHandle = el.querySelector('.checklist-drag-handle');

      // Checkbox toggle
      checkbox.addEventListener('change', () => {
        this._pushSnapshot(); // immediate snapshot before state changes
        this.currentNote.checklistItems[originalIndex].isChecked = checkbox.checked;
        this._pushSnapshot(); // snapshot after
        this.scheduleSave();
        // Re-render to update sort order and separator
        this.renderChecklist();
      });

      // Text input
      textInput.addEventListener('input', () => {
        this.currentNote.checklistItems[originalIndex].text = textInput.value;
        this._schedulePushSnapshot();
        this.scheduleSave();
        // F3: Visuellen Leer-Status aktualisieren
        const itemEl = textInput.closest('.checklist-item');
        if (textInput.value.trim() === '') {
          itemEl.classList.add('checklist-item-empty');
          if (!itemEl.querySelector('.checklist-item-unsaved-hint')) {
            const hint = document.createElement('span');
            hint.className = 'checklist-item-unsaved-hint';
            hint.textContent = 'not saved';
            itemEl.appendChild(hint);
          }
        } else {
          itemEl.classList.remove('checklist-item-empty');
          const hint = itemEl.querySelector('.checklist-item-unsaved-hint');
          if (hint) hint.remove();
        }
      });

      // F3: Bei Fokusverlust leere Einträge entfernen (mit kurzer Verzögerung für UX)
      textInput.addEventListener('blur', () => {
        if (textInput.value.trim() === '') {
          setTimeout(() => {
            // Prüfe ob der Eintrag immer noch leer ist (Nutzer könnte zurückgekehrt sein)
            if (this.currentNote.checklistItems[originalIndex]?.text.trim() === '') {
              this._pushSnapshot();
              this.currentNote.checklistItems.splice(originalIndex, 1);
              // Order-Felder neu berechnen
              this.currentNote.checklistItems.forEach((item, i) => {
                item.order = i;
              });
              this._pushSnapshot();
              this.renderChecklist();
              this.scheduleSave();
            }
          }, 200);
        }
      });

      // F1: Enter → neues Item unterhalb erstellen
      textInput.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          const newItem = {
            id: crypto.randomUUID(),
            text: '',
            isChecked: false,
            order: originalIndex + 1
          };
          // Shift order of all subsequent items
          this.currentNote.checklistItems.forEach(item => {
            if (item.order > originalIndex) {
              item.order++;
            }
          });
          this.currentNote.checklistItems.splice(originalIndex + 1, 0, newItem);
          this._pushSnapshot();
          this.renderChecklist();
          this.scheduleSave();

          // Focus new item
          const items = this.checklistContainer.querySelectorAll('.checklist-item-text');
          items[displayIndex + 1]?.focus();
        }

        // F1: Backspace auf leerem Item → Item löschen, Fokus auf vorheriges
        if (e.key === 'Backspace' && textInput.value === '' && this.currentNote.checklistItems.length > 1) {
          e.preventDefault();
          this._pushSnapshot();
          this.currentNote.checklistItems.splice(originalIndex, 1);
          // Recalculate order
          this.currentNote.checklistItems.forEach((item, i) => {
            item.order = i;
          });
          this._pushSnapshot();
          this.renderChecklist();
          this.scheduleSave();

          // Focus previous item (or first if we deleted the first)
          const focusIndex = Math.max(0, displayIndex - 1);
          const items = this.checklistContainer.querySelectorAll('.checklist-item-text');
          items[focusIndex]?.focus();
        }
      });

      // Delete button
      deleteBtn.addEventListener('click', () => {
        this._pushSnapshot();
        this.currentNote.checklistItems.splice(originalIndex, 1);
        // Recalculate order
        this.currentNote.checklistItems.forEach((item, i) => {
          item.order = i;
        });
        this._pushSnapshot();
        this.renderChecklist();
        this.scheduleSave();
      });

      // F2: Drag-and-Drop via drag handle
      if (dragHandle) {
        this.setupDragAndDrop(dragHandle, el, displayIndex, sortedItems);
      }
    });

    // Add new item button
    const addBtn = this.checklistContainer.querySelector('#add-checklist-item');
    if (addBtn) {
      addBtn.addEventListener('click', () => {
        const newItem = {
          id: crypto.randomUUID(),
          text: '',
          isChecked: false,
          order: this.currentNote.checklistItems.length
        };
        this.currentNote.checklistItems.push(newItem);
        this._pushSnapshot();
        this.renderChecklist();

        // Focus new item
        const items = this.checklistContainer.querySelectorAll('.checklist-item-text');
        items[items.length - 1]?.focus();
      });
    }
  }

  // F2: Drag-and-Drop setup for a checklist item
  setupDragAndDrop(handle, itemElement, displayIndex, sortedItems) {
    handle.addEventListener('mousedown', (e) => {
      e.preventDefault();
      const dragItem = itemElement;
      const container = this.checklistContainer;
      const allItems = Array.from(container.querySelectorAll('.checklist-item'));
      const dragIndex = allItems.indexOf(dragItem);

      dragItem.classList.add('dragging');
      let currentIndex = dragIndex;

      const startY = e.clientY;
      const itemHeight = dragItem.getBoundingClientRect().height;
      // Cache initial item center positions for separator-aware calculation
      const itemCenters = allItems.map(el => {
        const rect = el.getBoundingClientRect();
        return rect.top + rect.height / 2;
      });

      // Track source item's checked state for visual cross-boundary feedback
      const sourceItem = sortedItems[displayIndex];
      const sourceChecked = sourceItem ? sourceItem.isChecked : false;
      let visuallyToggled = false;

      const onMouseMove = (moveEvent) => {
        const deltaY = moveEvent.clientY - startY;
        dragItem.style.transform = `translateY(${deltaY}px)`;
        dragItem.style.zIndex = '100';

        // Calculate target based on actual item center positions (separator-aware)
        const dragCenter = itemCenters[dragIndex] + deltaY;
        let clampedTarget = dragIndex;
        let minDist = Infinity;
        for (let i = 0; i < itemCenters.length; i++) {
          const dist = Math.abs(dragCenter - itemCenters[i]);
          if (dist < minDist) {
            minDist = dist;
            clampedTarget = i;
          }
        }
        clampedTarget = Math.max(0, Math.min(allItems.length - 1, clampedTarget));

        // Visual cross-boundary feedback: toggle dragged item appearance
        // when it hovers over an item with a different checked state
        const targetItem = sortedItems[clampedTarget];
        const wouldToggle = targetItem && (targetItem.isChecked !== sourceChecked);
        if (wouldToggle !== visuallyToggled) {
          visuallyToggled = wouldToggle;
          const checkbox = dragItem.querySelector('input[type="checkbox"]');
          const textEl = dragItem.querySelector('.checklist-item-text');
          if (wouldToggle) {
            // Show what it would look like after drop
            dragItem.classList.toggle('checked', !sourceChecked);
            if (checkbox) checkbox.checked = !sourceChecked;
            if (textEl) textEl.style.opacity = !sourceChecked ? '0.5' : '1';
          } else {
            // Restore original visual state
            dragItem.classList.toggle('checked', sourceChecked);
            if (checkbox) checkbox.checked = sourceChecked;
            if (textEl) textEl.style.opacity = sourceChecked ? '0.5' : '1';
          }
        }

        if (clampedTarget !== currentIndex) {
          // Visual feedback: shift items AND separator together.
          // Build a visual slot list (items + separator) in DOM order so the
          // separator moves with the items that surround it.
          const allVisual = Array.from(
            container.querySelectorAll('.checklist-item, .checklist-separator[data-separator]')
          );
          const dragVisualIdx = allVisual.indexOf(dragItem);
          const targetVisualIdx = allVisual.indexOf(allItems[clampedTarget]);

          allVisual.forEach((el) => {
            if (el === dragItem) return;
            const vi = allVisual.indexOf(el);
            const lo = Math.min(dragVisualIdx, targetVisualIdx);
            const hi = Math.max(dragVisualIdx, targetVisualIdx);
            if (vi >= lo && vi <= hi) {
              const shift = clampedTarget > dragIndex ? -itemHeight : itemHeight;
              el.style.transform = `translateY(${shift}px)`;
            } else {
              el.style.transform = '';
            }
          });
          currentIndex = clampedTarget;
        }
      };

      const onMouseUp = () => {
        document.removeEventListener('mousemove', onMouseMove);
        document.removeEventListener('mouseup', onMouseUp);

        // Reset visual state (items + separator)
        container.querySelectorAll('.checklist-item, .checklist-separator[data-separator]')
          .forEach(el => {
            el.style.transform = '';
            el.style.zIndex = '';
          });
        dragItem.classList.remove('dragging');

        if (currentIndex !== dragIndex) {
          // Perform the move in the data model
          const sourceItem = sortedItems[dragIndex];
          const targetItem = sortedItems[currentIndex];

          if (sourceItem && targetItem) {
            const sourceOrigIdx = this.currentNote.checklistItems.findIndex(i => i.id === sourceItem.id);
            const targetOrigIdx = this.currentNote.checklistItems.findIndex(i => i.id === targetItem.id);

            if (sourceOrigIdx !== -1 && targetOrigIdx !== -1) {
              // Remove from source and insert at target
              const [moved] = this.currentNote.checklistItems.splice(sourceOrigIdx, 1);

              // Auto-toggle: if dragged across the checked/unchecked boundary,
              // toggle the isChecked state (matching Android app behavior)
              if (sourceItem.isChecked !== targetItem.isChecked) {
                moved.isChecked = targetItem.isChecked;
              }

              const insertIdx = this.currentNote.checklistItems.findIndex(i => i.id === targetItem.id);
              this.currentNote.checklistItems.splice(
                currentIndex > dragIndex ? insertIdx + 1 : insertIdx,
                0,
                moved
              );

              // Recalculate order
              this.currentNote.checklistItems.forEach((item, i) => {
                item.order = i;
              });

              // Switch to MANUAL sort if re-ordering manually
              this.currentNote.checklistSortOption = 'MANUAL';
            }
          }

          this._pushSnapshot();
          this.renderChecklist();
          this.scheduleSave();
        }
      };

      document.addEventListener('mousemove', onMouseMove);
      document.addEventListener('mouseup', onMouseUp);
    });
  }

  // F2: Render checklist item with drag handle
  renderChecklistItem(item, index) {
    const checked = item.isChecked ? 'checked' : '';
    const isEmpty = item.text.trim() === '';
    const emptyClass = isEmpty ? 'checklist-item-empty' : '';
    return `
      <div class="checklist-item ${checked} ${emptyClass}" data-item-id="${item.id}">
        <span class="checklist-drag-handle" title="Drag to reorder">⋮</span>
        <input type="checkbox" ${checked} />
        <input
          type="text"
          class="checklist-item-text"
          value="${this.escapeHtml(item.text)}"
          placeholder="Item..."
        />
        <button class="checklist-item-delete">✕</button>
        ${isEmpty ? '<span class="checklist-item-unsaved-hint">not saved</span>' : ''}
      </div>
    `;
  }

  // F2: Show sort option menu
  showSortMenu() {
    const existing = document.querySelector('.sort-menu');
    if (existing) {
      existing.remove();
      return;
    }

    const currentSort = this.currentNote?.checklistSortOption || 'UNCHECKED_FIRST';
    const options = [
      { value: 'MANUAL', label: 'Manual (Drag & Drop)' },
      { value: 'UNCHECKED_FIRST', label: 'Unchecked First' },
      { value: 'CHECKED_FIRST', label: 'Checked First' },
      { value: 'ALPHABETICAL_ASC', label: 'A → Z' },
      { value: 'ALPHABETICAL_DESC', label: 'Z → A' },
    ];

    const menu = document.createElement('div');
    menu.className = 'sort-menu';
    menu.innerHTML = options.map(opt => `
      <div class="sort-menu-item ${opt.value === currentSort ? 'active' : ''}" data-value="${opt.value}">
        ${opt.value === currentSort ? '● ' : '○ '}${opt.label}
      </div>
    `).join('');

    // Position below sort button
    const rect = this.sortBtn.getBoundingClientRect();
    menu.style.position = 'fixed';
    menu.style.top = `${rect.bottom + 4}px`;
    menu.style.right = `${window.innerWidth - rect.right}px`;

    document.body.appendChild(menu);

    // Click handler
    menu.addEventListener('click', (e) => {
      const item = e.target.closest('.sort-menu-item');
      if (!item) return;

      const value = item.dataset.value;
      this.currentNote.checklistSortOption = value;
      this.renderChecklist();
      this.scheduleSave();
      menu.remove();
    });

    // Close on outside click
    const closeHandler = (e) => {
      if (!menu.contains(e.target) && e.target !== this.sortBtn) {
        menu.remove();
        document.removeEventListener('click', closeHandler);
      }
    };
    setTimeout(() => document.addEventListener('click', closeHandler), 0);
  }

  loadNote(note) {
    // Reset previous state completely
    if (this.editorView) {
      this.editorView.destroy();
      this.editorView = null;
    }
    this.editorDiv.innerHTML = '';
    this.checklistContainer.innerHTML = '';

    // Reset undo stack for the new note
    this._undoStack.clear();
    if (this._undoPushTimer) {
      clearTimeout(this._undoPushTimer);
      this._undoPushTimer = null;
    }

    this.currentNote = { ...note };
    this.titleInput.value = note.title;
    
    // Show container, hide placeholder
    this.container.classList.remove('hidden');
    this.placeholderDiv.classList.add('hidden');
    
    if (note.noteType === 'CHECKLIST') {
      // Show checklist, hide editor
      this.editorDiv.classList.add('hidden');
      this.previewDiv.classList.add('hidden');
      this.previewToggleBtn.classList.add('hidden');
      this.checklistContainer.classList.remove('hidden');
      this.sortBtn.classList.remove('hidden');  // F2: Show sort button
      this.showPreview = false;
      this.renderChecklist();
      // Push initial snapshot so the first undo restores the loaded state
      this._pushSnapshot();
    } else {
      // Show editor (preview OFF by default)
      this.checklistContainer.classList.add('hidden');
      this.editorDiv.classList.remove('hidden');
      this.previewToggleBtn.classList.remove('hidden');
      this.sortBtn.classList.add('hidden');  // F2: Hide sort button for text notes
      this.initEditor();

      // Preview hidden by default
      this.showPreview = false;
      this.previewDiv.classList.add('hidden');
      this.editorDiv.style.width = '100%';
    }

    this._updateUndoButton();
    this.updateSyncStatus('Saved');
  }

  clear() {
    this.currentNote = null;
    this._undoStack.clear();
    if (this._undoPushTimer) {
      clearTimeout(this._undoPushTimer);
      this._undoPushTimer = null;
    }
    this.container.classList.add('hidden');
    this.placeholderDiv.classList.remove('hidden');
    if (this.undoBtn) this.undoBtn.disabled = true;

    if (this.editorView) {
      this.editorView.destroy();
      this.editorView = null;
    }
  }

  // ── Undo helpers ────────────────────────────────────────────────────────────

  /** Snapshot the current note state immediately. */
  _pushSnapshot() {
    if (!this.currentNote) return;
    this._undoStack.push({
      title: this.currentNote.title,
      checklistItems: this.currentNote.checklistItems,
    });
    this._updateUndoButton();
  }

  /**
   * Debounced snapshot push – used for rapid keystrokes so we don't bloat the
   * stack with one entry per character.  Flushes after 600 ms of inactivity.
   */
  _schedulePushSnapshot() {
    if (this._undoPushTimer) clearTimeout(this._undoPushTimer);
    this._undoPushTimer = setTimeout(() => {
      this._undoPushTimer = null;
      this._pushSnapshot();
    }, 600);
  }

  /** Perform one undo step. */
  _handleUndo() {
    if (!this.currentNote) return;

    // Text notes: delegate entirely to CodeMirror's built-in undo
    if (this.currentNote.noteType !== 'CHECKLIST') {
      if (this.editorView) {
        cmUndo(this.editorView);
      }
      return;
    }

    // Flush any pending debounced snapshot first so we have a current entry
    if (this._undoPushTimer) {
      clearTimeout(this._undoPushTimer);
      this._undoPushTimer = null;
      this._pushSnapshot();
    }

    const snapshot = this._undoStack.undo();
    if (!snapshot) return;

    // Restore title
    this.currentNote.title = snapshot.title;
    this.titleInput.value = snapshot.title;

    // Restore checklist items (deep replace)
    if (snapshot.checklistItems !== null) {
      this.currentNote.checklistItems = snapshot.checklistItems.map(i => ({ ...i }));
    }

    this.renderChecklist();
    this.scheduleSave();
    this._updateUndoButton();
  }

  /** Enable/disable the undo button based on stack state + editor type. */
  _updateUndoButton() {
    if (!this.undoBtn) return;
    if (!this.currentNote) {
      this.undoBtn.disabled = true;
      return;
    }
    if (this.currentNote.noteType === 'CHECKLIST') {
      this.undoBtn.disabled = !this._undoStack.canUndo;
    } else {
      // For text notes we always leave it enabled once a note is loaded
      // (CodeMirror manages its own history; querying it is not trivial)
      this.undoBtn.disabled = false;
    }
  }

  // ── /Undo helpers ───────────────────────────────────────────────────────────

  scheduleSave() {
    if (!this.autosave || !this.currentNote) {
      return;
    }

    // F3: Checklist-Schutz – nicht speichern wenn ein Eintrag gerade leer ist
    if (this.currentNote.noteType === 'CHECKLIST' && this.hasEmptyChecklistItems()) {
      this.updateSyncStatus('Unsaved (empty item)');
      return;
    }

    this.updateSyncStatus('Saving...');

    if (this.saveTimeout) {
      clearTimeout(this.saveTimeout);
    }

    this.saveTimeout = setTimeout(() => {
      this.save();
    }, 1000);
  }

  // F3: Check if any checklist items are empty
  hasEmptyChecklistItems() {
    if (!this.currentNote?.checklistItems) {
      return false;
    }
    return this.currentNote.checklistItems.some(item => item.text.trim() === '');
  }

  async save() {
    if (!this.currentNote) {
      return;
    }
    
    try {
      // Get updated note with new timestamp from backend
      const updatedNote = await noteService.saveNote(this.currentNote);
      
      // Update local reference with server response
      this.currentNote = { ...updatedNote };
      
      this.updateSyncStatus('Saved');
    } catch (error) {
      console.error('Failed to save note:', error);
      this.updateSyncStatus('Save error');
    }
  }

  async handleDelete() {
    if (!this.currentNote) {
      return;
    }

    // F4: Server-Deletion-Warnungsdialog (gemeinsame Komponente mit F6)
    const confirmed = await dialogService.confirmDeletion(1);

    if (!confirmed) {
      return;
    }

    try {
      await noteService.deleteNote(this.currentNote.id);
      this.clear();

      if (this.onDeleteCallback) {
        this.onDeleteCallback();
      }
    } catch (error) {
      console.error('Failed to delete note:', error);
      await dialogService.error({
        title: 'Delete Failed',
        message: 'Failed to delete note from server. Please check your connection and try again.'
      });
    }
  }

  updateSyncStatus(text) {
    this.syncStatus.textContent = text;
  }

  setAutosave(enabled) {
    this.autosave = enabled;
  }

  onDelete(callback) {
    this.onDeleteCallback = callback;
  }

  escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }
}
