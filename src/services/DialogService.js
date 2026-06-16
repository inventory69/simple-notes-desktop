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
    this._backdropClickHandler = null;
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
  confirm({ title, message, confirmText = 'Confirm', cancelText = 'Cancel', type = 'info' }) {
    return this._show({
      title,
      message,
      confirmText,
      cancelText,
      type,
      showCancel: true,
    });
  }

  /**
   * Show alert dialog with single OK button
   * @returns {Promise<boolean>} always true
   */
  alert({ title, message, buttonText = 'OK', type = 'info' }) {
    return this._show({
      title,
      message,
      confirmText: buttonText,
      type,
      showCancel: false,
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
   * F4/F6: Gemeinsamer Server-Deletion-Warnungsdialog.
   * Wird sowohl für Einzel-Delete (F4) als auch Batch-Delete (F6) verwendet.
   *
   * @param {number} noteCount - Anzahl der zu löschenden Notizen
   * @returns {Promise<boolean>} true wenn Nutzer bestätigt
   */
  confirmDeletion(noteCount = 1) {
    const title = noteCount === 1 ? 'Move to Trash' : `Move ${noteCount} Notes to Trash`;

    const message =
      noteCount === 1
        ? 'The note will be moved to trash and permanently deleted after 30 days. You can restore it from the trash view.'
        : `These ${noteCount} notes will be moved to trash and permanently deleted after 30 days. You can restore them from the trash view.`;

    return this.confirm({
      title,
      message,
      confirmText: 'Move to Trash',
      cancelText: 'Cancel',
      type: 'danger',
    });
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
    type = 'info',
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
      this._attachBackdropHandler(handleCancel);
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
      if (showCancel) this._attachBackdropHandler(handleCancel);
      document.addEventListener('keydown', handleKeydown);

      // Store for cleanup
      this.keydownHandler = handleKeydown;
    });
  }

  _attachBackdropHandler(handleCancel) {
    this._backdropClickHandler = (e) => {
      if (e.target === this.dialog) handleCancel();
    };
    this.dialog.addEventListener('click', this._backdropClickHandler);
  }

  _cleanup() {
    this.dialog.classList.add('hidden');
    if (this.keydownHandler) {
      document.removeEventListener('keydown', this.keydownHandler);
      this.keydownHandler = null;
    }
    if (this._backdropClickHandler) {
      this.dialog.removeEventListener('click', this._backdropClickHandler);
      this._backdropClickHandler = null;
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
      </svg>`,
    };
    return icons[type] || icons.info;
  }

  /**
   * Prompt for a folder name with live validation.
   * @param {{title: string, defaultValue?: string, showLocalOnly?: boolean}} opts
   * @returns {Promise<{name: string, localOnly: boolean}|null>}
   */
  promptFolderName({ title, defaultValue = '', showLocalOnly = false }) {
    return new Promise((resolve) => {
      this.resolvePromise = resolve;
      this.titleEl.textContent = title;

      const localOnlyRow = showLocalOnly
        ? `<label style="display:flex;align-items:center;gap:0.5rem;margin-top:0.75rem;cursor:pointer">
             <input type="checkbox" id="dialog-local-only">
             <span>Local only (don't sync to server)</span>
           </label>`
        : '';

      this.messageEl.innerHTML = `
        <div style="margin-bottom:0.5rem">Folder name:</div>
        <input type="text" id="dialog-input" class="dialog-input"
               placeholder="e.g. Work" maxlength="64" value="${defaultValue}">
        <div id="dialog-folder-error" style="color:var(--color-danger);font-size:0.85em;margin-top:0.25rem;min-height:1.2em"></div>
        ${localOnlyRow}
      `;
      this.confirmBtn.textContent = 'OK';
      this.cancelBtn.textContent = 'Cancel';
      this.cancelBtn.style.display = '';
      this.confirmBtn.className = this._getButtonClass('info');
      this.iconContainer.innerHTML = this._getIcon('info');
      this.iconContainer.className = 'dialog-icon dialog-icon-info';
      this.dialog.classList.remove('hidden');

      const inputEl = document.getElementById('dialog-input');
      const errorEl = document.getElementById('dialog-folder-error');

      const validate = (val) => {
        if (!val.trim()) {
          errorEl.textContent = '';
          return false;
        }
        // Basic JS validation matching the Rust rules
        const trimmed = val.trim();
        if (trimmed.length > 64) {
          errorEl.textContent = 'Name must be 64 characters or less.';
          return false;
        }
        if (trimmed === '.' || trimmed === '..') {
          errorEl.textContent = 'Invalid folder name.';
          return false;
        }
        if (/[/\\:*?"<>|]/.test(trimmed) || [...trimmed].some((c) => c.charCodeAt(0) < 0x20)) {
          errorEl.textContent = 'Name contains invalid characters.';
          return false;
        }
        errorEl.textContent = '';
        return true;
      };

      inputEl.addEventListener('input', () => validate(inputEl.value));

      setTimeout(() => {
        inputEl.focus();
        inputEl.select();
      }, 100);

      const handleConfirm = () => {
        const val = inputEl.value.trim();
        if (!validate(val)) {
          if (!val) errorEl.textContent = 'Folder name cannot be empty.';
          return;
        }
        const localOnly = document.getElementById('dialog-local-only')?.checked ?? false;
        this._cleanup();
        resolve({ name: val, localOnly });
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

      this.confirmBtn.onclick = handleConfirm;
      this.cancelBtn.onclick = handleCancel;
      this._attachBackdropHandler(handleCancel);
      document.addEventListener('keydown', handleKeydown);
      this.keydownHandler = handleKeydown;
    });
  }

  /**
   * Confirm folder deletion with optional "Keep notes" checkbox.
   * @param {{folderName: string, isNotEmpty: boolean}} opts
   * @returns {Promise<{confirmed: boolean, keepNotes: boolean}>}
   */
  confirmFolderDelete({ folderName, isNotEmpty }) {
    return new Promise((resolve) => {
      this.resolvePromise = resolve;

      this.titleEl.textContent = `Delete Folder "${folderName}"`;

      if (isNotEmpty) {
        this.messageEl.innerHTML = `
          <div style="margin-bottom:0.75rem">
            This folder contains notes. What should happen to them?
          </div>
          <label style="display:flex;align-items:center;gap:0.5rem;cursor:pointer">
            <input type="checkbox" id="dialog-keep-notes" checked>
            <span>Keep notes (move to Root)</span>
          </label>
          <div style="margin-top:0.5rem;font-size:0.85em;color:var(--color-fg);opacity:0.7">
            Uncheck to move all notes in this folder to Trash.
          </div>
        `;
      } else {
        this.messageEl.textContent = `Remove folder "${folderName}"? The folder is empty.`;
      }

      this.confirmBtn.textContent = 'Delete Folder';
      this.confirmBtn.className = this._getButtonClass('danger');
      this.cancelBtn.textContent = 'Cancel';
      this.cancelBtn.style.display = '';
      this.iconContainer.innerHTML = this._getIcon('danger');
      this.iconContainer.className = 'dialog-icon dialog-icon-danger';
      this.dialog.classList.remove('hidden');

      setTimeout(() => this.confirmBtn.focus(), 100);

      const handleConfirm = () => {
        const keepNotes = isNotEmpty ? (document.getElementById('dialog-keep-notes')?.checked ?? true) : true;
        this._cleanup();
        resolve({ confirmed: true, keepNotes });
      };

      const handleCancel = () => {
        this._cleanup();
        resolve({ confirmed: false, keepNotes: true });
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

      this.confirmBtn.onclick = handleConfirm;
      this.cancelBtn.onclick = handleCancel;
      this._attachBackdropHandler(handleCancel);
      document.addEventListener('keydown', handleKeydown);
      this.keydownHandler = handleKeydown;
    });
  }

  /**
   * Phase 3: Wahl der Server-Behandlung beim Ausschluss eines Ordners aus dem Sync.
   * Zeigt drei Optionen: "Remove from server", "Keep on server", "Cancel".
   *
   * @param {{folderName: string}} opts
   * @returns {Promise<'remove'|'keep'|null>} 'remove', 'keep', oder null (abgebrochen)
   */
  confirmFolderExclude({ folderName }) {
    return new Promise((resolve) => {
      this.titleEl.textContent = 'Exclude from Sync';
      this.messageEl.innerHTML = `
        <div>What should happen to notes in "<strong>${this._escapeHtml(folderName)}</strong>" on the server?</div>
        <div style="margin-top:0.5rem;font-size:0.85em;color:var(--color-fg);opacity:0.7">
          Notes will always remain available locally on this device.
        </div>
      `;
      this.iconContainer.innerHTML = this._getIcon('info');
      this.iconContainer.className = 'dialog-icon dialog-icon-info';

      // Aktions-Buttons temporär ersetzen (werden nach Wahl wiederhergestellt)
      const origActions = this.actionsContainer.innerHTML;
      this.actionsContainer.innerHTML = `
        <button id="fe-cancel" class="btn-secondary">Cancel</button>
        <button id="fe-keep" class="btn-primary">Keep on server</button>
        <button id="fe-remove" class="btn-danger">Remove from server</button>
      `;

      this.dialog.classList.remove('hidden');

      const done = (result) => {
        this.actionsContainer.innerHTML = origActions;
        this.confirmBtn = document.getElementById('dialog-confirm-btn');
        this.cancelBtn = document.getElementById('dialog-cancel-btn');
        this._cleanup();
        resolve(result);
      };

      document.getElementById('fe-remove').onclick = () => done('remove');
      document.getElementById('fe-keep').onclick = () => done('keep');
      document.getElementById('fe-cancel').onclick = () => done(null);

      const handleKeydown = (e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          done(null);
        }
      };

      this.keydownHandler = handleKeydown;
      this._attachBackdropHandler(() => done(null));
      document.addEventListener('keydown', handleKeydown);

      setTimeout(() => document.getElementById('fe-keep')?.focus(), 100);
    });
  }

  /**
   * Resolve a sync conflict or server-deletion for a note.
   * @param {{ isDeleted: boolean }} opts - isDeleted true = DELETED_ON_SERVER, false = CONFLICT
   * @returns {Promise<'keep_mine'|'use_server'|null>} null = dismissed
   */
  confirmConflictResolve({ isDeleted }) {
    return new Promise((resolve) => {
      this.titleEl.textContent = isDeleted ? 'Note Deleted on Server' : 'Sync Conflict';
      this.messageEl.innerHTML = isDeleted
        ? `<div>This note was deleted on the server by another device. What do you want to do?</div>`
        : `<div>This note was modified on two devices at the same time. Which version do you want to keep?</div>`;
      this.iconContainer.innerHTML = this._getIcon('warning');
      this.iconContainer.className = 'dialog-icon dialog-icon-warning';

      const origActions = this.actionsContainer.innerHTML;
      const keepLabel = isDeleted ? 'Keep local' : 'Keep mine';
      const serverLabel = isDeleted ? 'Discard' : 'Use server';
      this.actionsContainer.innerHTML = `
        <button id="cr-cancel" class="btn-secondary">Cancel</button>
        <button id="cr-server" class="btn-danger">${serverLabel}</button>
        <button id="cr-keep" class="btn-primary">${keepLabel}</button>
      `;

      this.dialog.classList.remove('hidden');

      const done = (result) => {
        this.actionsContainer.innerHTML = origActions;
        this.confirmBtn = document.getElementById('dialog-confirm-btn');
        this.cancelBtn = document.getElementById('dialog-cancel-btn');
        this._cleanup();
        resolve(result);
      };

      document.getElementById('cr-keep').onclick = () => done('keep_mine');
      document.getElementById('cr-server').onclick = () => done('use_server');
      document.getElementById('cr-cancel').onclick = () => done(null);

      const handleKeydown = (e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          done(null);
        }
      };

      this.keydownHandler = handleKeydown;
      this._attachBackdropHandler(() => done(null));
      document.addEventListener('keydown', handleKeydown);

      setTimeout(() => document.getElementById('cr-keep')?.focus(), 100);
    });
  }

  /**
   * Choose a target folder for moving notes.
   * @param {{folders: Array, currentFolder: string|null}} opts
   * @returns {Promise<string|null|undefined>} folder name, null = root, undefined = cancelled
   */
  chooseFolder({ folders, currentFolder: _currentFolder }) {
    return new Promise((resolve) => {
      this.resolvePromise = resolve;

      this.titleEl.textContent = 'Move to Folder';

      const options = [
        { value: '__root__', label: 'Root (no folder)' },
        ...folders.map((f) => ({ value: f.name, label: f.name })),
        { value: '__new__', label: '+ New folder…' },
      ];

      this.messageEl.innerHTML = `
        <div style="margin-bottom:0.5rem">Choose destination:</div>
        <select id="dialog-folder-select" class="dialog-input" style="width:100%">
          ${options.map((o) => `<option value="${this._escapeAttr(o.value)}">${this._escapeHtml(o.label)}</option>`).join('')}
        </select>
      `;
      this.confirmBtn.textContent = 'Move';
      this.confirmBtn.className = this._getButtonClass('info');
      this.cancelBtn.textContent = 'Cancel';
      this.cancelBtn.style.display = '';
      this.iconContainer.innerHTML = this._getIcon('info');
      this.iconContainer.className = 'dialog-icon dialog-icon-info';
      this.dialog.classList.remove('hidden');

      setTimeout(() => document.getElementById('dialog-folder-select')?.focus(), 100);

      const handleConfirm = async () => {
        const sel = document.getElementById('dialog-folder-select');
        const val = sel?.value;
        if (val === '__new__') {
          this._cleanup();
          // Prompt for new folder name
          const result = await this.promptFolderName({ title: 'New Folder' });
          if (!result) {
            resolve(undefined);
            return;
          }
          // Create the folder then return it as target
          try {
            const { createFolder } = await import('./tauri.js');
            await createFolder(result.name, null, result.localOnly);
          } catch (_) {
            /* ignore — noteService will reload */
          }
          resolve(result.name);
          return;
        }
        this._cleanup();
        resolve(val === '__root__' ? null : val);
      };

      const handleCancel = () => {
        this._cleanup();
        resolve(undefined);
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

      this.confirmBtn.onclick = handleConfirm;
      this.cancelBtn.onclick = handleCancel;
      this._attachBackdropHandler(handleCancel);
      document.addEventListener('keydown', handleKeydown);
      this.keydownHandler = handleKeydown;
    });
  }

  _escapeAttr(str) {
    return String(str).replace(/"/g, '&quot;');
  }

  _escapeHtml(str) {
    const d = document.createElement('div');
    d.textContent = str;
    return d.innerHTML;
  }
}

// Singleton export
export const dialogService = new DialogService();
