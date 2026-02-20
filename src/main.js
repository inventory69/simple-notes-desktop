import { ConnectionDialog } from './components/ConnectionDialog.js';
import { NoteEditor } from './components/NoteEditor.js';
import { NotesList } from './components/NotesList.js';
import { SettingsDialog } from './components/SettingsDialog.js';
import { dialogService } from './services/DialogService.js';
import noteService from './services/noteService.js';
import * as tauri from './services/tauri.js';

/**
 * Main Application
 */
class App {
  constructor() {
    this.connectionDialog = new ConnectionDialog();
    this.notesList = new NotesList();
    this.noteEditor = new NoteEditor();
    this.settingsDialog = new SettingsDialog();

    this.mainContainer = document.getElementById('main-container');
    this.newNoteBtn = document.getElementById('new-note-btn');
    this.newChecklistBtn = document.getElementById('new-checklist-btn');
    this.syncBtn = document.getElementById('sync-btn');
    this.selectModeBtn = document.getElementById('select-mode-btn');
    this.settingsBtn = document.getElementById('settings-btn');
    this.disconnectBtn = document.getElementById('disconnect-btn');

    // F6: Batch Actions
    this.batchActionsBar = document.getElementById('batch-actions');
    this.batchDeleteBtn = document.getElementById('batch-delete-btn');
    this.batchCancelBtn = document.getElementById('batch-cancel-btn');
    this.selectionCount = this.batchActionsBar?.querySelector('.selection-count');

    this.init();
  }

  async init() {
    // Set up event listeners
    this.setupEventListeners();

    // Detect desktop environment for theming
    await this.detectDesktopEnvironment();

    // Load theme
    const settings = await this.settingsDialog.loadAndApplyTheme();
    if (settings) {
      this.noteEditor.setAutosave(settings.autosave);
    }

    // Check for saved credentials and auto-connect
    await this.checkAutoConnect();
  }

  async detectDesktopEnvironment() {
    try {
      const desktop = await tauri.getDesktopEnvironment();
      if (desktop) {
        console.log('Detected desktop environment:', desktop);
        if (desktop.includes('kde') || desktop.includes('plasma')) {
          document.documentElement.setAttribute('data-desktop', 'kde');
        }
      }
    } catch (error) {
      console.error('Failed to detect desktop environment:', error);
    }
  }

  setupEventListeners() {
    // Connection dialog callback
    this.connectionDialog.onConnect(() => this.handleConnected());

    // Notes list selection
    this.notesList.onSelect((note) => this.noteEditor.loadNote(note));

    // F6: Notes list selection mode changes
    this.notesList.onSelectionChange((state) => {
      if (state.selectionMode) {
        this.batchActionsBar.style.display = 'flex';
        this.selectionCount.textContent = `${state.count} selected`;
        this.selectModeBtn.classList.add('active');
      } else {
        this.batchActionsBar.style.display = 'none';
        this.selectModeBtn.classList.remove('active');
      }
    });

    // Note editor delete
    this.noteEditor.onDelete(() => this.notesList.refresh());

    // Settings save callback
    this.settingsDialog.onSave((settings) => {
      this.noteEditor.setAutosave(settings.autosave);
    });

    // Toolbar buttons
    this.newNoteBtn.addEventListener('click', () => this.handleNewNote());
    this.newChecklistBtn.addEventListener('click', () => this.handleNewChecklist());
    this.syncBtn.addEventListener('click', () => this.handleSync());
    this.selectModeBtn.addEventListener('click', () => {
      if (this.notesList.selectionMode) {
        this.notesList.exitSelectionMode();
      } else {
        this.notesList.enterSelectionMode();
      }
    });
    this.settingsBtn.addEventListener('click', () => this.settingsDialog.show());
    this.disconnectBtn.addEventListener('click', () => this.handleDisconnect());

    // F6: Batch action buttons
    this.batchDeleteBtn?.addEventListener('click', () => {
      this.notesList.deleteSelected();
    });
    this.batchCancelBtn?.addEventListener('click', () => {
      this.notesList.exitSelectionMode();
    });

    // F1: Global keyboard shortcuts
    document.addEventListener('keydown', (e) => {
      // Only handle when main container is visible (connected)
      if (this.mainContainer.classList.contains('hidden')) return;

      // Don't intercept when typing in dialogs
      if (e.target.closest('.dialog:not(.hidden)')) return;

      // Ctrl+N → New Note
      if (e.ctrlKey && !e.shiftKey && e.key === 'n') {
        e.preventDefault();
        this.handleNewNote();
        return;
      }

      // Ctrl+Shift+N → New Checklist
      if (e.ctrlKey && e.shiftKey && e.key === 'N') {
        e.preventDefault();
        this.handleNewChecklist();
        return;
      }

      // Ctrl+S → Force save current note
      if (e.ctrlKey && e.key === 's') {
        e.preventDefault();
        if (this.noteEditor.currentNote) {
          this.noteEditor.save();
        }
        return;
      }

      // Ctrl+F → Focus search input
      if (e.ctrlKey && e.key === 'f') {
        e.preventDefault();
        document.getElementById('search-input')?.focus();
        return;
      }

      // Escape → Clear search / deselect
      if (e.key === 'Escape' && !e.target.closest('.dialog')) {
        const searchInput = document.getElementById('search-input');
        if (document.activeElement === searchInput && searchInput.value) {
          searchInput.value = '';
          searchInput.dispatchEvent(new Event('input'));
        }
        return;
      }
    });
  }

