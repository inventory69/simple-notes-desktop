import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
import { NoteEditor } from './components/NoteEditor.js';
import { NotesList } from './components/NotesList.js';
import { SettingsDialog } from './components/SettingsDialog.js';
import { UpdateToast } from './components/UpdateToast.js';
import { dialogService } from './services/DialogService.js';
import noteService from './services/noteService.js';
import * as tauri from './services/tauri.js';

/**
 * Main Application
 */
class App {
  constructor() {
    this.notesList = new NotesList();
    this.noteEditor = new NoteEditor();
    this.settingsDialog = new SettingsDialog();
    this.updateToast = new UpdateToast();

    this.splash = document.getElementById('splash');
    this.mainContainer = document.getElementById('main-container');
    this.newNoteBtn = document.getElementById('new-note-btn');
    this.newChecklistBtn = document.getElementById('new-checklist-btn');
    this.syncBtn = document.getElementById('sync-btn');
    this.selectModeBtn = document.getElementById('select-mode-btn');
    this.settingsBtn = document.getElementById('settings-btn');

    // F6: Batch Actions
    this.batchActionsBar = document.getElementById('batch-actions');
    this.batchDeleteBtn = document.getElementById('batch-delete-btn');
    this.batchCancelBtn = document.getElementById('batch-cancel-btn');
    this.batchPinBtn = document.getElementById('batch-pin-btn');
    this.batchUnpinBtn = document.getElementById('batch-unpin-btn');
    this.batchColorBtn = document.getElementById('batch-color-btn');
    this.batchMoveBtn = document.getElementById('batch-move-btn');
    this.newFolderBtn = document.getElementById('new-folder-btn');
    this.trashViewBtn = document.getElementById('trash-view-btn');
    this.selectionCount = this.batchActionsBar?.querySelector('.selection-count');

    this.init();
  }

  async init() {
    // Nicht awaiten — resized/positioniert nur das Fenster, hat keine Abhängigkeit zu
    // Theme/Notes-Loading. Awaiten würde den 200ms-Delay darin unnötig vor den Splash-Screen
    // schieben.
    this.clampWindowToMonitor();

    // Set up event listeners
    this.setupEventListeners();
    this.setupSidebarResize();

    // Detect desktop environment for theming
    await this.detectDesktopEnvironment();

    // Load theme
    const settings = await this.settingsDialog.loadAndApplyTheme();
    if (settings) {
      this.noteEditor.setAutosave(settings.autosave);
      this.noteEditor.setDefaultOpenMode(settings.default_open_mode);
    }

    // 'notes-synced' zuerst registrieren (lokale IPC, quasi sofort) — läuft damit
    // garantiert vor dem Hintergrund-Connect weiter unten, der das Event irgendwann
    // emittieren kann.
    listen('notes-synced', async () => {
      noteService.firstSyncPending = false;
      const openNote = this.noteEditor.currentNote;
      await Promise.all([noteService.loadNotes(), noteService.loadFolders()]);
      if (!openNote) return;
      const updated = noteService.notes.find((n) => n.id === openNote.id);
      if (!updated) return;
      // Backend CONFLICT ist das autoritative Signal für „anderes Gerät hat editiert".
      // updatedAt-Vergleich allein liefert False-Positives: eigener Autosave bumpt den Timestamp.
      if (updated.syncStatus === 'CONFLICT') {
        await this.noteEditor.handleServerConflict();
      } else if (!this.noteEditor.isDirty() && updated.updatedAt > openNote.updatedAt) {
        // Konfliktfreies Update von einem anderen Gerät, keine lokalen ungespeicherten Edits.
        // noteService.notes enthält NoteMetadata (kein deviceId) — volles Note-Objekt holen.
        const fullNote = await noteService.getNote(updated.id);
        this.noteEditor.loadNote(fullNote);
      }
    });

    // Lokalen Online/Offline-Status prüfen und Haupt-UI SOFORT zeigen — local_store-Reads
    // sind unmittelbar, kein Netzwerk. Der eigentliche Server-Connect (Netzwerk-Roundtrip,
    // bis zu 30s Timeout) läuft dahinter im Hintergrund weiter (siehe _backgroundConnect).
    await this.checkAutoConnect();

    // Startup-Update-Check fire-and-forget (Windows-only, wenn update_notifications aktiv)
    this._startupUpdateCheck();
  }

