import * as tauri from '../services/tauri.js';
import noteService from '../services/noteService.js';

/**
 * Conflict Resolution Dialog Component
 * Handles sync conflicts by showing user options to resolve conflicts
 */
export class ConflictDialog {
  constructor() {
    this.dialog = document.getElementById('conflict-dialog');
    this.resolvePromise = null;
    this.currentConflictInfo = null;
    this.localNote = null;

    this.init();
  }

  init() {
    if (!this.dialog) return;

    // Button handlers
    const keepLocalBtn = document.getElementById('conflict-keep-local-btn');
    const keepRemoteBtn = document.getElementById('conflict-keep-remote-btn');
    const keepBothBtn = document.getElementById('conflict-keep-both-btn');
    const cancelBtn = document.getElementById('conflict-cancel-btn');

    if (keepLocalBtn) {
      keepLocalBtn.addEventListener('click', () => this.resolve('KEEP_LOCAL'));
    }
    if (keepRemoteBtn) {
      keepRemoteBtn.addEventListener('click', () => this.resolve('KEEP_REMOTE'));
    }
    if (keepBothBtn) {
      keepBothBtn.addEventListener('click', () => this.resolve('KEEP_BOTH'));
    }
    if (cancelBtn) {
      cancelBtn.addEventListener('click', () => this.cancel());
    }

    // Keyboard shortcuts
    document.addEventListener('keydown', (e) => {
      if (this.dialog && !this.dialog.classList.contains('hidden')) {
        if (e.key === 'Escape') {
          e.preventDefault();
          this.cancel();
        }
      }
    });
  }

  /**
   * Format timestamp for display
   * @param {number} timestamp - Unix timestamp in ms
   * @returns {string} Formatted date string
   */
  formatTimestamp(timestamp) {
    if (!timestamp) return 'Unknown';
    const date = new Date(timestamp);
    return date.toLocaleString('en-US', {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  }

  /**
   * Get preview content from note
   * @param {Object} note - Note object
   * @returns {string} Preview text
   */
  getPreviewContent(note) {
    if (note.noteType === 'CHECKLIST' && note.checklistItems) {
      const items = note.checklistItems.slice(0, 3);
      return items
        .map((item) => {
          const check = item.isChecked ? '☑' : '☐';
          const text = item.text.length > 50 ? item.text.substring(0, 50) + '…' : item.text;
          return `${check} ${text}`;
        })
        .join('\n');
    }

    if (note.content) {
      const lines = note.content.split('\n').filter((l) => l.trim());
      if (lines.length > 0) {
        const preview = lines.slice(0, 3).join('\n');
        return preview.length > 150 ? preview.substring(0, 150) + '…' : preview;
      }
    }

    return 'Empty note';
  }

  /**
   * Show conflict dialog
   * @param {Object} localNote - Local note with changes
   * @returns {Promise<Object|null>} Resolved note or null if cancelled
   */
  async show(localNote) {
    this.localNote = localNote;

    try {
      // Get conflict info from backend
      this.currentConflictInfo = await tauri.getConflictInfo(localNote.id, localNote);
    } catch (error) {
      console.error('Failed to get conflict info:', error);
      // Fallback: use local note as both versions
      this.currentConflictInfo = {
        noteId: localNote.id,
        localNote: localNote,
        remoteNote: localNote,
        localModifiedAt: localNote.updatedAt,
        remoteModifiedAt: localNote.updatedAt,
        diffSummary: 'Could not retrieve server version',
      };
    }

    this.updateDialogContent();
    this.dialog.classList.remove('hidden');

    return new Promise((resolve) => {
      this.resolvePromise = resolve;
    });
  }

  /**
   * Update dialog content with conflict info
   */
  updateDialogContent() {
    if (!this.currentConflictInfo) return;

    const { localNote, remoteNote, localModifiedAt, remoteModifiedAt, diffSummary } =
      this.currentConflictInfo;

    // Update local version
    const localTitleEl = document.getElementById('local-title');
    const localContentEl = document.getElementById('local-content-preview');
    const localTimeEl = document.getElementById('local-modified-time');

    if (localTitleEl) localTitleEl.textContent = localNote?.title || 'Untitled';
    if (localContentEl) localContentEl.textContent = this.getPreviewContent(localNote);
    if (localTimeEl) localTimeEl.textContent = `Modified: ${this.formatTimestamp(localModifiedAt)}`;

    // Update remote version
    const remoteTitleEl = document.getElementById('remote-title');
    const remoteContentEl = document.getElementById('remote-content-preview');
    const remoteTimeEl = document.getElementById('remote-modified-time');

    if (remoteTitleEl) remoteTitleEl.textContent = remoteNote?.title || 'Untitled';
    if (remoteContentEl) remoteContentEl.textContent = this.getPreviewContent(remoteNote);
    if (remoteTimeEl) remoteTimeEl.textContent = `Modified: ${this.formatTimestamp(remoteModifiedAt)}`;

    // Update diff summary
    const diffSummaryEl = document.getElementById('diff-summary');
    const conflictDiffEl = document.getElementById('conflict-diff');

    if (diffSummary && diffSummaryEl && conflictDiffEl) {
      diffSummaryEl.textContent = diffSummary;
      conflictDiffEl.classList.remove('hidden');
    }
  }

  /**
   * Resolve conflict with selected option
   * @param {string} resolution - Resolution type: "KEEP_LOCAL", "KEEP_REMOTE", "KEEP_BOTH"
   */
  async resolve(resolution) {
    if (!this.currentConflictInfo || !this.localNote) {
      this.hide();
      if (this.resolvePromise) {
        this.resolvePromise(null);
        this.resolvePromise = null;
      }
      return;
    }

    try {
      const resolvedNote = await tauri.resolveConflict(
        this.localNote.id,
        this.localNote,
        resolution
      );

      // Update local cache
      if (resolvedNote) {
        const index = noteService.notes.findIndex((n) => n.id === resolvedNote.id);
        if (index >= 0) {
          noteService.notes[index] = { ...resolvedNote };
        } else {
          noteService.notes.unshift(resolvedNote);
        }
        noteService.currentNote = resolvedNote;
        noteService.notify();
      }

      this.hide();
      if (this.resolvePromise) {
        this.resolvePromise(resolvedNote);
        this.resolvePromise = null;
      }
    } catch (error) {
      console.error('Failed to resolve conflict:', error);
      this.hide();
      if (this.resolvePromise) {
        this.resolvePromise(null);
        this.resolvePromise = null;
      }
    }
  }

  /**
   * Cancel conflict resolution
   */
  cancel() {
    this.hide();
    if (this.resolvePromise) {
      this.resolvePromise(null);
      this.resolvePromise = null;
    }
  }

  /**
   * Hide the dialog
   */
  hide() {
    this.dialog.classList.add('hidden');
    this.currentConflictInfo = null;
    this.localNote = null;
  }

  /**
   * Check if dialog is visible
   * @returns {boolean}
   */
  isVisible() {
    return !this.dialog.classList.contains('hidden');
  }
}

export default ConflictDialog;