  async checkAutoConnect() {
    try {
      const credentials = await tauri.getCredentials();
      if (credentials) {
        // Try to connect
        const success = await tauri.connect(credentials.url, credentials.username, credentials.password);

        if (success) {
          await this.handleConnected();
          return;
        }
      }
    } catch (error) {
      console.log('Auto-connect failed:', error);
    }

    // Show connection dialog if auto-connect failed
    this.connectionDialog.show();
  }

  async handleConnected() {
    // Show main container
    this.mainContainer.classList.remove('hidden');

    // Load notes
    try {
      await noteService.loadNotes();
    } catch (error) {
      console.error('Failed to load notes:', error);
      await dialogService.error({
        title: 'Load Failed',
        message: 'Failed to load notes from server',
      });
    }
  }

  async handleNewNote() {
    try {
      const title = await dialogService.prompt({
        title: 'New Note',
        message: 'Title of the new note:',
        placeholder: 'Enter note title',
      });
      if (!title) return;

      const note = await noteService.createNote(title, 'TEXT');
      await noteService.saveNote(note);
      this.noteEditor.loadNote(note);
    } catch (error) {
      console.error('Failed to create note:', error);
      await dialogService.error({
        title: 'Creation Failed',
        message: 'Failed to create note',
      });
    }
  }

  async handleNewChecklist() {
    try {
      const title = await dialogService.prompt({
        title: 'New Checklist',
        message: 'Title of the new checklist:',
        placeholder: 'Enter checklist title',
      });
      if (!title) return;

      const note = await noteService.createNote(title, 'CHECKLIST');
      await noteService.saveNote(note);
      this.noteEditor.loadNote(note);
    } catch (error) {
      console.error('Failed to create checklist:', error);
      await dialogService.error({
        title: 'Creation Failed',
        message: 'Failed to create checklist',
      });
    }
  }

  async handleSync() {
    try {
      this.syncBtn.disabled = true;
      this.syncBtn.classList.add('spinning');

      await noteService.loadNotes();

      this.syncBtn.disabled = false;
      this.syncBtn.classList.remove('spinning');

      await dialogService.success({
        title: 'Sync Complete',
        message: 'Notes synchronized successfully',
      });
    } catch (error) {
      console.error('Sync failed:', error);
      await dialogService.error({
        title: 'Sync Failed',
        message: error.message || 'Could not synchronize notes',
      });
      this.syncBtn.disabled = false;
      this.syncBtn.classList.remove('spinning');
    }
  }

  async handleDisconnect() {
    const confirmed = await dialogService.confirm({
      title: 'Disconnect',
      message: 'Do you really want to disconnect?',
      confirmText: 'Disconnect',
      cancelText: 'Cancel',
      type: 'warning',
    });

    if (!confirmed) {
      return;
    }

    try {
      await tauri.clearCredentials();

      // Clear UI
      this.mainContainer.classList.add('hidden');
      this.noteEditor.clear();
      noteService.notes = [];
      noteService.currentNote = null;
      noteService.notify();

      // Show connection dialog
      this.connectionDialog.show();
    } catch (error) {
      console.error('Failed to disconnect:', error);
      await dialogService.error({
        title: 'Disconnect Failed',
        message: 'Error disconnecting from server',
      });
    }
  }
}

// Initialize app when DOM is ready
window.addEventListener('DOMContentLoaded', () => {
  new App();
});
