import { openUrl } from '@tauri-apps/plugin-opener';
import { dialogService } from '../services/DialogService.js';
import * as tauri from '../services/tauri.js';
import { MODE_BY_ID, THEME_IDS, THEMES } from '../utils/themes.js';

/**
 * Settings Dialog Component
 */
const FONT_SCALE = { system: 1, small: 0.85, normal: 1, large: 1.15, xlarge: 1.3 };

export class SettingsDialog {
  constructor() {
    this.dialog = document.getElementById('settings-dialog');
    this.themeGrid = document.getElementById('theme-grid');
    this.autosaveCheckbox = document.getElementById('autosave-checkbox');
    this.trayCheckbox = document.getElementById('tray-checkbox');
    this.autostartCheckbox = document.getElementById('autostart-checkbox');
    this.deviceIdInput = document.getElementById('device-id');
    this.syncFolderInput = document.getElementById('sync-folder-input');
    this.offlineCheckbox = document.getElementById('offline-mode-checkbox');
    this.serverUrlInput = document.getElementById('settings-server-url');
    this.serverUsernameInput = document.getElementById('settings-username');
    this.serverPasswordInput = document.getElementById('settings-password');
    this.connectionStatus = document.getElementById('connection-status');
    this.testConnBtn = document.getElementById('test-connection-btn');
    this.homeView = document.getElementById('settings-home');
    this.backBtn = document.getElementById('settings-back-btn');
    this.updatesCard = document.getElementById('updates-card');
    this.headerTitle = this.dialog.querySelector('.settings-dialog-header h2');
    this.saveBtn = document.getElementById('save-settings-btn');
    this.cancelBtn = document.getElementById('cancel-settings-btn');
    this.appVersionEl = document.getElementById('app-version');
    this.viewChangelogBtn = document.getElementById('view-changelog-btn');
    this.githubLink = document.getElementById('github-link');
    this.updatesSection = document.getElementById('updates-section');
    this.updateNotificationsCheckbox = document.getElementById('update-notifications-checkbox');
    this.checkUpdatesBtn = document.getElementById('check-updates-btn');
    this.updateStatus = document.getElementById('update-status');
    this.installUpdateBtn = document.getElementById('install-update-btn');
    this.defaultOpenModeSelect = document.getElementById('default-open-mode-select');
    this.fontSizeChips = document.getElementById('font-size-chips');
    this.onSaveCallback = null;
    this.onReconnectCallback = null;
    this.onViewChangelogCallback = null;
    this.originalTheme = null;
    this._originalFontSize = null;
    this._currentFontSize = 'system';
    this._currentTheme = null;
    this._platform = null;
    this._pendingUpdateVersion = null;
    this._appVersion = null;
    this._mql = window.matchMedia('(prefers-color-scheme: dark)');
    this._mqlListener = () => {
      if (this._currentTheme === 'system') {
        this.applyTheme('system');
      }
    };
    this._mql.addEventListener('change', this._mqlListener);

    this.init();
  }

  init() {
    this.saveBtn.addEventListener('click', () => this.handleSave());
    this.cancelBtn.addEventListener('click', () => this.handleCancel());

    this.backBtn.addEventListener('click', () => this._showHome());
    this.homeView.addEventListener('click', (e) => {
      const card = e.target.closest('.settings-nav-card');
      if (card) this._showSection(card.dataset.section);
    });

    this.renderThemeGrid();

    // Font size chip handler - live preview
    this.fontSizeChips.addEventListener('click', (e) => {
      const chip = e.target.closest('.chip');
      if (!chip) return;
      const value = chip.dataset.value;
      this._setActiveChip(value);
      this.applyFontSize(value);
    });

    this.testConnBtn.addEventListener('click', () => this._testConnection());

    // Offline toggle: update status label live
    this.offlineCheckbox.addEventListener('change', () => this._applyOfflineState());

    // Sync folder input sanitization (Android parity: only alphanumeric, dash, underscore)
    this.syncFolderInput.addEventListener('input', () => {
      const sanitized = this.syncFolderInput.value.replace(/[^a-zA-Z0-9_-]/g, '').substring(0, 50);
      if (sanitized !== this.syncFolderInput.value) {
        this.syncFolderInput.value = sanitized;
      }
    });

    this.viewChangelogBtn?.addEventListener('click', () => this.onViewChangelogCallback?.());

    // GitHub link handler
    this.githubLink.addEventListener('click', async (e) => {
      e.preventDefault();
      try {
        await openUrl('https://github.com/inventory69/simple-notes-desktop');
      } catch (error) {
        console.error('Failed to open link:', error);
      }
    });

    // Update button handlers
    this.checkUpdatesBtn.addEventListener('click', () => this._handleCheckUpdates());
    this.installUpdateBtn.addEventListener('click', () => this._handleInstallUpdate());

    // Plattform ermitteln — async, wird lange vor dem ersten settings-open fertig
    tauri
      .getPlatform()
      .then((platform) => {
        this._platform = platform;
        if (platform === 'windows') {
          this.updatesCard.classList.remove('hidden');
        }
      })
      .catch((err) => {
        // Update-Sektion bleibt hidden (sicherer Standardzustand)
        console.error('Failed to detect platform:', err);
      });

    // Load app version on init
    this.loadAppVersion();
  }

