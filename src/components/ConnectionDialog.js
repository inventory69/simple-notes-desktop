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
    this.errorDiv = document.getElementById('connection-error');
    this.onConnectCallback = null;

    this.init();
  }

  init() {
    this.form.addEventListener('submit', async (e) => {
      e.preventDefault();
      await this.handleConnect();
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
      const success = await tauri.connect(url, username, password);

      if (success) {
        // Save credentials
        await tauri.saveCredentials({ url, username, password });

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
