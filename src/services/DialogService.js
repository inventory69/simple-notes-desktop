/**
 * Universal Dialog Service
 * 
 * Replaces native browser dialogs (confirm, alert) with custom styled dialogs.
 * Provides consistent UI across all dialog types.
 * 
 * @example
 * // Confirm Dialog
 * const confirmed = await dialogService.confirm({
 *   title: 'Delete Note',
 *   message: 'Do you really want to delete "My Note"?',
 *   confirmText: 'Delete',
 *   cancelText: 'Cancel',
 *   type: 'danger'
 * });
 * 
 * // Alert Dialog
 * await dialogService.alert({
 *   title: 'Error',
 *   message: 'Failed to save settings',
 *   type: 'error'
 * });
 * 
 * // Shorthand methods
 * await dialogService.error({ message: 'Something went wrong' });
 * await dialogService.success({ message: 'Saved successfully' });
 * await dialogService.warning({ message: 'Connection lost' });
 * await dialogService.info({ message: 'Sync complete' });
 */
class DialogService {
  constructor() {
    this.dialog = null;
    this.resolvePromise = null;
    this.keydownHandler = null;
    this.init();
  }

  init() {
    // Create dialog element if not exists
    if (!document.getElementById('universal-dialog')) {
      this.createDialogElement();
    }
    
    this.dialog = document.getElementById('universal-dialog');
    this.titleEl = document.getElementById('dialog-title');
    this.messageEl = document.getElementById('dialog-message');
    this.confirmBtn = document.getElementById('dialog-confirm-btn');
    this.cancelBtn = document.getElementById('dialog-cancel-btn');
    this.iconContainer = document.getElementById('dialog-icon');
    this.actionsContainer = document.getElementById('dialog-actions');
  }

  createDialogElement() {
    const html = `
      <div id="universal-dialog" class="dialog hidden" role="dialog" aria-modal="true">
        <div class="dialog-content universal-dialog-content">
          <div class="dialog-header">
            <div id="dialog-icon" class="dialog-icon" aria-hidden="true"></div>
            <h3 id="dialog-title" class="dialog-title">Dialog</h3>
          </div>
          <p id="dialog-message" class="dialog-message"></p>
          <div id="dialog-actions" class="dialog-actions">
            <button id="dialog-cancel-btn" class="btn-secondary">Cancel</button>
            <button id="dialog-confirm-btn" class="btn-primary">OK</button>
          </div>
        </div>
      </div>
    `;
    document.body.insertAdjacentHTML('beforeend', html);
  }

  /**
   * Show confirm dialog with two buttons (Cancel/Confirm)
   * @returns {Promise<boolean>} true if confirmed, false if cancelled
   */
  confirm({ 
    title, 
    message, 
    confirmText = 'Confirm', 
    cancelText = 'Cancel', 
    type = 'info' 
  }) {
    return this._show({
      title,
      message,
      confirmText,
      cancelText,
      type,
      showCancel: true
    });
  }

  /**
   * Show alert dialog with single OK button
   * @returns {Promise<boolean>} always true
   */
  alert({ 
    title, 
    message, 
    buttonText = 'OK', 
    type = 'info' 
  }) {
    return this._show({
      title,
      message,
      confirmText: buttonText,
      type,
      showCancel: false
    });
  }

  /**
   * Shorthand for error alert
   */
  error({ title = 'Error', message }) {
    return this.alert({ title, message, type: 'error' });
  }

  /**
   * Shorthand for info alert
   */
  info({ title = 'Information', message }) {
    return this.alert({ title, message, type: 'info' });
  }

  /**
   * Shorthand for warning alert
   */
  warning({ title = 'Warning', message }) {
    return this.alert({ title, message, type: 'warning' });
  }

  /**
   * Shorthand for success alert
   */
  success({ title = 'Success', message }) {
    return this.alert({ title, message, type: 'success' });
  }

