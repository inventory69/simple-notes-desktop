import { EditorView } from '@codemirror/view';
import { EditorState } from '@codemirror/state';
import { basicSetup } from 'codemirror';
import { markdown } from '@codemirror/lang-markdown';
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import noteService from '../services/noteService.js';
import { dialogService } from '../services/DialogService.js';

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
    this.checklistContainer = document.getElementById('checklist-container');
    this.placeholderDiv = document.getElementById('no-note-selected');
    
    this.editorView = null;
    this.currentNote = null;
    this.saveTimeout = null;
    this.autosave = true;
    this.onDeleteCallback = null;
    this.showPreview = false;
    
    this.init();
  }

  init() {
    // Title input handler
    this.titleInput.addEventListener('input', () => {
      if (this.currentNote) {
        this.currentNote.title = this.titleInput.value;
        this.scheduleSave();
      }
    });
    
    // Delete button
    this.deleteBtn.addEventListener('click', () => this.handleDelete());
    
    // Preview toggle
    this.previewToggleBtn.addEventListener('click', () => this.togglePreview());
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

  renderChecklist() {
    if (!this.currentNote?.checklistItems) {
      return;
    }
    
    const items = this.currentNote.checklistItems.map((item, index) => 
      this.renderChecklistItem(item, index)
    ).join('');
    
    this.checklistContainer.innerHTML = `
      ${items}
      <button class="checklist-add-btn" id="add-checklist-item">+ Add Item</button>
    `;
    
    // Add event listeners
    this.checklistContainer.querySelectorAll('.checklist-item').forEach((el, index) => {
      const checkbox = el.querySelector('input[type="checkbox"]');
      const textInput = el.querySelector('.checklist-item-text');
      const deleteBtn = el.querySelector('.checklist-item-delete');
      
      checkbox.addEventListener('change', () => {
        this.currentNote.checklistItems[index].isChecked = checkbox.checked;
        el.classList.toggle('checked', checkbox.checked);
        this.scheduleSave();
      });
      
      textInput.addEventListener('input', () => {
        this.currentNote.checklistItems[index].text = textInput.value;
        this.scheduleSave();
      });
      
      deleteBtn.addEventListener('click', () => {
        this.currentNote.checklistItems.splice(index, 1);
        this.renderChecklist();
        this.scheduleSave();
      });
    });
    
    // Add new item button
    const addBtn = this.checklistContainer.querySelector('#add-checklist-item');
    addBtn.addEventListener('click', () => {
      this.currentNote.checklistItems.push({
        id: crypto.randomUUID(),
        text: '',
        isChecked: false,
        order: this.currentNote.checklistItems.length
      });
      this.renderChecklist();
      
      // Focus new item
      const items = this.checklistContainer.querySelectorAll('.checklist-item-text');
      items[items.length - 1]?.focus();
    });
  }

  renderChecklistItem(item, index) {
    const checked = item.isChecked ? 'checked' : '';
    return `
      <div class="checklist-item ${checked}">
        <input type="checkbox" ${checked} />
        <input 
          type="text" 
          class="checklist-item-text" 
          value="${this.escapeHtml(item.text)}" 
          placeholder="Item..."
        />
        <button class="checklist-item-delete">✕</button>
      </div>
    `;
  }

  loadNote(note) {
    // Reset previous state completely
    if (this.editorView) {
      this.editorView.destroy();
      this.editorView = null;
    }
    this.editorDiv.innerHTML = '';
    this.checklistContainer.innerHTML = '';
    
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
      this.showPreview = false;
      this.renderChecklist();
    } else {
      // Show editor (preview OFF by default)
      this.checklistContainer.classList.add('hidden');
      this.editorDiv.classList.remove('hidden');
      this.previewToggleBtn.classList.remove('hidden');
      this.initEditor();
      
      // Preview hidden by default
      this.showPreview = false;
      this.previewDiv.classList.add('hidden');
      this.editorDiv.style.width = '100%';
    }
    
    this.updateSyncStatus('Saved');
  }

  clear() {
    this.currentNote = null;
    this.container.classList.add('hidden');
    this.placeholderDiv.classList.remove('hidden');
    
    if (this.editorView) {
      this.editorView.destroy();
      this.editorView = null;
    }
  }

  scheduleSave() {
    if (!this.autosave || !this.currentNote) {
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
    
    const confirmed = await dialogService.confirm({
      title: 'Delete Note',
      message: `Do you really want to delete "${this.currentNote.title}"?`,
      confirmText: 'Delete',
      cancelText: 'Cancel',
      type: 'danger'
    });
    
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
        message: 'Failed to delete note. Please try again.'
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
