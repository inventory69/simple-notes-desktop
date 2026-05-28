import * as tauri from '../services/tauri.js';

/**
 * Update-Toast-Komponente: zeigt eine dezente Benachrichtigung unten rechts,
 * wenn beim Startup ein Update verfügbar ist.
 */
export class UpdateToast {
  constructor() {
    this.toast = document.getElementById('update-toast');
    this.textEl = document.getElementById('update-toast-text');
    this.installBtn = document.getElementById('update-toast-install');
    this.dismissBtn = document.getElementById('update-toast-dismiss');

    this.installBtn.addEventListener('click', () => this._handleInstall());
    this.dismissBtn.addEventListener('click', () => this.hide());
  }

  show(version) {
    this.textEl.textContent = `Update available: v${version}`;
    this.installBtn.disabled = false;
    this.installBtn.textContent = 'Install';
    this.toast.classList.remove('hidden');
  }

  hide() {
    this.toast.classList.add('hidden');
  }

  async _handleInstall() {
    this.installBtn.disabled = true;
    this.installBtn.textContent = 'Installing…';
    try {
      await tauri.installUpdate();
      // App wird beendet — dieser Code wird normalerweise nicht erreicht
    } catch (error) {
      this.textEl.textContent = error.message || error;
      this.installBtn.disabled = false;
      this.installBtn.textContent = 'Retry';
    }
  }
}