  async _handleCheckUpdates() {
    this.checkUpdatesBtn.disabled = true;
    this.updateStatus.textContent = 'Checking…';
    this.updateStatus.className = 'update-status';
    this.installUpdateBtn.classList.add('hidden');

    let lastError;
    for (let attempt = 0; attempt < 3; attempt++) {
      try {
        const newVersion = await tauri.checkForUpdates();
        if (newVersion) {
          this._pendingUpdateVersion = newVersion;
          this.updateStatus.textContent = `Update available: v${newVersion}`;
          this.updateStatus.className = 'update-status update-available';
          this.installUpdateBtn.classList.remove('hidden');
        } else {
          this._pendingUpdateVersion = null;
          const current = this._appVersion || this.appVersionEl.textContent || '';
          this.updateStatus.textContent = `Up to date (${current})`;
          this.updateStatus.className = 'update-status update-current';
        }
        this.checkUpdatesBtn.disabled = false;
        return;
      } catch (error) {
        lastError = error;
        if (attempt < 2) {
          this.updateStatus.textContent = `Checking… (retry ${attempt + 2}/3)`;
          await new Promise((r) => setTimeout(r, 2000));
        }
      }
    }
    this._pendingUpdateVersion = null;
    this.updateStatus.textContent = `Error: ${lastError.message || lastError}`;
    this.updateStatus.className = 'update-status update-error';
    this.checkUpdatesBtn.disabled = false;
  }

  async _handleInstallUpdate() {
    this.installUpdateBtn.disabled = true;
    this.installUpdateBtn.textContent = 'Installing…';
    this.checkUpdatesBtn.disabled = true;
    this.updateStatus.textContent = 'Downloading and installing update…';
    this.updateStatus.className = 'update-status';

    try {
      await tauri.installUpdate();
      // App wird beendet — dieser Code sollte nicht erreicht werden
    } catch (error) {
      this.updateStatus.textContent = `${error.message || error}`;
      this.updateStatus.className = 'update-status update-error';
    } finally {
      // Knöpfe immer freigeben — falls App nicht beendet wird (z. B. Update weggefallen)
      this.installUpdateBtn.disabled = false;
      this.installUpdateBtn.textContent = 'Install Update';
      this.checkUpdatesBtn.disabled = false;
    }
  }

  /** Update-Status beim (Wieder-)Öffnen des Dialogs wiederherstellen */
  _restoreUpdateState() {
    if (this._pendingUpdateVersion) {
      this.updateStatus.textContent = `Update available: v${this._pendingUpdateVersion}`;
      this.updateStatus.className = 'update-status update-available';
      this.installUpdateBtn.classList.remove('hidden');
    } else {
      this.updateStatus.textContent = '';
      this.updateStatus.className = 'update-status hidden';
      this.installUpdateBtn.classList.add('hidden');
    }
  }

  async loadAppVersion() {
    try {
      const version = await tauri.getAppVersion();
      this._appVersion = `v${version}`;
      this.appVersionEl.textContent = this._appVersion;
    } catch (error) {
      console.error('Failed to load app version:', error);
      this.appVersionEl.textContent = 'Unknown';
    }
  }

  _applyOfflineState() {
    const offline = this.offlineCheckbox.checked;
    for (const el of [
      this.serverUrlInput,
      this.serverUsernameInput,
      this.serverPasswordInput,
      this.syncFolderInput,
      this.testConnBtn,
    ]) {
      if (el) el.disabled = offline;
    }
    if (offline) {
      this.connectionStatus.textContent = 'Status: Offline';
      return;
    }
    // Online-Modus: "Online" bedeutet nur den Modus, nicht Erreichbarkeit. Die echte
    // Verbindung asynchron prüfen, damit der Status nicht "Online" lügt während der
    // Server (z.B. in einem anderen Netzwerk) gar nicht erreichbar ist.
    this.connectionStatus.textContent = 'Status: Online';
    this._refreshConnectionStatus();
  }