  _startupUpdateCheck() {
    // Nicht awaiten — läuft im Hintergrund, damit init() nicht blockiert
    this._doStartupUpdateCheck().catch(() => {});
  }

  async _doStartupUpdateCheck() {
    const settings = await tauri.getSettings();
    if (settings.update_notifications === false) return;
    // Bis zu 3 Versuche mit 3s Pause — GitHub CDN / Azure manchmal instabil
    for (let attempt = 0; attempt < 3; attempt++) {
      try {
        const version = await tauri.checkForUpdates();
        if (version) this.updateToast.show(version);
        return;
      } catch (_e) {
        if (attempt < 2) await new Promise((r) => setTimeout(r, 3000));
      }
    }
  }

  setupSidebarResize() {
    const handle = document.getElementById('sidebar-resize-handle');
    const sidebar = document.querySelector('.sidebar');
    const root = document.documentElement;

    const saved = localStorage.getItem('sidebarWidth');
    if (saved) root.style.setProperty('--sidebar-width', `${saved}px`);

    let dragging = false;
    let startX = 0;
    let startWidth = 0;

    handle.addEventListener('mousedown', (e) => {
      dragging = true;
      startX = e.clientX;
      startWidth = sidebar.offsetWidth;
      handle.classList.add('is-dragging');
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
      e.preventDefault();
    });

    document.addEventListener('mousemove', (e) => {
      if (!dragging) return;
      const w = Math.max(150, Math.min(520, startWidth + e.clientX - startX));
      root.style.setProperty('--sidebar-width', `${w}px`);
    });

    document.addEventListener('mouseup', () => {
      if (!dragging) return;
      dragging = false;
      handle.classList.remove('is-dragging');
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      localStorage.setItem('sidebarWidth', sidebar.offsetWidth);
    });
  }

