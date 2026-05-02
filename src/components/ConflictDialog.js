import noteService from '../services/noteService.js';
import * as tauri from '../services/tauri.js';

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
    this.isMergeMode = false;

    this.init();
  }

  init() {
    if (!this.dialog) return;

    // Button handlers
    const keepLocalBtn = document.getElementById('conflict-keep-local-btn');
    const keepRemoteBtn = document.getElementById('conflict-keep-remote-btn');
    const keepBothBtn = document.getElementById('conflict-keep-both-btn');
    const mergeBtn = document.getElementById('conflict-merge-btn');
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
    if (mergeBtn) {
      mergeBtn.addEventListener('click', () => this.enterMergeMode());
    }
    if (cancelBtn) {
      cancelBtn.addEventListener('click', () => this.cancel());
    }

    // Merge editor buttons
    const mergeCancelBtn = document.getElementById('merge-cancel-btn');
    const mergeSaveBtn = document.getElementById('merge-save-btn');

    if (mergeCancelBtn) {
      mergeCancelBtn.addEventListener('click', () => this.exitMergeMode());
    }
    if (mergeSaveBtn) {
      mergeSaveBtn.addEventListener('click', () => this.saveMergedVersion());
    }

    // Keyboard shortcuts
    document.addEventListener('keydown', (e) => {
      if (this.dialog && !this.dialog.classList.contains('hidden')) {
        if (e.key === 'Escape') {
          e.preventDefault();
          if (this.isMergeMode) {
            this.exitMergeMode();
          } else {
            this.cancel();
          }
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
          const text = item.text.length > 50 ? `${item.text.substring(0, 50)}…` : item.text;
          return `${check} ${text}`;
        })
        .join('\n');
    }

    if (note.content) {
      const lines = note.content.split('\n').filter((l) => l.trim());
      if (lines.length > 0) {
        const preview = lines.slice(0, 3).join('\n');
        return preview.length > 150 ? `${preview.substring(0, 150)}…` : preview;
      }
    }

    return 'Empty note';
  }

  /**
   * Get full content from note for merge editor
   * @param {Object} note - Note object
   * @returns {string} Full content text
   */
  getFullContent(note) {
    if (note.noteType === 'CHECKLIST' && note.checklistItems) {
      return note.checklistItems
        .map((item) => {
          const check = item.isChecked ? '☑' : '☐';
          return `${check} ${item.text}`;
        })
        .join('\n');
    }

    return note.content || '';
  }

  /**
   * Enter merge mode - show the merge editor
   */
  enterMergeMode() {
    if (!this.currentConflictInfo) return;

    this.isMergeMode = true;

    const { localNote, remoteNote } = this.currentConflictInfo;

    const mergeEditor = document.getElementById('merge-editor');
    const mainActions = document.getElementById('conflict-main-actions');
    const conflictDetails = document.querySelector('.conflict-details');
    const conflictDiff = document.getElementById('conflict-diff');

    if (mergeEditor) mergeEditor.classList.remove('hidden');
    if (mainActions) mainActions.classList.add('hidden');
    if (conflictDetails) conflictDetails.classList.add('hidden');
    if (conflictDiff) conflictDiff.classList.add('hidden');

    const mergeLocalTitle = document.getElementById('merge-local-title');
    const mergeLocalContent = document.getElementById('merge-local-content');
    const mergeRemoteTitle = document.getElementById('merge-remote-title');
    const mergeRemoteContent = document.getElementById('merge-remote-content');

    if (mergeLocalTitle) mergeLocalTitle.textContent = localNote?.title || 'Untitled';
    if (mergeLocalContent) mergeLocalContent.textContent = this.getFullContent(localNote);
    if (mergeRemoteTitle) mergeRemoteTitle.textContent = remoteNote?.title || 'Untitled';
    if (mergeRemoteContent) mergeRemoteContent.textContent = this.getFullContent(remoteNote);

    const mergeTitleInput = document.getElementById('merge-title-input');
    const mergeContentArea = document.getElementById('merge-content-area');

    if (mergeTitleInput) {
      mergeTitleInput.value = localNote?.title || '';
    }
    if (mergeContentArea) {
      mergeContentArea.value = this.getFullContent(localNote);
    }
  }

  /**
   * Exit merge mode - hide the merge editor and show main actions
   */
  exitMergeMode() {
    this.isMergeMode = false;

    const mergeEditor = document.getElementById('merge-editor');
    const mainActions = document.getElementById('conflict-main-actions');
    const conflictDetails = document.querySelector('.conflict-details');
    const conflictDiff = document.getElementById('conflict-diff');

    if (mergeEditor) mergeEditor.classList.add('hidden');
    if (mainActions) mainActions.classList.remove('hidden');
    if (conflictDetails) conflictDetails.classList.remove('hidden');
    if (conflictDiff && this.currentConflictInfo?.diffSummary) {
      conflictDiff.classList.remove('hidden');
    }
  }

  /**
   * Save the merged version as the resolved note
   * Uses KEEP_LOCAL strategy with the merged content
   */
  async saveMergedVersion() {
    if (!this.currentConflictInfo || !this.localNote) {
      this.hide();
      if (this.resolvePromise) {
        this.resolvePromise(null);
        this.resolvePromise = null;
      }
      return;
    }

    const mergeTitleInput = document.getElementById('merge-title-input');
    const mergeContentArea = document.getElementById('merge-content-area');

    const mergedTitle = mergeTitleInput?.value || this.localNote.title;
    const mergedContent = mergeContentArea?.value || '';

    const mergedNote = {
      ...this.localNote,
      title: mergedTitle,
    };

    if (this.localNote.noteType === 'CHECKLIST') {
      mergedNote.checklistItems = this.parseChecklistContent(mergedContent);
    } else {
      mergedNote.content = mergedContent;
    }

    try {
      const resolvedNote = await tauri.resolveConflict(this.localNote.id, mergedNote, 'KEEP_LOCAL');

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
      console.error('Failed to save merged version:', error);
      this.hide();
      if (this.resolvePromise) {
        this.resolvePromise(null);
        this.resolvePromise = null;
      }
    }
  }

  /**
   * Parse checklist content text back into checklist items
   * @param {string} content - Checklist content text with ☑/☐ markers
   * @returns {Array} Checklist items array
   */
  parseChecklistContent(content) {
    if (!content.trim()) {
      return [];
    }

    const lines = content.split('\n');
    const items = [];

    lines.forEach((line, index) => {
      if (!line.trim()) return;

      let isChecked = false;
      let text = line.trim();

      if (text.startsWith('☑')) {
        isChecked = true;
        text = text.substring(1).trim();
      } else if (text.startsWith('☐')) {
        text = text.substring(1).trim();
      }

      if (text) {
        items.push({
          id: crypto.randomUUID(),
          text: text,
          isChecked: isChecked,
          order: index,
        });
      }
    });

    return items.length > 0 ? items : this.localNote?.checklistItems || [];
  }

  /**
   * Show conflict dialog
   * @param {Object} localNote - Local note with changes
   * @returns {Promise<Object|null>} Resolved note or null if cancelled
   */
  async show(localNote) {
    this.resetState();
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

    const { localNote, remoteNote, localModifiedAt, remoteModifiedAt, diffSummary } = this.currentConflictInfo;

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
      const resolvedNote = await tauri.resolveConflict(this.localNote.id, this.localNote, resolution);

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
   * Reset all state and UI to initial state
   */
  resetState() {
    this.isMergeMode = false;
    this.currentConflictInfo = null;
    this.localNote = null;

    const mergeEditor = document.getElementById('merge-editor');
    const mainActions = document.getElementById('conflict-main-actions');
    const conflictDetails = document.querySelector('.conflict-details');
    const conflictDiff = document.getElementById('conflict-diff');

    if (mergeEditor) mergeEditor.classList.add('hidden');
    if (mainActions) mainActions.classList.remove('hidden');
    if (conflictDetails) conflictDetails.classList.remove('hidden');
    if (conflictDiff) conflictDiff.classList.add('hidden');
  }

  /**
   * Hide the dialog and reset all state
   */
  hide() {
    this.resetState();
    this.dialog.classList.add('hidden');
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