  async _refreshConnectionStatus() {
    try {
      const connected = await tauri.isConnected();
      if (this.offlineCheckbox.checked) return; // zwischenzeitlich auf Offline umgeschaltet
      this.connectionStatus.textContent = connected ? 'Status: Online' : 'Status: Online (not connected)';
    } catch (_e) {
      /* Status unverändert lassen */
    }
  }

  async _testConnection() {
    const url = this.serverUrlInput.value.trim();
    const username = this.serverUsernameInput.value.trim();
    const password = this.serverPasswordInput.value;
    if (!url || !username || !password) {
      await dialogService.error({ title: 'Missing details', message: 'Enter server details first.' });
      return;
    }
    const syncFolder = this.syncFolderInput.value.trim() || 'notes';
    const prevStatus = this.connectionStatus.textContent;
    this.testConnBtn.disabled = true;
    this.connectionStatus.textContent = 'Status: Testing…';
    try {
      // test_connection has no side effects (unlike connect, which stores the client
      // and uploads local notes) — so testing never silently changes the offline state.
      const ok = await tauri.testConnection(url, username, password, syncFolder);
      if (ok) {
        this.connectionStatus.textContent = 'Status: Reachable';
        await dialogService.info({ title: 'Connection OK', message: 'Server reachable.' });
      } else {
        this.connectionStatus.textContent = prevStatus;
        await dialogService.error({ title: 'Connection Failed', message: 'Could not connect to the server.' });
      }
    } catch (e) {
      this.connectionStatus.textContent = prevStatus;
      await dialogService.error({ title: 'Connection Failed', message: `Could not connect: ${e.message || e}` });
    } finally {
      this.testConnBtn.disabled = false;
    }
  }

  async show() {
    // Load current settings
    try {
      const settings = await tauri.getSettings();
      const deviceId = await tauri.getDeviceId();

      // Store original theme for cancel
      this.originalTheme = settings.theme;
      // Store original font size for cancel
      this._originalFontSize = settings.font_size || 'system';
      // Store original values for change detection
      this._previousSyncFolder = settings.sync_folder || 'notes';
      this._previousOffline = settings.offline_mode !== false;

      this.selectTheme(settings.theme);
      this.autosaveCheckbox.checked = settings.autosave;
      this.trayCheckbox.checked = settings.minimize_to_tray || false;
      this.autostartCheckbox.checked = settings.autostart || false;
      this.syncFolderInput.value = settings.sync_folder || '';
      this.updateNotificationsCheckbox.checked = settings.update_notifications !== false;
      this.defaultOpenModeSelect.value = settings.default_open_mode || 'edit';
      this.deviceIdInput.value = deviceId;
      this._setActiveChip(this._originalFontSize);
      this.offlineCheckbox.checked = this._previousOffline;

      let creds = null;
      try {
        creds = await tauri.getCredentials();
      } catch (_e) {
        /* leave empty */
      }
      this.serverUrlInput.value = creds?.url || '';
      this.serverUsernameInput.value = creds?.username || '';
      this.serverPasswordInput.value = creds?.password || '';
      // Snapshot for change detection (reconnect when online creds are edited)
      this._loadedCreds = {
        url: creds?.url || '',
        username: creds?.username || '',
        password: creds?.password || '',
      };

      this._applyOfflineState();

      // Update-Status korrekt anzeigen (verhindert stale Zustand aus vorheriger Session)
      this._restoreUpdateState();

      this.dialog.classList.remove('hidden');
      this._showHome();
    } catch (error) {
      console.error('Failed to load settings:', error);
    }
  }

  _showHome() {
    this.homeView.classList.remove('hidden');
    for (const s of this.dialog.querySelectorAll('.settings-section')) {
      s.classList.add('hidden');
    }
    this.backBtn.classList.add('hidden');
    if (this.headerTitle) this.headerTitle.textContent = 'Settings';
  }

  _showSection(id) {
    this.homeView.classList.add('hidden');
    for (const s of this.dialog.querySelectorAll('.settings-section')) {
      s.classList.toggle('hidden', s.dataset.section !== id);
    }
    this.backBtn.classList.remove('hidden');
    const section = this.dialog.querySelector(`.settings-section[data-section="${id}"]`);
    const title = section?.querySelector('h3')?.textContent;
    if (this.headerTitle && title) this.headerTitle.textContent = title;
  }

  hide() {
    this.dialog.classList.add('hidden');
  }

  handleCancel() {
    if (this.originalTheme) {
      this.selectTheme(this.originalTheme);
    }
    // Restore original font size
    if (this._originalFontSize) {
      this.applyFontSize(this._originalFontSize);
    }
    this.hide();
  }

