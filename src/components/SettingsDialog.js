import { openUrl } from '@tauri-apps/plugin-opener';
import { dialogService } from '../services/DialogService.js';
import * as tauri from '../services/tauri.js';

/**
 * Settings Dialog Component
 */
export class SettingsDialog {
  constructor() {
    this.dialog = document.getElementById('settings-dialog');
    this.themeSelect = document.getElementById('theme-select');
    this.autosaveCheckbox = document.getElementById('autosave-checkbox');
    this.trayCheckbox = document.getElementById('tray-checkbox');
    this.autostartCheckbox = document.getElementById('autostart-checkbox');
    this.deviceIdInput = document.getElementById('device-id');
    this.saveBtn = document.getElementById('save-settings-btn');
    this.cancelBtn = document.getElementById('cancel-settings-btn');
    this.appVersionEl = document.getElementById('app-version');
    this.githubLink = document.getElementById('github-link');
    this.onSaveCallback = null;
    this.originalTheme = null; // Store original theme for cancel

    this.init();
  }

  init() {
    this.saveBtn.addEventListener('click', () => this.handleSave());
    this.cancelBtn.addEventListener('click', () => this.handleCancel());

    // Theme change handler - only preview, don't save
    this.themeSelect.addEventListener('change', () => {
      this.applyTheme(this.themeSelect.value);
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

    // Load app version on init
    this.loadAppVersion();
  }

  async loadAppVersion() {
    try {
      const version = await tauri.getAppVersion();
      this.appVersionEl.textContent = `v${version}`;
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

      this.themeSelect.value = settings.theme;
      this.autosaveCheckbox.checked = settings.autosave;
      this.trayCheckbox.checked = settings.minimize_to_tray || false;
      this.autostartCheckbox.checked = settings.autostart || false;
      this.deviceIdInput.value = deviceId;

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
    this.hide();
  }

  async handleSave() {
    try {
      const settings = {
        theme: this.themeSelect.value,
        autosave: this.autosaveCheckbox.checked,
        minimize_to_tray: this.trayCheckbox.checked,
        autostart: this.autostartCheckbox.checked,
      };

      await tauri.saveSettings(settings);
      // Update tray runtime state immediately (no restart needed)
      await tauri.updateTraySetting(settings.minimize_to_tray);
      this.applyTheme(settings.theme);

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
    const root = document.documentElement;

    if (theme === 'dark') {
      root.setAttribute('data-theme', 'dark');
    } else if (theme === 'light') {
      root.removeAttribute('data-theme');
    } else {
      // System theme
      const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
      if (prefersDark) {
        root.setAttribute('data-theme', 'dark');
      } else {
        root.removeAttribute('data-theme');
      }
    }
  }

  async loadAndApplyTheme() {
    try {
      const settings = await tauri.getSettings();
      this.applyTheme(settings.theme);
      return settings;
    } catch (error) {
      console.error('Failed to load theme:', error);
      return null;
    }
  }

  onSave(callback) {
    this.onSaveCallback = callback;
  }
}
