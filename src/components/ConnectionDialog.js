import { dialogService } from '../services/DialogService.js';
import * as tauri from '../services/tauri.js';

/**
 * Connection Dialog Component
 */
export class ConnectionDialog {
  constructor() {
    this.dialog = document.getElementById('connection-dialog');
    this.form = document.getElementById('connection-form');
    this.urlInput = document.getElementById('server-url');
    this.usernameInput = document.getElementById('username');
    this.passwordInput = document.getElementById('password');
    this.syncFolderInput = document.getElementById('connect-sync-folder');
    this.errorDiv = document.getElementById('connection-error');
    this.onConnectCallback = null;

    this.init();
  }

  init() {
    this.form.addEventListener('submit', async (e) => {
      e.preventDefault();
      await this.handleConnect();
    });

    // Sync folder input sanitization (same as SettingsDialog)
    this.syncFolderInput.addEventListener('input', () => {
      const sanitized = this.syncFolderInput.value
        .replace(/[^a-zA-Z0-9_-]/g, '')
        .substring(0, 50);
      if (sanitized !== this.syncFolderInput.value) {
        this.syncFolderInput.value = sanitized;
      }
    });

    // Load saved credentials if available
    this.loadCredentials();
  }

  async loadCredentials() {
    try {
      const credentials = await tauri.getCredentials();
      if (credentials) {
        this.urlInput.value = credentials.url;
        this.usernameInput.value = credentials.username;
        this.passwordInput.value = credentials.password;
      }
      // Load sync folder from settings
      try {
        const settings = await tauri.getSettings();
        this.syncFolderInput.value = settings.sync_folder || '';
      } catch (_e) { /* use default placeholder */ }
    } catch (error) {
      console.error('Failed to load credentials:', error);
    }
  }

  async handleConnect() {
    const url = this.urlInput.value.trim();
    const username = this.usernameInput.value.trim();
    const password = this.passwordInput.value;

    if (!url || !username || !password) {
      await dialogService.warning({
        title: 'Missing Information',
        message: 'Please fill in all fields',
      });
      return;
    }

    try {
      this.hideError();
      // Use sync folder from the input field
      const syncFolderValue = this.syncFolderInput.value.trim();
      const syncFolder = syncFolderValue || 'notes';
      const success = await tauri.connect(url, username, password, syncFolder);

      if (success) {
        // Save credentials
        await tauri.saveCredentials({ url, username, password });

        // Save sync folder to settings
        try {
          const settings = await tauri.getSettings();
          settings.sync_folder = syncFolder;
          await tauri.saveSettings(settings);
        } catch (_e) { /* non-critical */ }

        // Hide dialog
        this.hide();

        // Notify callback
        if (this.onConnectCallback) {
          this.onConnectCallback();
        }
      } else {
        await dialogService.error({
          title: 'Connection Failed',
          message: 'Connection failed. Please check your credentials.',
        });
      }
    } catch (error) {
      console.error('Connection error:', error);
      await dialogService.error({
        title: 'Connection Failed',
        message: error.message || 'Could not connect to server. Please check your credentials and try again.',
      });
    }
  }

  showError(message) {
    this.errorDiv.textContent = message;
    this.errorDiv.classList.remove('hidden');
  }

  hideError() {
    this.errorDiv.classList.add('hidden');
  }

  show() {
    this.dialog.classList.remove('hidden');
  }

  hide() {
    this.dialog.classList.add('hidden');
  }

  onConnect(callback) {
    this.onConnectCallback = callback;
  }
}