  async handleSave() {
    try {
      const offline = this.offlineCheckbox.checked;
      const syncFolderValue = this.syncFolderInput.value.trim();
      const settings = {
        theme: this._currentTheme,
        autosave: this.autosaveCheckbox.checked,
        minimize_to_tray: this.trayCheckbox.checked,
        autostart: this.autostartCheckbox.checked,
        sync_folder: syncFolderValue || 'notes',
        update_notifications: this.updateNotificationsCheckbox.checked,
        default_open_mode: this.defaultOpenModeSelect.value,
        font_size: this._currentFontSize,
        offline_mode: offline,
      };

      await tauri.saveSettings(settings);
      // Update tray runtime state immediately (no restart needed)
      await tauri.updateTraySetting(settings.minimize_to_tray);
      this.applyTheme(settings.theme);

      const url = this.serverUrlInput.value.trim();
      const username = this.serverUsernameInput.value.trim();
      const password = this.serverPasswordInput.value;
      if (url && username && password) {
        await tauri.saveCredentials({ url, username, password });
      }

      // Reconcile connection only when something connection-relevant changed.
      const credsChanged =
        !!this._loadedCreds &&
        (url !== this._loadedCreds.url ||
          username !== this._loadedCreds.username ||
          password !== this._loadedCreds.password);
      const connChanged =
        offline !== this._previousOffline ||
        settings.sync_folder !== this._previousSyncFolder ||
        (!offline && credsChanged);
      if (connChanged) {
        try {
          if (offline) {
            await tauri.disconnect();
          } else if (url && username && password) {
            const ok = await tauri.connect(url, username, password, settings.sync_folder);
            if (!ok) {
              await dialogService.error({
                title: 'Connection Failed',
                message: 'Could not connect. Saved offline; check server details.',
              });
            }
          }
          if (this.onReconnectCallback) await this.onReconnectCallback();
        } catch (e) {
          console.error('Connection reconcile failed:', e);
        }
      }

      if (this.onSaveCallback) {
        this.onSaveCallback(settings);
      }

      this.hide();
    } catch (error) {
      console.error('Failed to save settings:', error);
      await dialogService.error({
        title: 'Settings Error',
        message: 'Failed to save settings. Please try again.',
      });
    }
  }

  renderThemeGrid() {
    this.themeGrid.innerHTML = '';
    for (const t of THEMES) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'theme-card';
      btn.dataset.themeId = t.id;
      btn.setAttribute('aria-pressed', 'false');
      const dots = t.swatch.map((c) => `<i style="background:${c}"></i>`).join('');
      btn.innerHTML = `<span class="theme-swatch">${dots}</span><span class="theme-name">${t.label}</span>`;
      btn.addEventListener('click', () => this.selectTheme(t.id));
      this.themeGrid.appendChild(btn);
    }
  }

  selectTheme(id) {
    this.applyTheme(id);
    for (const card of this.themeGrid.querySelectorAll('.theme-card')) {
      card.setAttribute('aria-pressed', String(card.dataset.themeId === id));
    }
  }

  applyTheme(theme) {
    this._currentTheme = theme;
    const root = document.documentElement;
    const kde = root.getAttribute('data-desktop') === 'kde';

    let effective = theme;
    if (theme === 'system') {
      const dark = this._mql.matches;
      effective = kde ? (dark ? 'breeze-dark' : 'breeze-light') : dark ? 'dark' : 'light';
    } else if (!THEME_IDS.has(theme)) {
      effective = 'light';
    }

    if (effective === 'light') {
      root.removeAttribute('data-theme');
    } else {
      root.setAttribute('data-theme', effective);
    }
    root.setAttribute('data-mode', MODE_BY_ID[effective] || 'light');
  }

  applyFontSize(value) {
    this._currentFontSize = value in FONT_SCALE ? value : 'system';
    document.documentElement.style.setProperty('--font-scale', FONT_SCALE[this._currentFontSize]);
  }

  _setActiveChip(value) {
    this._currentFontSize = value in FONT_SCALE ? value : 'system';
    for (const chip of this.fontSizeChips.querySelectorAll('.chip')) {
      chip.classList.toggle('active', chip.dataset.value === this._currentFontSize);
    }
  }

  async loadAndApplyTheme() {
    try {
      const settings = await tauri.getSettings();
      this.applyTheme(settings.theme);
      this.applyFontSize(settings.font_size || 'system');
      return settings;
    } catch (error) {
      console.error('Failed to load theme:', error);
      this.applyTheme('system');
      return null;
    }
  }

  onSave(callback) {
    this.onSaveCallback = callback;
  }

  onReconnect(callback) {
    this.onReconnectCallback = callback;
  }

  onViewChangelog(callback) {
    this.onViewChangelogCallback = callback;
  }
}
