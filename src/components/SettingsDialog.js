import { openUrl } from '@tauri-apps/plugin-opener';
import { dialogService } from '../services/DialogService.js';
import * as tauri from '../services/tauri.js';

/**
 * Settings Dialog Component
 */
const FONT_SCALE = { system: 1, small: 0.85, normal: 1, large: 1.15, xlarge: 1.3 };

export class SettingsDialog {
  constructor() {
    this.dialog = document.getElementById('settings-dialog');
    this.themeSelect = document.getElementById('theme-select');
    this.autosaveCheckbox = document.getElementById('autosave-checkbox');
    this.trayCheckbox = document.getElementById('tray-checkbox');
    this.autostartCheckbox = document.getElementById('autostart-checkbox');
    this.deviceIdInput = document.getElementById('device-id');
    this.syncFolderInput = document.getElementById('sync-folder-input');
    this.saveBtn = document.getElementById('save-settings-btn');
    this.cancelBtn = document.getElementById('cancel-settings-btn');
    this.appVersionEl = document.getElementById('app-version');
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
    this.originalTheme = null; // Store original theme for cancel
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

    // Theme change handler - only preview, don't save
    this.themeSelect.addEventListener('change', () => {
      this.applyTheme(this.themeSelect.value);
    });

    // Font size chip handler - live preview
    this.fontSizeChips.addEventListener('click', (e) => {
      const chip = e.target.closest('.chip');
      if (!chip) return;
      const value = chip.dataset.value;
      this._setActiveChip(value);
      this.applyFontSize(value);
    });

    // Sync folder input sanitization (Android parity: only alphanumeric, dash, underscore)
    this.syncFolderInput.addEventListener('input', () => {
      const sanitized = this.syncFolderInput.value.replace(/[^a-zA-Z0-9_-]/g, '').substring(0, 50);
      if (sanitized !== this.syncFolderInput.value) {
        this.syncFolderInput.value = sanitized;
      }
    });

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
          this.updatesSection.classList.remove('hidden');
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

  async show() {
    // Load current settings
    try {
      const settings = await tauri.getSettings();
      const deviceId = await tauri.getDeviceId();

      // Store original theme for cancel
      this.originalTheme = settings.theme;
      // Store original font size for cancel
      this._originalFontSize = settings.font_size || 'system';
      // Store original sync folder for change detection
      this._previousSyncFolder = settings.sync_folder || 'notes';

      this.themeSelect.value = settings.theme;
      this.autosaveCheckbox.checked = settings.autosave;
      this.trayCheckbox.checked = settings.minimize_to_tray || false;
      this.autostartCheckbox.checked = settings.autostart || false;
      this.syncFolderInput.value = settings.sync_folder || '';
      this.updateNotificationsCheckbox.checked = settings.update_notifications !== false;
      this.defaultOpenModeSelect.value = settings.default_open_mode || 'edit';
      this.deviceIdInput.value = deviceId;
      this._setActiveChip(this._originalFontSize);

      // Update-Status korrekt anzeigen (verhindert stale Zustand aus vorheriger Session)
      this._restoreUpdateState();

      this.dialog.classList.remove('hidden');
    } catch (error) {
      console.error('Failed to load settings:', error);
    }
  }

  hide() {
    this.dialog.classList.add('hidden');
  }

  handleCancel() {
    // Restore original theme
    if (this.originalTheme) {
      this.applyTheme(this.originalTheme);
    }
    // Restore original font size
    if (this._originalFontSize) {
      this.applyFontSize(this._originalFontSize);
    }
    this.hide();
  }

  async handleSave() {
    try {
      const syncFolderValue = this.syncFolderInput.value.trim();
      const settings = {
        theme: this.themeSelect.value,
        autosave: this.autosaveCheckbox.checked,
        minimize_to_tray: this.trayCheckbox.checked,
        autostart: this.autostartCheckbox.checked,
        sync_folder: syncFolderValue || 'notes',
        update_notifications: this.updateNotificationsCheckbox.checked,
        default_open_mode: this.defaultOpenModeSelect.value,
        font_size: this._currentFontSize,
      };

      await tauri.saveSettings(settings);
      // Update tray runtime state immediately (no restart needed)
      await tauri.updateTraySetting(settings.minimize_to_tray);
      this.applyTheme(settings.theme);

      // Reconnect with new sync folder if it changed
      if (this._previousSyncFolder !== undefined && this._previousSyncFolder !== settings.sync_folder) {
        try {
          const credentials = await tauri.getCredentials();
          if (credentials) {
            await tauri.connect(credentials.url, credentials.username, credentials.password, settings.sync_folder);
            // Notify app to reload notes after reconnect
            if (this.onReconnectCallback) {
              await this.onReconnectCallback();
            }
          }
        } catch (e) {
          console.error('Failed to reconnect with new sync folder:', e);
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

  applyTheme(theme) {
    this._currentTheme = theme;
    const root = document.documentElement;

    if (theme === 'dark') {
      root.setAttribute('data-theme', 'dark');
    } else if (theme === 'light') {
      root.removeAttribute('data-theme');
    } else {
      // System theme — reuse the persistent MQL instance so the change listener stays in sync
      if (this._mql.matches) {
        root.setAttribute('data-theme', 'dark');
      } else {
        root.removeAttribute('data-theme');
      }
    }
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
      return null;
    }
  }

  onSave(callback) {
    this.onSaveCallback = callback;
  }

  onReconnect(callback) {
    this.onReconnectCallback = callback;
  }
}