  /**
   * Show prompt dialog with text input
   * @returns {Promise<string|null>} input value if confirmed, null if cancelled
   */
  prompt({ 
    title, 
    message, 
    placeholder = '',
    defaultValue = '',
    confirmText = 'OK', 
    cancelText = 'Cancel', 
    type = 'info' 
  }) {
    return new Promise((resolve) => {
      this.resolvePromise = resolve;
      
      // Set content
      this.titleEl.textContent = title;
      this.messageEl.innerHTML = `
        ${message}
        <input type="text" id="dialog-input" class="dialog-input" 
               placeholder="${placeholder}" value="${defaultValue}">
      `;
      this.confirmBtn.textContent = confirmText;
      this.cancelBtn.textContent = cancelText;
      
      // Show cancel button
      this.cancelBtn.style.display = '';
      
      // Button styling
      this.confirmBtn.className = this._getButtonClass(type);
      
      // Icon
      this.iconContainer.innerHTML = this._getIcon(type);
      this.iconContainer.className = `dialog-icon dialog-icon-${type}`;
      
      // Show dialog
      this.dialog.classList.remove('hidden');
      
      // Focus input
      const inputEl = document.getElementById('dialog-input');
      setTimeout(() => {
        inputEl.focus();
        inputEl.select();
      }, 100);
      
      // Event handlers
      const handleConfirm = () => {
        const value = inputEl.value.trim();
        this._cleanup();
        resolve(value || null);
      };
      
      const handleCancel = () => {
        this._cleanup();
        resolve(null);
      };
      
      const handleKeydown = (e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          handleCancel();
        } else if (e.key === 'Enter' && !e.shiftKey) {
          e.preventDefault();
          handleConfirm();
        }
      };
      
      // Attach handlers
      this.confirmBtn.onclick = handleConfirm;
      this.cancelBtn.onclick = handleCancel;
      document.addEventListener('keydown', handleKeydown);
      
      // Store for cleanup
      this.keydownHandler = handleKeydown;
    });
  }

  _show({ title, message, confirmText, cancelText, type, showCancel }) {
    return new Promise((resolve) => {
      this.resolvePromise = resolve;
      
      // Set content
      this.titleEl.textContent = title;
      this.messageEl.textContent = message;
      this.confirmBtn.textContent = confirmText;
      
      // Cancel button visibility
      if (showCancel) {
        this.cancelBtn.style.display = '';
        this.cancelBtn.textContent = cancelText;
      } else {
        this.cancelBtn.style.display = 'none';
      }
      
      // Button styling based on type
      this.confirmBtn.className = this._getButtonClass(type);
      
      // Icon based on type
      this.iconContainer.innerHTML = this._getIcon(type);
      this.iconContainer.className = `dialog-icon dialog-icon-${type}`;
      
      // Show dialog
      this.dialog.classList.remove('hidden');
      
      // Focus confirm button for accessibility
      setTimeout(() => this.confirmBtn.focus(), 100);
      
      // Event handlers
      const handleConfirm = () => {
        this._cleanup();
        resolve(true);
      };
      
      const handleCancel = () => {
        this._cleanup();
        resolve(false);
      };
      
      const handleKeydown = (e) => {
        if (e.key === 'Escape' && showCancel) {
          e.preventDefault();
          handleCancel();
        } else if (e.key === 'Enter' && !e.shiftKey) {
          e.preventDefault();
          handleConfirm();
        }
      };
      
      // Attach handlers
      this.confirmBtn.onclick = handleConfirm;
      this.cancelBtn.onclick = handleCancel;
      document.addEventListener('keydown', handleKeydown);
      
      // Store for cleanup
      this.keydownHandler = handleKeydown;
    });
  }

  _cleanup() {
    this.dialog.classList.add('hidden');
    if (this.keydownHandler) {
      document.removeEventListener('keydown', this.keydownHandler);
      this.keydownHandler = null;
    }
  }

  _getButtonClass(type) {
    switch (type) {
      case 'danger':
      case 'error':
        return 'btn-danger';
      case 'warning':
        return 'btn-warning';
      case 'success':
        return 'btn-success';
      default:
        return 'btn-primary';
    }
  }

  _getIcon(type) {
    const icons = {
      info: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="10"></circle>
        <line x1="12" y1="16" x2="12" y2="12"></line>
        <line x1="12" y1="8" x2="12.01" y2="8"></line>
      </svg>`,
      success: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"></path>
        <polyline points="22 4 12 14.01 9 11.01"></polyline>
      </svg>`,
      warning: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path>
        <line x1="12" y1="9" x2="12" y2="13"></line>
        <line x1="12" y1="17" x2="12.01" y2="17"></line>
      </svg>`,
      error: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="10"></circle>
        <line x1="15" y1="9" x2="9" y2="15"></line>
        <line x1="9" y1="9" x2="15" y2="15"></line>
      </svg>`,
      danger: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <polyline points="3 6 5 6 21 6"></polyline>
        <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path>
      </svg>`
    };
    return icons[type] || icons.info;
  }
}

// Singleton export
export const dialogService = new DialogService();