  async clampWindowToMonitor() {
    try {
      // Kurz warten bis das Fenster vollständig gerendert ist, damit
      // outerSize() die echte Größe inkl. Titelleiste zurückgibt.
      await new Promise((r) => setTimeout(r, 200));

      const win = getCurrentWindow();
      const monitor = await win.currentMonitor();
      if (!monitor) return;

      const scale = monitor.scaleFactor;
      const logScreenW = Math.floor(monitor.size.width / scale);
      const logScreenH = Math.floor(monitor.size.height / scale);
      if (logScreenW < 400 || logScreenH < 300) return;

      const safeW = Math.floor(logScreenW * 0.92);
      const safeH = Math.floor(logScreenH * 0.92);

      // outerSize enthält Titelleiste/Ränder — nur das passt zum Monitor-Vergleich.
      const outer = await win.outerSize();
      const inner = await win.innerSize();
      const logOuterW = Math.floor(outer.width / scale);
      const logOuterH = Math.floor(outer.height / scale);
      const logInnerW = Math.floor(inner.width / scale);
      const logInnerH = Math.floor(inner.height / scale);

      if (logOuterW <= safeW && logOuterH <= safeH) return;

      // Dekorations-Overhead (Titelleiste, Ränder) berechnen
      const decorW = Math.max(0, logOuterW - logInnerW);
      const decorH = Math.max(0, logOuterH - logInnerH);

      const newW = Math.max(600, Math.min(logInnerW, safeW - decorW));
      const newH = Math.max(500, Math.min(logInnerH, safeH - decorH));

      if (newW !== logInnerW || newH !== logInnerH) {
        await win.setSize(new LogicalSize(newW, newH));
        await win.center();
      }
    } catch (_e) {
      // Fehler beim Fenstergröße-Clamp ignorieren
    }
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
      this.noteEditor.setDefaultOpenMode(settings.default_open_mode);
    });

    // Settings reconnect callback (offline toggle or sync folder changed)
    this.settingsDialog.onReconnect(async () => {
      try {
        const s = await tauri.getSettings();
        this._setOnline(s.offline_mode === false);
      } catch (_e) {
        /* keep current state */
      }
      await this.handleConnected();
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

    // F6: Batch action buttons
    this.batchDeleteBtn?.addEventListener('click', () => {
      this.notesList.deleteSelected();
    });
    this.batchCancelBtn?.addEventListener('click', () => {
      this.notesList.exitSelectionMode();
    });
    this.batchPinBtn?.addEventListener('click', () => {
      this.notesList.pinSelected(true);
    });
    this.batchUnpinBtn?.addEventListener('click', () => {
      this.notesList.pinSelected(false);
    });
    this.batchColorBtn?.addEventListener('click', () => {
      this.notesList.colorSelected();
    });
    this.batchMoveBtn?.addEventListener('click', () => {
      this.notesList.moveSelected();
    });

    // New folder button
    this.newFolderBtn?.addEventListener('click', () => this.handleNewFolder());

    // Trash view button
    this.trashViewBtn?.addEventListener('click', async () => {
      const entering = !noteService.isTrashMode();
      noteService.setTrashMode(entering);
      if (entering) {
        await noteService.loadTrash();
      }
    });

    // Keep trash button active state in sync with trash mode (handles Escape / back button exits)
    noteService.subscribe(() => {
      this.trashViewBtn?.classList.toggle('active', noteService.isTrashMode());
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

  _setOnline(online) {
    this.online = online;
    document.documentElement.classList.toggle('is-offline', !online);
    if (this.syncBtn) this.syncBtn.style.display = online ? '' : 'none';
  }

  async checkAutoConnect() {
    let settings = null;
    try {
      settings = await tauri.getSettings();
    } catch (_e) {
      /* defaults below */
    }

    // Sync button visibility tracks the offline_mode SETTING, not whether the
    // connection currently succeeds — otherwise a momentarily-unreachable server
    // at startup hides the button even though the user is in online mode.
    const online = !!settings && settings.offline_mode === false;
    this._setOnline(online);

    // Lokale Notizen sofort laden & UI zeigen — muss NICHT auf den Server-Connect warten,
    // local_store ist die sofortige Quelle der Wahrheit (local-first seit v0.9.0).
    await this.handleConnected();

    // Online mode: best-effort connect with saved credentials, als Fire-and-Forget im
    // Hintergrund. Fehlerbehandlung bleibt wie zuvor rein informativ (console.log),
    // kein blockierender Dialog — die UI ist zu diesem Zeitpunkt bereits sichtbar.
    if (online) this._backgroundConnect(settings);
  }

  // Netzwerk-Connect + ggf. Hintergrund-Sync, entkoppelt von der UI-Anzeige (checkAutoConnect
  // wartet NICHT auf dieses Promise). Fehler werden nur geloggt, kein Dialog — ein
  // Verbindungsfehlschlag beim Start bedeutet nur, dass Sync noch nicht live ist; der
  // Sync-Button bleibt sichtbar und ein manueller/periodischer Sync kann es später erneut
  // versuchen.
  async _backgroundConnect(settings) {
    try {
      const credentials = await tauri.getCredentials();
      if (credentials) {
        await tauri.connect(credentials.url, credentials.username, credentials.password, settings.sync_folder || null);
      }
    } catch (error) {
      console.log('Auto-connect failed:', error);
    }
  }

  async handleConnected() {
    // Clear editor and selection state before loading new folder
    this.noteEditor.clear();
    this.notesList.clearSelection();
    noteService.setCurrentFolder(null);

    // Show main container
    this.splash?.classList.add('hidden');
    this.mainContainer.classList.remove('hidden');

    // Load notes and folders in parallel
    if (this.online) noteService.firstSyncPending = true;
    try {
      await Promise.all([noteService.loadNotes(), noteService.loadFolders()]);
      if (noteService.notes.length > 0) noteService.firstSyncPending = false;
    } catch (error) {
      noteService.firstSyncPending = false;
      console.error('Failed to load notes:', error);
      await dialogService.error({
        title: 'Load Failed',
        message: 'Failed to load notes from server',
      });
    }
  }

  async handleNewFolder() {
    try {
      if (noteService.getFolders().length === 0) {
        const confirmed = await dialogService.confirm({
          title: 'Compatibility Notice',
          message:
            'Notes moved into folders will not be visible in older versions of the Android app. Folder support requires Android app version 2.7.0 or later.\n\nContinue?',
        });
        if (!confirmed) return;
      }
      const result = await dialogService.promptFolderName({ title: 'New Folder', showLocalOnly: true });
      if (!result) return;
      await noteService.createFolder(result.name, null, result.localOnly);
    } catch (error) {
      console.error('Failed to create folder:', error);
      await dialogService.error({
        title: 'Creation Failed',
        message: 'Failed to create folder',
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
      // Place note in the currently viewed folder if inside one
      if (noteService.getCurrentFolder()) {
        note.folderName = noteService.getCurrentFolder();
      }
      await noteService.saveNote(note);
      this.noteEditor.loadNote(note);
      this.noteEditor.focusContent();
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
      if (noteService.getCurrentFolder()) {
        note.folderName = noteService.getCurrentFolder();
      }
      await noteService.saveNote(note);
      this.noteEditor.loadNote(note);
      this.noteEditor.focusContent();
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
      this._flashSync(null);

      // Online mode but the client was dropped (e.g. server down at startup):
      // reconnect first, so a success flash actually means the server was reached.
      if (this.online) await this._ensureConnected();

      await noteService.sync();
      this.syncBtn.disabled = false;
      this.syncBtn.classList.remove('spinning');

      // Die offene Notiz wird zentral vom globalen 'notes-synced'-Listener aktualisiert
      // (Backend emittiert das Event nach jedem Sync). Hier nichts Eigenes nötig.

      this._flashSync(true, 'Notes synchronized');
    } catch (error) {
      console.error('Sync failed:', error);
      this.syncBtn.disabled = false;
      this.syncBtn.classList.remove('spinning');
      this._flashSync(false, error.message || 'Could not synchronize notes');
    }
  }

  // Ensure a live WebDAV client before syncing (online mode). Throws on failure so
  // handleSync surfaces it via the error badge instead of a silent local-only no-op.
  async _ensureConnected() {
    if (await tauri.isConnected()) return;
    const creds = await tauri.getCredentials();
    if (!creds) throw new Error('No server credentials saved');
    let syncFolder = null;
    try {
      const s = await tauri.getSettings();
      syncFolder = s.sync_folder || null;
    } catch (_e) {
      /* use default */
    }
    const ok = await tauri.connect(creds.url, creds.username, creds.password, syncFolder);
    if (!ok) throw new Error('Could not reach server');
  }

  // Inline sync feedback on the button (badge + tooltip) instead of a modal dialog.
  _flashSync(ok, message) {
    clearTimeout(this._syncFlashTimer);
    this.syncBtn.classList.remove('sync-ok', 'sync-err');
    if (ok === null) {
      this.syncBtn.title = 'Sync';
      return;
    }
    // Restart the badge animation if the class was just removed.
    void this.syncBtn.offsetWidth;
    this.syncBtn.classList.add(ok ? 'sync-ok' : 'sync-err');
    this.syncBtn.title = message;
    this._syncFlashTimer = setTimeout(
      () => {
        this.syncBtn.classList.remove('sync-ok', 'sync-err');
        this.syncBtn.title = 'Sync';
      },
      ok ? 2500 : 6000,
    );
  }
}

// Initialize app when DOM is ready
window.addEventListener('DOMContentLoaded', () => {
  // Fenster wurde mit visible: false erstellt — jetzt zeigen, wo der Splash schon im DOM ist
  // (kein weißer Frame vor dem ersten Paint mehr).
  tauri.showMainWindow();
  new App();
});
