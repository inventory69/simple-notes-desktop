import { undo as cmUndo } from '@codemirror/commands';
import { markdown } from '@codemirror/lang-markdown';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { writeHtml, writeText } from '@tauri-apps/plugin-clipboard-manager';
import { open } from '@tauri-apps/plugin-dialog';
import { basicSetup } from 'codemirror';
import DOMPurify from 'dompurify';
import { Marked } from 'marked';
import { dialogService } from '../services/DialogService.js';
import noteService from '../services/noteService.js';
import { assetUrl, attachImage, exportToCalendar, getAssetDataUrl } from '../services/tauri.js';
import { colorPicker } from '../utils/ColorPicker.js';
import { buildCalendarPayload, buildCalendarPayloadForItem } from '../utils/calendarExport.js';
import { computeImageRewrite, parseImageAlt } from '../utils/imageAltTokens.js';
import { markdownHighlightExtensions } from '../utils/markdownHighlight.js';
import {
  applyBold,
  applyChecklist,
  applyCode,
  applyHeading,
  applyHR,
  applyImage,
  applyItalic,
  applyLink,
  applyList,
  applyStrikethrough,
  applyTable,
} from '../utils/markdownToolbar.js';
import { getColorPair } from '../utils/noteColors.js';
import { collectAssetNames, markdownToShareHtml, noteToMarkdown, noteToPlainText } from '../utils/noteShare.js';
import { UndoStack } from '../utils/UndoStack.js';
import { imageActionsMenu } from './ImageActionsMenu.js';
import { imageViewerDialog } from './ImageViewerDialog.js';

/** Autosave debounce delay in milliseconds (matches Android app: 3 seconds) */
const AUTOSAVE_DEBOUNCE_MS = 3000;

/** Base64 data-URL budget for share/copy (~8MB — base64 is ≈1.33x the underlying file size).
 *  Assets past the budget degrade to a `[🖼 alt]` text placeholder instead of bloating the
 *  clipboard payload. */
const SHARE_DATA_URL_BUDGET = 8 * 1024 * 1024;

/** Matches a `.assets/<name>` image href (the second capture group of IMAGE_REGEX, standalone). */
const ASSET_HREF_REGEX = /^\.assets\/([A-Za-z0-9][A-Za-z0-9._-]*)$/;

function assetNameFrom(href) {
  return ASSET_HREF_REGEX.exec(href || '')?.[1] ?? null;
}

function escapeAttr(str) {
  return String(str).replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/** Ordinal counter for `.md-img` renders, reset per `updatePreview()` call — see the `ponytail:`
 *  note in imageAltTokens.js: it drifts from IMAGE_REGEX's raw-match count inside code fences,
 *  matching a latent bug Android has too. */
let previewImgOrdinal = 0;

/** Own Marked instance (not the global `marked.use()`) so NotesList.renderPreviewLine, which
 *  imports the default export, stays untouched. Renders `.assets/<name>` images without a
 *  `src` — DOMPurify's default ALLOWED_URI_REGEXP doesn't know the `snasset:` scheme and would
 *  strip it; `src` is set after sanitizing instead (see updatePreview()). External image URLs
 *  fall through to the default renderer (`return false`). */
const previewMarked = new Marked().use({
  renderer: {
    image({ href, text }) {
      const name = assetNameFrom(href);
      if (!name) return false;
      const { cleanAlt, sizePercent, align } = parseImageAlt(text || '');
      const ordinal = previewImgOrdinal++;
      const sizeFactor = sizePercent / 100;
      const style =
        align === 'inline'
          ? ` style="height:${sizeFactor * 2}em;max-width:${sizeFactor * 8}em"`
          : ` style="width:${sizePercent}%"`;
      return (
        `<img data-asset="${escapeAttr(name)}" data-ordinal="${ordinal}" data-size="${sizePercent}" ` +
        `data-align="${align}" alt="${escapeAttr(cleanAlt)}" class="md-img md-img--${align}"${style}>`
      );
    },
  },
});

/**
 * Note Editor Component
 */
export class NoteEditor {
  constructor() {
    this.container = document.getElementById('editor-container');
    this.editorDiv = document.getElementById('editor');
    this.previewDiv = document.getElementById('preview');
    this.titleInput = document.getElementById('note-title');
    this.syncStatus = document.getElementById('sync-status');
    this.deleteBtn = document.getElementById('delete-note-btn');
    this.menuBtn = document.getElementById('note-menu-btn');
    this.previewToggleBtn = document.getElementById('preview-toggle-btn');
    this.sortBtn = document.getElementById('checklist-sort-btn');
    this.checklistContainer = document.getElementById('checklist-container');
    this.placeholderDiv = document.getElementById('no-note-selected');

    this.undoBtn = document.getElementById('undo-btn');
    this.addItemHeaderBtn = document.getElementById('add-checklist-item-btn');
    this.colorBtn = document.getElementById('note-color-btn');

    this.mdToolbar = document.getElementById('markdown-toolbar');
    this.mdBtnBold = document.getElementById('md-btn-bold');
    this.mdBtnItalic = document.getElementById('md-btn-italic');
    this.mdBtnStrikethrough = document.getElementById('md-btn-strikethrough');
    this.mdBtnHeading = document.getElementById('md-btn-heading');
    this.mdBtnCode = document.getElementById('md-btn-code');
    this.mdBtnLink = document.getElementById('md-btn-link');
    this.mdBtnImage = document.getElementById('md-btn-image');
    this.mdBtnList = document.getElementById('md-btn-list');
    this.mdBtnChecklist = document.getElementById('md-btn-checklist');
    this.mdBtnHr = document.getElementById('md-btn-hr');
    this.mdBtnTable = document.getElementById('md-btn-table');

    this.editorView = null;
    this.currentNote = null;
    this.saveTimeout = null;
    this.autosave = true;
    this.onDeleteCallback = null;
    this.showPreview = false;

    // Undo/Redo stack (used for checklist notes and title changes)
    this._undoStack = new UndoStack(50);
    // Debounce timer for checklist text – we don't want a snapshot per keystroke
    this._undoPushTimer = null;
    // Debounce timer for local sidebar updates (title + preview refresh)
    this._localUpdateTimer = null;
    // True when the user has made changes that haven't been persisted to the server yet
    this._isDirty = false;
    // Warm cache of asset name → data URL for the current note, so selection-copy (synchronous
    // clipboard event) doesn't need to await IPC. Cleared per loadNote() — content-addressed
    // names never go stale, this is purely to bound memory across note switches.
    this._assetDataUrls = new Map();
    // Names currently being fetched by _warmAssetDataUrls(), so back-to-back calls (updatePreview
    // runs on every keystroke) don't fire duplicate IPC requests for the same asset.
    this._warmingAssets = new Set();

    this.defaultOpenMode = 'edit';
    this.imageCompressionMode = 'compressed';
    this.defaultImageSizePercent = 50;

    this.init();
  }

  init() {
    // Title input handler
    this.titleInput.addEventListener('input', () => {
      if (this.currentNote) {
        this.currentNote.title = this.titleInput.value;
        this._schedulePushSnapshot();
        this._scheduleLocalUpdate();
        this.scheduleSave();
      }
    });

    // F1: Enter im Title → Fokus auf Content / erstes Checklist-Item
    this.titleInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        if (this.currentNote?.noteType === 'CHECKLIST') {
          const firstItem = this.checklistContainer.querySelector('.checklist-item-text');
          firstItem?.focus();
        } else if (this.editorView) {
          this.editorView.focus();
        }
      }
    });

    // Delete button
    this.deleteBtn.addEventListener('click', () => this.handleDelete());

    // ⋮ overflow menu (share / copy-text / calendar / archive)
    this.menuBtn?.addEventListener('click', () => this.showNoteMenu());

    // Preview toggle
    this.previewToggleBtn.addEventListener('click', () => this.togglePreview());

    // Preview images: left-click opens the fullscreen viewer, right-click opens the
    // alignment/size/alt/info menu. Both delegate off `.md-img` (missing-asset placeholders
    // are plain divs, not images, so they're naturally excluded).
    this.previewDiv.addEventListener('click', (e) => {
      const img = e.target.closest('.md-img');
      if (!img) return;
      // A drag-select that ends on top of an image must not open the viewer — that would
      // destroy the just-made selection before the user can Ctrl+C it.
      if (!window.getSelection()?.isCollapsed) return;
      imageViewerDialog.show(img.dataset.asset);
    });

    this.previewDiv.addEventListener('contextmenu', (e) => {
      const img = e.target.closest('.md-img');
      if (!img) return;
      e.preventDefault();
      imageActionsMenu.show(
        e.clientX,
        e.clientY,
        {
          assetName: img.dataset.asset,
          sizePercent: Number(img.dataset.size),
          align: img.dataset.align,
          cleanAlt: img.alt,
          ordinal: Number(img.dataset.ordinal),
        },
        (newState) => this._applyImageRewrite(newState),
      );
    });

    // Selection-copy: rewrite `.md-img` sources from `snasset:` (app-only) to the warm data-URL
    // cache before letting the copy through, so pasted selections carry real images instead of
    // a src no external app can resolve. Falls through to native serialization when the
    // selection contains no images (no regression surface for plain-text copies).
    this.previewDiv.addEventListener('copy', (e) => {
      const sel = window.getSelection();
      if (!sel || sel.rangeCount === 0 || sel.isCollapsed) return;
      const fragment = sel.getRangeAt(0).cloneContents();
      if (!fragment.querySelector('img[data-asset]')) return;

      for (const img of fragment.querySelectorAll('img[data-asset]')) {
        const dataUrl = this._assetDataUrls.get(img.dataset.asset);
        if (dataUrl) {
          const alt = img.alt;
          for (const attr of [...img.attributes]) {
            if (attr.name === 'class' || attr.name.startsWith('data-')) img.removeAttribute(attr.name);
          }
          img.setAttribute('src', dataUrl);
          img.setAttribute('alt', alt);
          img.setAttribute('style', 'max-width:100%');
        } else {
          img.replaceWith(document.createTextNode(img.alt ? `[🖼 ${img.alt}]` : '[🖼]'));
        }
      }

      const holder = document.createElement('div');
      holder.appendChild(fragment);
      e.clipboardData.setData('text/html', holder.innerHTML);
      e.clipboardData.setData('text/plain', sel.toString());
      e.preventDefault();
    });

    // F2: Sort button with dropdown
    this.sortBtn.addEventListener('click', () => this.showSortMenu());

    // Undo button
    this.undoBtn.addEventListener('click', () => this._handleUndo());

    // Header add-item button (checklist only)
    this.addItemHeaderBtn.addEventListener('click', () => this.addChecklistItem());

    // Color button
    this.colorBtn?.addEventListener('click', () => {
      if (!this.currentNote) return;
      colorPicker.show(this.colorBtn, this.currentNote.color ?? null, async (color) => {
        this.currentNote.color = color;
        this._updateColorBtn();
        this.updateSyncStatus('Saving...');
        await this.save();
      });
    });

    // Markdown toolbar button listeners
    this.mdBtnBold?.addEventListener('click', () => this._mdFormat('bold'));
    this.mdBtnItalic?.addEventListener('click', () => this._mdFormat('italic'));
    this.mdBtnStrikethrough?.addEventListener('click', () => this._mdFormat('strikethrough'));
    this.mdBtnHeading?.addEventListener('click', () => this._mdFormat('heading'));
    this.mdBtnCode?.addEventListener('click', () => this._mdFormat('code'));
    this.mdBtnLink?.addEventListener('click', () => this._mdFormat('link'));
    this.mdBtnImage?.addEventListener('click', () => this._insertImage());
    this.mdBtnList?.addEventListener('click', () => this._mdFormat('list'));
    this.mdBtnChecklist?.addEventListener('click', () => this._mdFormat('checklist'));
    this.mdBtnHr?.addEventListener('click', () => this._mdFormat('hr'));
    this.mdBtnTable?.addEventListener('click', () => this._mdFormat('table'));

    // Ctrl+B / Ctrl+I shortcuts for bold and italic (text notes only)
    document.addEventListener('keydown', (e) => {
      if (!this.currentNote || this.currentNote.noteType !== 'TEXT') return;
      if (!(e.ctrlKey || e.metaKey)) return;
      if (e.key === 'b') {
        e.preventDefault();
        this._mdFormat('bold');
      }
      if (e.key === 'i') {
        e.preventDefault();
        this._mdFormat('italic');
      }
    });

    // Ctrl+Z / Ctrl+Shift+Z keyboard shortcut
    document.addEventListener('keydown', (e) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      if (e.key === 'z' && !e.shiftKey) {
        // Only intercept when a note is loaded
        if (!this.currentNote) return;
        if (this.currentNote.noteType === 'CHECKLIST') {
          e.preventDefault();
          this._handleUndo();
        }
        // For text notes: let CodeMirror handle it natively (its own Ctrl+Z)
      }
      if (e.key === 'Enter' && !e.shiftKey) {
        if (this.currentNote?.noteType !== 'CHECKLIST') return;
        if (!e.target.closest('.checklist-container, .note-title')) return;
        e.preventDefault();
        this.addChecklistItem();
      }
    });
  }

  _mdFormat(action) {
    if (!this.editorView) return;
    switch (action) {
      case 'bold':
        applyBold(this.editorView);
        break;
      case 'italic':
        applyItalic(this.editorView);
        break;
      case 'strikethrough':
        applyStrikethrough(this.editorView);
        break;
      case 'heading':
        applyHeading(this.editorView);
        break;
      case 'code':
        applyCode(this.editorView);
        break;
      case 'link':
        applyLink(this.editorView);
        break;
      case 'list':
        applyList(this.editorView);
        break;
      case 'checklist':
        applyChecklist(this.editorView);
        break;
      case 'hr':
        applyHR(this.editorView);
        break;
      case 'table':
        applyTable(this.editorView);
        break;
    }
  }

  /** Opens a native file picker, processes the chosen image, and inserts it at the cursor. */
  async _insertImage() {
    if (!this.editorView) return;
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: 'Images', extensions: ['jpg', 'jpeg', 'png', 'webp', 'gif'] }],
      });
      if (!path) return;
      const name = await attachImage(path, this.imageCompressionMode);
      applyImage(this.editorView, `.assets/${name}`, this.defaultImageSizePercent);
    } catch (error) {
      console.error('Failed to insert image:', error);
      await dialogService.error({ title: 'Insert Image', message: error.message || String(error) });
    }
  }

  initEditor() {
    if (this.editorView) {
      this.editorView.destroy();
    }

    // ponytail: EditorView.theme() wins over basicSetup's baseTheme (same scoping class)
    const gutterTheme = EditorView.theme({
      '.cm-gutters': {
        backgroundColor: 'var(--color-bg-elevated)',
        borderRight: '1px solid var(--color-border)',
        color: 'var(--color-muted)',
      },
      '.cm-activeLineGutter': {
        backgroundColor: 'color-mix(in srgb, var(--color-fg) 5%, transparent)',
      },
    });

    const startState = EditorState.create({
      doc: this.currentNote?.content || '',
      extensions: [
        basicSetup,
        gutterTheme,
        markdown(),
        ...markdownHighlightExtensions,
        EditorView.lineWrapping,
        EditorView.updateListener.of((update) => {
          if (update.docChanged && this.currentNote) {
            this.currentNote.content = update.state.doc.toString();
            this._scheduleLocalUpdate();
            this.scheduleSave();
            if (this.showPreview) {
              if (this._previewTimer) clearTimeout(this._previewTimer);
              this._previewTimer = setTimeout(() => this.updatePreview(), 200);
            }
          }
          // Keep undo-button state in sync with CodeMirror's own history
          if (update.docChanged || update.selectionSet) {
            this._updateUndoButton();
          }
        }),
      ],
    });

    this.editorView = new EditorView({
      state: startState,
      parent: this.editorDiv,
    });
  }

  togglePreview() {
    this.showPreview = !this.showPreview;

    if (this.showPreview) {
      this.editorDiv.classList.add('hidden');
      this.previewDiv.classList.remove('hidden');
      this.previewToggleBtn.classList.add('active');
      this.mdToolbar?.classList.add('hidden');
      this.updatePreview();
    } else {
      this.previewDiv.classList.add('hidden');
      this.editorDiv.classList.remove('hidden');
      this.previewToggleBtn.classList.remove('active');
      this.mdToolbar?.classList.remove('hidden');
    }
  }

  /** Renders the preview synchronously — text and images appear together, no IPC round-trip.
   *  Images load natively through the `snasset:` custom protocol (parallel, cached by the
   *  webview) after the sanitized HTML is in the DOM; see previewMarked's image renderer. */
  updatePreview() {
    const note = this.currentNote;
    if (!note?.content) {
      this.previewDiv.innerHTML = '<div class="empty-placeholder">No content to preview</div>';
      return;
    }

    previewImgOrdinal = 0;
    const html = previewMarked.parse(note.content);
    this.previewDiv.innerHTML = DOMPurify.sanitize(html);

    this.previewDiv.querySelectorAll('img[data-asset]').forEach((img) => {
      img.addEventListener('error', () => this._showMissingAssetPlaceholder(img), { once: true });
      img.src = assetUrl(img.dataset.asset);
    });

    this._warmAssetDataUrls();
  }

  /** Fire-and-forget: pre-fetches data URLs for every image in the current preview into
   *  `_assetDataUrls`, so the synchronous `copy` listener above never has to await IPC. */
  async _warmAssetDataUrls() {
    const noteId = this.currentNote?.id;
    for (const img of this.previewDiv.querySelectorAll('img[data-asset]')) {
      const name = img.dataset.asset;
      if (this._assetDataUrls.has(name) || this._warmingAssets.has(name)) continue;
      this._warmingAssets.add(name);
      const dataUrl = await getAssetDataUrl(name).catch(() => null);
      this._warmingAssets.delete(name);
      if (this.currentNote?.id !== noteId) return; // note switched mid-fetch
      if (dataUrl) this._assetDataUrls.set(name, dataUrl);
    }
  }

  /** Missing asset (not yet synced, deleted locally, or a manually typed link): swap the `<img>`
   *  for a placeholder with the broken-image icon + alt text, mirroring Android's ImagePlaceholder.
   *  `error` doesn't bubble, so this is wired per-image in updatePreview() rather than delegated. */
  _showMissingAssetPlaceholder(img) {
    const placeholder = document.createElement('div');
    placeholder.className = 'md-img-missing';
    placeholder.innerHTML = `
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M3 3l18 18M10.5 10.5a2 2 0 0 0 2.83 2.83M21 15V5a2 2 0 0 0-2-2H8m-5 4v10a2 2 0 0 0 2 2h13"/>
      </svg>
      <span></span>
    `;
    placeholder.querySelector('span').textContent = img.alt || 'Image not available';
    img.replaceWith(placeholder);
  }

  /** Right-click menu callback: rewrites the note's `ordinal`-th image link in place and lets
   *  the existing updateListener (docChanged) pick up content-update/autosave/preview-refresh —
   *  no separate save path needed. Silent no-op if the text shifted since the menu opened
   *  (computeImageRewrite returns null on ordinal/asset-name mismatch). */
  _applyImageRewrite({ assetName, ordinal, sizePercent, align, cleanAlt }) {
    if (!this.editorView) return;
    const content = this.editorView.state.doc.toString();
    const rewrite = computeImageRewrite(content, ordinal, assetName, sizePercent, align, cleanAlt);
    if (!rewrite) return;
    this.editorView.dispatch({ changes: rewrite });
  }

  // F2: Render checklist with sorting and separator
  renderChecklist() {
    if (!this.currentNote?.checklistItems) {
      return;
    }

    const sortOption = this.currentNote.checklistSortOption || 'UNCHECKED_FIRST';
    const items = this.sortChecklistItems(this.currentNote.checklistItems, sortOption);

    // F2: Separator-Position berechnen
    const checkedCount = items.filter((item) => item.isChecked).length;
    const uncheckedCount = items.length - checkedCount;
    let separatorIndex = -1;

    // Separator only makes sense when BOTH groups exist
    if (checkedCount > 0 && uncheckedCount > 0) {
      if (sortOption === 'UNCHECKED_FIRST' || sortOption === 'MANUAL') {
        separatorIndex = items.findIndex((item) => item.isChecked);
      } else if (sortOption === 'CHECKED_FIRST') {
        separatorIndex = items.findIndex((item) => !item.isChecked);
      }
    }

    // Render items with separator
    let html = '';
    items.forEach((item, index) => {
      if (index === separatorIndex) {
        html += this.renderSeparator(checkedCount);
      }
      html += this.renderChecklistItem(item, index);
    });

    html += `<button class="checklist-add-btn" id="add-checklist-item">+ Add Item</button>`;

    this.checklistContainer.innerHTML = html;

    // F2: Attach event listeners
    this.attachChecklistListeners(items);
  }

  // F2: Sort checklist items according to sort option
  sortChecklistItems(items, sortOption) {
    const sorted = [...items];

    switch (sortOption) {
      case 'ALPHABETICAL_ASC':
        sorted.sort((a, b) => a.text.localeCompare(b.text));
        break;
      case 'ALPHABETICAL_DESC':
        sorted.sort((a, b) => b.text.localeCompare(a.text));
        break;
      case 'UNCHECKED_FIRST':
        sorted.sort((a, b) => {
          if (a.isChecked !== b.isChecked) return a.isChecked ? 1 : -1;
          return a.order - b.order;
        });
        break;
      case 'CHECKED_FIRST':
        sorted.sort((a, b) => {
          if (a.isChecked !== b.isChecked) return a.isChecked ? -1 : 1;
          return a.order - b.order;
        });
        break;
      default: {
        // Android-Parität (ChecklistSorter.kt): unchecked vor checked, innerhalb jeder
        // Gruppe nach originalOrder sortiert (zementierte Drag-Drop-Position). Falls das
        // Feld fehlt (Pre-F04-Notiz), fällt order als Fallback ein.
        const unchecked = sorted
          .filter((i) => !i.isChecked)
          .sort((a, b) => (a.originalOrder ?? a.order) - (b.originalOrder ?? b.order));
        const checked = sorted
          .filter((i) => i.isChecked)
          .sort((a, b) => (a.originalOrder ?? a.order) - (b.originalOrder ?? b.order));
        return [...unchecked, ...checked];
      }
    }

    return sorted;
  }

  // Android-Parität: order und originalOrder nach jeder strukturellen Änderung synchron halten.
  _renumberOrders() {
    this.currentNote.checklistItems.forEach((item, i) => {
      item.order = i;
      item.originalOrder = i;
    });
  }

  addChecklistItem(afterItemId = null) {
    if (!this.currentNote || this.currentNote.noteType !== 'CHECKLIST') return null;

    const items = this.currentNote.checklistItems;
    const sort = this.currentNote.checklistSortOption ?? 'MANUAL';
    const hasSeparator = sort === 'MANUAL' || sort === 'UNCHECKED_FIRST';
    let insertIdx;

    if (afterItemId) {
      const trigger = items.findIndex((i) => i.id === afterItemId);
      if (hasSeparator && trigger >= 0 && items[trigger].isChecked) {
        const firstChecked = items.findIndex((i) => i.isChecked);
        insertIdx = firstChecked >= 0 ? firstChecked : trigger + 1;
      } else {
        insertIdx = trigger + 1;
      }
    } else {
      insertIdx = items.length;
    }

    const newItem = { id: crypto.randomUUID(), text: '', isChecked: false, order: insertIdx };
    items.splice(insertIdx, 0, newItem);
    this._renumberOrders();
    this._pushSnapshot();
    this.renderChecklist();
    this.scheduleSave();

    const el = this.checklistContainer.querySelector(
      `.checklist-item[data-item-id="${newItem.id}"] .checklist-item-text`,
    );
    if (el) {
      el.focus();
      requestAnimationFrame(() => {
        const item = el.closest('.checklist-item');
        if (!item) return;
        const containerRect = this.checklistContainer.getBoundingClientRect();
        const itemRect = item.getBoundingClientRect();
        const gap = containerRect.bottom - itemRect.bottom;
        if (gap < 48) this.checklistContainer.scrollTop += 48 - gap;
      });
    }
    return newItem;
  }

  // Einmalige Ableitung von originalOrder aus order für Notizen, die vor Android-F04 (v1.9.0)
  // erstellt wurden und das Feld noch nicht kennen. Kein Speichern — nur In-Memory-Fix.
  _fixPreF04Orders() {
    const items = this.currentNote?.checklistItems;
    if (!items || items.length === 0) return;
    const isPreF04 = items.every((item) => item.originalOrder == null || item.originalOrder === 0);
    if (isPreF04) {
      items.forEach((item) => {
        if (item.originalOrder == null) {
          item.originalOrder = item.order;
        }
      });
    }
  }

  // F2: Render separator between checked/unchecked groups
  renderSeparator(checkedCount) {
    const label = checkedCount === 1 ? '1 completed' : `${checkedCount} completed`;
    return `
      <div class="checklist-separator" data-separator="true">
        <div class="checklist-separator-line"></div>
        <span class="checklist-separator-text">${label}</span>
        <div class="checklist-separator-line"></div>
      </div>
    `;
  }

  // F2: Attach all event listeners for checklist items
  attachChecklistListeners(sortedItems) {
    const itemElements = this.checklistContainer.querySelectorAll('.checklist-item');

    itemElements.forEach((el, displayIndex) => {
      const item = sortedItems[displayIndex];
      if (!item) return;

      // Find the actual index in the original array
      const originalIndex = this.currentNote.checklistItems.findIndex((i) => i.id === item.id);
      if (originalIndex === -1) return;

      const checkbox = el.querySelector('input[type="checkbox"]');
      const textInput = el.querySelector('.checklist-item-text');
      const deleteBtn = el.querySelector('.checklist-item-delete');
      const calendarBtn = el.querySelector('.checklist-item-calendar');
      const dragHandle = el.querySelector('.checklist-drag-handle');

      // Checkbox toggle
      checkbox.addEventListener('change', () => {
        this._pushSnapshot(); // immediate snapshot before state changes
        this.currentNote.checklistItems[originalIndex].isChecked = checkbox.checked;
        this._pushSnapshot(); // snapshot after
        this._scheduleLocalUpdate();
        this.scheduleSave();
        // Always re-render so the item moves into its group and the separator
        // updates; restore focus to the same checkbox after the DOM rebuild.
        const itemId = item.id;
        this.renderChecklist();
        this.checklistContainer
          .querySelector(`.checklist-item[data-item-id="${itemId}"] input[type="checkbox"]`)
          ?.focus();
      });

      // Text input
      textInput.addEventListener('input', () => {
        this.currentNote.checklistItems[originalIndex].text = textInput.value;
        this._schedulePushSnapshot();
        this._scheduleLocalUpdate();
        this.scheduleSave();

        // v0.3.1: Auto-resize textarea and update scroll gradients
        this.autoResizeTextarea(textInput);
        const scrollContainer = textInput.closest('.checklist-item-scroll');
        if (scrollContainer) {
          this.updateScrollGradients(scrollContainer);
        }

        // F3: Visuellen Leer-Status aktualisieren
        const itemEl = textInput.closest('.checklist-item');
        if (textInput.value.trim() === '') {
          itemEl.classList.add('checklist-item-empty');
          if (!itemEl.querySelector('.checklist-item-unsaved-hint')) {
            const hint = document.createElement('span');
            hint.className = 'checklist-item-unsaved-hint';
            hint.textContent = 'not saved';
            itemEl.appendChild(hint);
          }
        } else {
          itemEl.classList.remove('checklist-item-empty');
          const hint = itemEl.querySelector('.checklist-item-unsaved-hint');
          if (hint) hint.remove();
        }
      });

      // F3: Bei Fokusverlust leere Einträge entfernen (mit kurzer Verzögerung für UX)
      textInput.addEventListener('blur', () => {
        if (textInput.value.trim() === '') {
          const targetNoteId = this.currentNote?.id;
          const targetItemId = item.id;
          setTimeout(() => {
            // Guard: user switched to a different note during the 200ms window
            if (this.currentNote?.id !== targetNoteId) return;
            const idx = this.currentNote.checklistItems.findIndex((i) => i.id === targetItemId);
            if (idx === -1) return;
            if (this.currentNote.checklistItems[idx].text.trim() !== '') return;
            this._pushSnapshot();
            this.currentNote.checklistItems.splice(idx, 1);
            this._renumberOrders();
            this._pushSnapshot();
            this.renderChecklist();
            this.scheduleSave();
          }, 200);
        }
      });

      // F1: Enter → neues Item unterhalb erstellen
      textInput.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          this.addChecklistItem(item.id);
        }
      });

      // Delete button
      deleteBtn.addEventListener('click', () => {
        this._pushSnapshot();
        this.currentNote.checklistItems.splice(originalIndex, 1);
        this._renumberOrders();
        this._pushSnapshot();
        this.renderChecklist();
        this.scheduleSave();
      });

      // Add to calendar
      calendarBtn?.addEventListener('click', async () => {
        const payload = buildCalendarPayloadForItem(item, this.currentNote.title);
        try {
          await exportToCalendar(payload.title, payload.description);
        } catch (error) {
          console.error('Failed to export item to calendar:', error);
          await dialogService.error({ title: 'Calendar', message: error.message || String(error) });
        }
      });

      // F2: Drag-and-Drop via drag handle
      if (dragHandle) {
        this.setupDragAndDrop(dragHandle, el, displayIndex, sortedItems);
      }

      // v0.3.1: Scroll event for gradient visibility
      const scrollContainer = el.querySelector('.checklist-item-scroll');
      if (scrollContainer) {
        scrollContainer.addEventListener('scroll', () => {
          this.updateScrollGradients(scrollContainer);
        });
      }
    });

    // Add new item button
    const addBtn = this.checklistContainer.querySelector('#add-checklist-item');
    if (addBtn) {
      addBtn.addEventListener('click', () => this.addChecklistItem());
    }

    // v0.3.1: Initialize textarea auto-resize and scroll gradients
    this.initChecklistTextareas();
  }

  // F2: Drag-and-Drop setup for a checklist item
  setupDragAndDrop(handle, itemElement, displayIndex, sortedItems) {
    handle.addEventListener('mousedown', (e) => {
      e.preventDefault();
      const dragItem = itemElement;
      const container = this.checklistContainer;
      const allItems = Array.from(container.querySelectorAll('.checklist-item'));
      const dragIndex = allItems.indexOf(dragItem);

      dragItem.classList.add('dragging');
      let currentIndex = dragIndex;

      const startY = e.clientY;
      const initialScrollTop = container.scrollTop;
      const itemHeight = dragItem.getBoundingClientRect().height;
      // Cache initial item center positions for separator-aware calculation
      const itemCenters = allItems.map((el) => {
        const rect = el.getBoundingClientRect();
        return rect.top + rect.height / 2;
      });
      // Cache container rect once — it doesn't change during a drag
      const containerRect = container.getBoundingClientRect();

      // Auto-scroll state
      const SCROLL_ZONE = 60;
      const SCROLL_SPEED = 8;
      let scrollRAF = null;
      let scrollDir = 0;

      const runScroll = () => {
        if (scrollDir !== 0) {
          container.scrollTop += scrollDir * SCROLL_SPEED;
          scrollRAF = requestAnimationFrame(runScroll);
        }
      };

      const updateAutoScroll = (mouseY) => {
        let newDir = 0;
        if (mouseY < containerRect.top + SCROLL_ZONE) newDir = -1;
        else if (mouseY > containerRect.bottom - SCROLL_ZONE) newDir = 1;
        if (newDir !== scrollDir) {
          scrollDir = newDir;
          if (scrollRAF) cancelAnimationFrame(scrollRAF);
          if (scrollDir !== 0) scrollRAF = requestAnimationFrame(runScroll);
        }
      };

      // Stop the rAF loop if the window loses focus before mouseup is delivered
      const onBlur = () => {
        if (scrollRAF) cancelAnimationFrame(scrollRAF);
        scrollDir = 0;
      };
      window.addEventListener('blur', onBlur);

      // Track source item's checked state for visual cross-boundary feedback
      const sourceItem = sortedItems[displayIndex];
      const sourceChecked = sourceItem ? sourceItem.isChecked : false;
      let visuallyToggled = false;

      const onMouseMove = (moveEvent) => {
        // Compensate transform for container scroll so the item follows the mouse
        const scrollOffset = container.scrollTop - initialScrollTop;
        const deltaY = moveEvent.clientY - startY + scrollOffset;
        dragItem.style.transform = `translateY(${deltaY}px)`;
        dragItem.style.zIndex = '100';

        // Calculate target based on actual item center positions (separator-aware).
        // deltaY already incorporates scrollOffset, so comparing itemCenters[dragIndex] + deltaY
        // against itemCenters[i] is equivalent to the scroll-adjusted form but without allocation.
        const dragCenter = itemCenters[dragIndex] + deltaY;
        let clampedTarget = dragIndex;
        let minDist = Infinity;
        for (let i = 0; i < itemCenters.length; i++) {
          const dist = Math.abs(dragCenter - itemCenters[i]);
          if (dist < minDist) {
            minDist = dist;
            clampedTarget = i;
          }
        }
        clampedTarget = Math.max(0, Math.min(allItems.length - 1, clampedTarget));

        // Visual cross-boundary feedback: toggle dragged item appearance
        // when it hovers over an item with a different checked state
        const targetItem = sortedItems[clampedTarget];
        const wouldToggle = targetItem && targetItem.isChecked !== sourceChecked;
        if (wouldToggle !== visuallyToggled) {
          visuallyToggled = wouldToggle;
          const checkbox = dragItem.querySelector('input[type="checkbox"]');
          const textEl = dragItem.querySelector('.checklist-item-text');
          const displayChecked = wouldToggle ? !sourceChecked : sourceChecked;
          dragItem.classList.toggle('checked', displayChecked);
          if (checkbox) checkbox.checked = displayChecked;
          if (textEl) textEl.style.opacity = displayChecked ? '0.5' : '1';
        }

        updateAutoScroll(moveEvent.clientY);

        if (clampedTarget !== currentIndex) {
          // Visual feedback: shift items AND separator together.
          // Build a visual slot list (items + separator) in DOM order so the
          // separator moves with the items that surround it.
          const allVisual = Array.from(
            container.querySelectorAll('.checklist-item, .checklist-separator[data-separator]'),
          );
          const dragVisualIdx = allVisual.indexOf(dragItem);
          const targetVisualIdx = allVisual.indexOf(allItems[clampedTarget]);

          allVisual.forEach((el, vi) => {
            if (el === dragItem) return;
            const lo = Math.min(dragVisualIdx, targetVisualIdx);
            const hi = Math.max(dragVisualIdx, targetVisualIdx);
            if (vi >= lo && vi <= hi) {
              const shift = clampedTarget > dragIndex ? -itemHeight : itemHeight;
              el.style.transform = `translateY(${shift}px)`;
            } else {
              el.style.transform = '';
            }
          });
          currentIndex = clampedTarget;
        }
      };

      const onMouseUp = () => {
        document.removeEventListener('mousemove', onMouseMove);
        document.removeEventListener('mouseup', onMouseUp);
        window.removeEventListener('blur', onBlur);

        if (scrollRAF) cancelAnimationFrame(scrollRAF);
        scrollDir = 0;

        // Reset visual state (items + separator)
        container.querySelectorAll('.checklist-item, .checklist-separator[data-separator]').forEach((el) => {
          el.style.transform = '';
          el.style.zIndex = '';
        });
        dragItem.classList.remove('dragging');

        if (currentIndex !== dragIndex) {
          // Perform the move in the data model
          const sourceItem = sortedItems[dragIndex];
          const targetItem = sortedItems[currentIndex];

          if (sourceItem && targetItem) {
            const sourceOrigIdx = this.currentNote.checklistItems.findIndex((i) => i.id === sourceItem.id);
            const targetOrigIdx = this.currentNote.checklistItems.findIndex((i) => i.id === targetItem.id);

            if (sourceOrigIdx !== -1 && targetOrigIdx !== -1) {
              // Remove from source and insert at target
              const [moved] = this.currentNote.checklistItems.splice(sourceOrigIdx, 1);

              // Auto-toggle: if dragged across the checked/unchecked boundary,
              // toggle the isChecked state (matching Android app behavior)
              if (sourceItem.isChecked !== targetItem.isChecked) {
                moved.isChecked = targetItem.isChecked;
              }

              const insertIdx = this.currentNote.checklistItems.findIndex((i) => i.id === targetItem.id);
              this.currentNote.checklistItems.splice(currentIndex > dragIndex ? insertIdx + 1 : insertIdx, 0, moved);
              this._renumberOrders();

              // Switch to MANUAL sort if re-ordering manually
              this.currentNote.checklistSortOption = 'MANUAL';
            }
          }

          this._pushSnapshot();
          this.renderChecklist();
          this.scheduleSave();
        }
      };

      document.addEventListener('mousemove', onMouseMove);
      document.addEventListener('mouseup', onMouseUp);
    });
  }

  // F2: Render checklist item with drag handle
  // v0.3.1: textarea for word-wrap + scroll gradient container
  renderChecklistItem(item, _index) {
    const checked = item.isChecked ? 'checked' : '';
    const isEmpty = item.text.trim() === '';
    const emptyClass = isEmpty ? 'checklist-item-empty' : '';
    return `
      <div class="checklist-item ${checked} ${emptyClass}" data-item-id="${item.id}">
        <span class="checklist-drag-handle" title="Drag to reorder">⋮</span>
        <input type="checkbox" ${checked} />
        <div class="checklist-item-text-wrapper">
          <div class="checklist-item-scroll">
            <textarea class="checklist-item-text" placeholder="Item..." rows="1">${this.escapeHtml(item.text)}</textarea>
          </div>
          <div class="checklist-gradient-top"></div>
          <div class="checklist-gradient-bottom"></div>
        </div>
        <button class="checklist-item-calendar" type="button" title="Add to calendar" ${isEmpty ? 'disabled' : ''}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <rect x="3" y="4" width="18" height="18" rx="2" ry="2"></rect>
            <line x1="16" y1="2" x2="16" y2="6"></line>
            <line x1="8" y1="2" x2="8" y2="6"></line>
            <line x1="3" y1="10" x2="21" y2="10"></line>
          </svg>
        </button>
        <button class="checklist-item-delete" type="button">✕</button>
        ${isEmpty ? '<span class="checklist-item-unsaved-hint">not saved</span>' : ''}
      </div>
    `;
  }

  // F2: Show sort option menu
  showSortMenu() {
    // Remove any stale outside-click handler from a previous open/close cycle
    if (this._sortMenuCloseHandler) {
      document.removeEventListener('click', this._sortMenuCloseHandler);
      this._sortMenuCloseHandler = null;
    }

    const existing = document.querySelector('.sort-menu');
    if (existing) {
      existing.remove();
      return;
    }

    const currentSort = this.currentNote?.checklistSortOption || 'UNCHECKED_FIRST';
    const options = [
      { value: 'MANUAL', label: 'Manual (Drag & Drop)' },
      { value: 'UNCHECKED_FIRST', label: 'Unchecked First' },
      { value: 'CHECKED_FIRST', label: 'Checked First' },
      { value: 'ALPHABETICAL_ASC', label: 'A → Z' },
      { value: 'ALPHABETICAL_DESC', label: 'Z → A' },
    ];

    const menu = document.createElement('div');
    menu.className = 'sort-menu';
    menu.innerHTML = options
      .map(
        (opt) => `
      <div class="sort-menu-item ${opt.value === currentSort ? 'active' : ''}" data-value="${opt.value}">
        ${opt.value === currentSort ? '● ' : '○ '}${opt.label}
      </div>
    `,
      )
      .join('');

    // Position below sort button
    const rect = this.sortBtn.getBoundingClientRect();
    menu.style.position = 'fixed';
    menu.style.top = `${rect.bottom + 4}px`;
    menu.style.right = `${window.innerWidth - rect.right}px`;

    document.body.appendChild(menu);

    // Click handler
    menu.addEventListener('click', (e) => {
      const item = e.target.closest('.sort-menu-item');
      if (!item) return;

      const value = item.dataset.value;
      this.currentNote.checklistSortOption = value;
      this.renderChecklist();
      this.scheduleSave();
      menu.remove();
    });

    // Close on outside click — store handler on the instance so it can be
    // cleaned up if showSortMenu() is called again before it fires naturally.
    const closeHandler = (e) => {
      if (!menu.contains(e.target) && e.target !== this.sortBtn) {
        menu.remove();
        document.removeEventListener('click', closeHandler);
        this._sortMenuCloseHandler = null;
      }
    };
    this._sortMenuCloseHandler = closeHandler;
    setTimeout(() => document.addEventListener('click', closeHandler), 0);
  }

  /** ⋮ overflow menu: share / copy-as-text / calendar / archive. Rebuilt on every open so the
   *  archive label always reflects the current note's `archivedAt` — no cached button state. */
  showNoteMenu() {
    if (this._noteMenuCloseHandler) {
      document.removeEventListener('click', this._noteMenuCloseHandler);
      this._noteMenuCloseHandler = null;
    }

    const existing = document.querySelector('.note-actions-menu');
    if (existing) {
      existing.remove();
      return;
    }

    const archived = !!this.currentNote?.archivedAt;

    const menu = document.createElement('div');
    menu.className = 'sort-menu note-actions-menu';
    menu.innerHTML = `
      <div class="sort-menu-item" data-action="share">Copy with images</div>
      <div class="sort-menu-item" data-action="copy-text">Copy as text</div>
      <div class="sort-menu-item" data-action="calendar">Add to calendar</div>
      <div class="sort-menu-separator"></div>
      <div class="sort-menu-item" data-action="archive">${archived ? 'Unarchive' : 'Archive'}</div>
    `;

    const rect = this.menuBtn.getBoundingClientRect();
    menu.style.position = 'fixed';
    menu.style.top = `${rect.bottom + 4}px`;
    menu.style.right = `${window.innerWidth - rect.right}px`;
    document.body.appendChild(menu);

    menu.addEventListener('click', (e) => {
      const item = e.target.closest('.sort-menu-item');
      if (!item) return;
      menu.remove();
      if (this._noteMenuCloseHandler) {
        document.removeEventListener('click', this._noteMenuCloseHandler);
        this._noteMenuCloseHandler = null;
      }

      switch (item.dataset.action) {
        case 'share':
          this._shareNote();
          break;
        case 'copy-text':
          this._copyPlainText();
          break;
        case 'calendar':
          this._handleCalendar();
          break;
        case 'archive':
          this.handleArchiveToggle();
          break;
      }
    });

    const closeHandler = (e) => {
      if (!menu.contains(e.target) && e.target !== this.menuBtn) {
        menu.remove();
        document.removeEventListener('click', closeHandler);
        this._noteMenuCloseHandler = null;
      }
    };
    this._noteMenuCloseHandler = closeHandler;
    setTimeout(() => document.addEventListener('click', closeHandler), 0);
  }

  /** "Add to calendar" — moved out of the header into the ⋮ menu, behavior unchanged. */
  async _handleCalendar() {
    if (!this.currentNote) return;
    const payload = buildCalendarPayload(this.currentNote);
    if (!payload) {
      await dialogService.error({ message: 'Note is empty' });
      return;
    }
    try {
      await exportToCalendar(payload.title, payload.description);
    } catch (error) {
      console.error('Failed to export to calendar:', error);
      await dialogService.error({ title: 'Calendar', message: error.message || String(error) });
    }
  }

  /** Resolves asset names to data URLs for share/copy: warm cache first, then IPC. Stops
   *  fetching once SHARE_DATA_URL_BUDGET is exceeded — the rest degrade to text placeholders
   *  in markdownToShareHtml() rather than bloating the clipboard payload. */
  async _resolveShareDataUrls(names) {
    const dataUrls = new Map();
    let budgetBytes = SHARE_DATA_URL_BUDGET;
    for (const name of names) {
      if (budgetBytes <= 0) {
        dataUrls.set(name, null);
        continue;
      }
      const dataUrl = this._assetDataUrls.get(name) ?? (await getAssetDataUrl(name).catch(() => null));
      if (dataUrl) {
        this._assetDataUrls.set(name, dataUrl);
        budgetBytes -= dataUrl.length;
      }
      dataUrls.set(name, dataUrl ?? null);
    }
    return dataUrls;
  }

  /** Briefly shows a status message, then reverts to "Saved" — there's no reusable toast
   *  component (UpdateToast is wired to the updater, dialogService is modal-only). */
  _flashSyncStatus(text) {
    this.updateSyncStatus(text);
    setTimeout(() => this.updateSyncStatus('Saved'), 1500);
  }

  /** ⋮ → Copy with images: rich clipboard (`text/html` with images inline at their position in
   *  the text) plus a plaintext fallback — the desktop equivalent of Android's system share sheet. */
  async _shareNote() {
    const note = this.currentNote;
    if (!note) return;
    const md = noteToMarkdown(note);
    if (!md.trim()) {
      await dialogService.error({ message: 'Note is empty' });
      return;
    }
    try {
      const dataUrls = await this._resolveShareDataUrls(collectAssetNames(md));
      await writeHtml(markdownToShareHtml(md, dataUrls), noteToPlainText(note));
      this._flashSyncStatus('Copied');
    } catch (error) {
      console.error('Failed to copy note with images:', error);
      await dialogService.error({ title: 'Copy with Images', message: error.message || String(error) });
    }
  }

  /** ⋮ → Copy as text: same content, no images — `[🖼 alt]` placeholders instead. */
  async _copyPlainText() {
    const note = this.currentNote;
    if (!note) return;
    const text = noteToPlainText(note);
    if (!text.trim()) {
      await dialogService.error({ message: 'Note is empty' });
      return;
    }
    try {
      await writeText(text);
      this._flashSyncStatus('Copied');
    } catch (error) {
      console.error('Failed to copy note as text:', error);
      await dialogService.error({ title: 'Share', message: error.message || String(error) });
    }
  }

  loadNote(note) {
    // Flush any pending autosave for the previous note before switching
    if (this.saveTimeout) {
      clearTimeout(this.saveTimeout);
      this.saveTimeout = null;
      if (this.currentNote) {
        this.saveNoteImmediate(this.currentNote);
      }
    }

    // Reset previous state completely
    if (this.editorView) {
      this.editorView.destroy();
      this.editorView = null;
    }
    this.editorDiv.innerHTML = '';
    this.checklistContainer.innerHTML = '';

    // Reset scroll positions before loading new content
    this.checklistContainer.scrollTop = 0;
    this.editorDiv.scrollTop = 0;
    this.previewDiv.scrollTop = 0;

    // Reset undo stack for the new note
    this._undoStack.clear();
    this._assetDataUrls.clear();
    this._warmingAssets.clear();
    if (this._undoPushTimer) {
      clearTimeout(this._undoPushTimer);
      this._undoPushTimer = null;
    }
    if (this._localUpdateTimer) {
      clearTimeout(this._localUpdateTimer);
      this._localUpdateTimer = null;
    }

    this.currentNote = { ...note };
    this._isDirty = false;
    this.titleInput.value = note.title;

    // Show container, hide placeholder
    this.container.classList.remove('hidden');
    this.placeholderDiv.classList.add('hidden');

    this.showPreview = false;
    this.previewToggleBtn.classList.remove('active');

    if (note.noteType === 'CHECKLIST') {
      // Show checklist, hide editor
      this.editorDiv.classList.add('hidden');
      this.previewDiv.classList.add('hidden');
      this.previewToggleBtn.classList.add('hidden');
      this.checklistContainer.classList.remove('hidden');
      this.sortBtn.classList.remove('hidden'); // F2: Show sort button
      this.addItemHeaderBtn.classList.remove('hidden');
      this.mdToolbar?.classList.add('hidden');
      this._fixPreF04Orders();
      this.renderChecklist();
      // Push initial snapshot so the first undo restores the loaded state
      this._pushSnapshot();
    } else {
      // Show editor (preview OFF by default)
      this.checklistContainer.classList.add('hidden');
      this.editorDiv.classList.remove('hidden');
      this.previewToggleBtn.classList.remove('hidden');
      this.sortBtn.classList.add('hidden'); // F2: Hide sort button for text notes
      this.addItemHeaderBtn.classList.add('hidden');
      this.mdToolbar?.classList.remove('hidden');
      this.initEditor();

      // Preview hidden by default; enable if default_open_mode is "preview" (existing notes only)
      if (this.defaultOpenMode === 'preview' && note.content) {
        this.togglePreview();
      } else {
        this.previewDiv.classList.add('hidden');
        this.editorDiv.style.width = '100%';
      }
    }

    this._updateUndoButton();
    this._updateColorBtn();
    this.updateSyncStatus('Saved');
  }

  focusContent() {
    if (!this.currentNote) return;
    if (this.currentNote.noteType === 'CHECKLIST') {
      this.addChecklistItem();
    } else {
      this.editorView?.focus();
    }
  }

  clear() {
    this.currentNote = null;
    this._isDirty = false;
    this._undoStack.clear();
    if (this._undoPushTimer) {
      clearTimeout(this._undoPushTimer);
      this._undoPushTimer = null;
    }
    if (this._localUpdateTimer) {
      clearTimeout(this._localUpdateTimer);
      this._localUpdateTimer = null;
    }
    this.container.classList.add('hidden');
    this.placeholderDiv.classList.remove('hidden');
    this.mdToolbar?.classList.add('hidden');
    if (this.undoBtn) this.undoBtn.disabled = true;
    this._updateColorBtn();

    if (this.editorView) {
      this.editorView.destroy();
      this.editorView = null;
    }
  }

  /** Setzt den Color-Button-Zustand basierend auf der aktuellen Notizfarbe. */
  _updateColorBtn() {
    if (!this.colorBtn) return;
    const color = this.currentNote?.color;
    const colorPair = getColorPair(color);
    if (color) {
      this.colorBtn.style.setProperty('--btn-color', color);
      this.colorBtn.classList.add('has-color');
      this.colorBtn.title = `Note color: ${color}`;
    } else {
      this.colorBtn.style.removeProperty('--btn-color');
      this.colorBtn.classList.remove('has-color');
      this.colorBtn.title = 'Note color';
    }
    if (colorPair) {
      this.container.style.setProperty('--nc-l', colorPair.light);
      this.container.style.setProperty('--nc-d', colorPair.dark);
      this.container.classList.add('has-color');
    } else {
      this.container.style.removeProperty('--nc-l');
      this.container.style.removeProperty('--nc-d');
      this.container.classList.remove('has-color');
    }
  }

  // ── Undo helpers ────────────────────────────────────────────────────────────

  /** Snapshot the current note state immediately. */
  _pushSnapshot() {
    if (!this.currentNote) return;
    this._undoStack.push({
      title: this.currentNote.title,
      checklistItems: this.currentNote.checklistItems,
    });
    this._updateUndoButton();
  }

  /**
   * Debounced snapshot push – used for rapid keystrokes so we don't bloat the
   * stack with one entry per character.  Flushes after 600 ms of inactivity.
   */
  _schedulePushSnapshot() {
    if (this._undoPushTimer) clearTimeout(this._undoPushTimer);
    this._undoPushTimer = setTimeout(() => {
      this._undoPushTimer = null;
      this._pushSnapshot();
    }, 600);
  }

  /**
   * Schedule a local-only cache update so the sidebar reflects the current
   * editor state (title, content, checklist preview) without waiting for
   * the server round-trip.  Debounced at 300 ms.
   */
  _scheduleLocalUpdate() {
    if (this._localUpdateTimer) clearTimeout(this._localUpdateTimer);
    this._localUpdateTimer = setTimeout(() => {
      this._localUpdateTimer = null;
      if (this.currentNote) {
        noteService.updateNoteLocally(this.currentNote);
      }
    }, 300);
  }

  /** Perform one undo step. */
  _handleUndo() {
    if (!this.currentNote) return;

    // Text notes: delegate entirely to CodeMirror's built-in undo
    if (this.currentNote.noteType !== 'CHECKLIST') {
      if (this.editorView) {
        cmUndo(this.editorView);
      }
      return;
    }

    // Flush any pending debounced snapshot first so we have a current entry
    if (this._undoPushTimer) {
      clearTimeout(this._undoPushTimer);
      this._undoPushTimer = null;
      this._pushSnapshot();
    }

    const snapshot = this._undoStack.undo();
    if (!snapshot) return;

    // Restore title
    this.currentNote.title = snapshot.title;
    this.titleInput.value = snapshot.title;

    // Restore checklist items (deep replace)
    if (snapshot.checklistItems !== null) {
      this.currentNote.checklistItems = snapshot.checklistItems.map((i) => ({ ...i }));
    }

    this.renderChecklist();
    this.scheduleSave();
    this._updateUndoButton();
  }

  /** Enable/disable the undo button based on stack state + editor type. */
  _updateUndoButton() {
    if (!this.undoBtn) return;
    if (!this.currentNote) {
      this.undoBtn.disabled = true;
      return;
    }
    if (this.currentNote.noteType === 'CHECKLIST') {
      this.undoBtn.disabled = !this._undoStack.canUndo;
    } else {
      // For text notes we always leave it enabled once a note is loaded
      // (CodeMirror manages its own history; querying it is not trivial)
      this.undoBtn.disabled = false;
    }
  }

  // ── /Undo helpers ───────────────────────────────────────────────────────────

  /**
   * Called after a sync refresh when the server version of the currently open note
   * is newer than what the editor holds.  Reloads silently if there are no unsaved
   * changes; shows a conflict dialog otherwise.
   */
  /** Echter Server-Konflikt (Backend hat SyncStatus=CONFLICT gesetzt). */
  async handleServerConflict() {
    if (!this.currentNote) return;
    const id = this.currentNote.id;
    const confirmed = await dialogService.confirm({
      title: 'Note Updated on Server',
      message: 'This note was changed on another device. Load the server version and discard your local changes?',
      confirmText: 'Load Server Version',
      cancelText: 'Keep Mine',
      type: 'warning',
    });
    if (this.saveTimeout) {
      clearTimeout(this.saveTimeout);
      this.saveTimeout = null;
    }
    if (!confirmed && this._isDirty) await this.save();
    await noteService.resolveConflict(id, confirmed ? 'use_server' : 'keep_mine');
    // notes enthält NoteMetadata (kein deviceId) — volles Note-Objekt holen.
    const resolved = await noteService.getNote(id);
    if (resolved) this.loadNote(resolved);
  }

  /** Fire-and-forget save for a specific note snapshot (used to flush pending saves). */
  saveNoteImmediate(noteToSave) {
    noteService.saveNote(this._sanitizeForSave(noteToSave)).catch((err) => {
      console.error('[NoteEditor] Background save failed:', err);
      // Only update the status indicator if the user is still viewing the same note.
      // If the note was switched before the response arrived, do NOT clobber the new
      // note's status with an error that belongs to the previous one.
      if (this.currentNote?.id === noteToSave.id) {
        this.updateSyncStatus('Save error');
      }
    });
  }

  scheduleSave() {
    if (!this.currentNote) {
      return;
    }
    this._isDirty = true;

    if (!this.autosave) {
      return;
    }

    // F3: Checklist-Schutz – nicht speichern wenn ein Eintrag gerade leer ist
    if (this.currentNote.noteType === 'CHECKLIST' && this.hasEmptyChecklistItems()) {
      this.updateSyncStatus('Unsaved (empty item)');
      return;
    }

    this.updateSyncStatus('Saving...');

    if (this.saveTimeout) {
      clearTimeout(this.saveTimeout);
    }

    this.saveTimeout = setTimeout(() => {
      this.saveTimeout = null;
      this.save();
    }, AUTOSAVE_DEBOUNCE_MS);
  }

  // F3: Check if any checklist items are empty
  hasEmptyChecklistItems() {
    if (!this.currentNote?.checklistItems) {
      return false;
    }
    return this.currentNote.checklistItems.some((item) => item.text.trim() === '');
  }

  /** Returns a shallow copy of `note` with empty checklist items removed (does NOT mutate the
   *  original, so Undo snapshots remain intact). Non-checklist notes are returned as-is. */
  _sanitizeForSave(note) {
    if (note?.noteType !== 'CHECKLIST' || !Array.isArray(note.checklistItems)) {
      return note;
    }
    const cleaned = note.checklistItems
      .filter((i) => i.text.trim() !== '')
      .map((i, idx) => ({ ...i, order: idx, originalOrder: idx }));
    return { ...note, checklistItems: cleaned };
  }

  async save() {
    if (!this.currentNote) {
      return;
    }
    const noteToSave = this._sanitizeForSave(this.currentNote);

    try {
      const updatedNote = await noteService.saveNote(noteToSave);

      // Only apply the server response if the user hasn't switched to a different note
      // while the network request was in flight. Without this guard, the resolved promise
      // would overwrite this.currentNote with the old note, leaving the editor pointing
      // at stale data and causing ghost saves of unintended notes.
      if (this.currentNote?.id === noteToSave.id) {
        // Keep the live client checklistItems — they may contain unsaved empty items that
        // _sanitizeForSave stripped. Replacing them with the server copy would desync
        // the DOM event listeners (whose originalIndex was captured at render time).
        this.currentNote = { ...updatedNote, checklistItems: this.currentNote.checklistItems };
        this._isDirty = false;
        this.updateSyncStatus('Saved');
      }
    } catch (error) {
      console.error('Failed to save note:', error);
      if (this.currentNote?.id === noteToSave.id) {
        this.updateSyncStatus('Save error');
      }
    }
  }

  async handleDelete() {
    if (!this.currentNote) {
      return;
    }

    // F4: Server-Deletion-Warnungsdialog (gemeinsame Komponente mit F6)
    const confirmed = await dialogService.confirmDeletion(1);

    if (!confirmed) {
      return;
    }

    try {
      await noteService.deleteNote(this.currentNote.id);
      this.clear();

      if (this.onDeleteCallback) {
        this.onDeleteCallback();
      }
    } catch (error) {
      console.error('Failed to delete note:', error);
      await dialogService.error({
        title: 'Move to Trash Failed',
        message: 'Failed to move note to trash. Please check your connection and try again.',
      });
    }
  }

  async handleArchiveToggle() {
    if (!this.currentNote) {
      return;
    }

    // Flush a pending autosave/dirty edit first so archive_notes doesn't overwrite it
    // with the last-persisted state (mirrors Android's toggleArchive() flush-before-toggle).
    if (this.saveTimeout) {
      clearTimeout(this.saveTimeout);
      this.saveTimeout = null;
    }
    if (this._isDirty) {
      await this.save();
    }

    const archived = !this.currentNote.archivedAt;

    try {
      await noteService.archiveNotes([this.currentNote.id], archived);
      this.clear();

      if (this.onDeleteCallback) {
        this.onDeleteCallback();
      }
    } catch (error) {
      console.error('Failed to archive note:', error);
      await dialogService.error({
        title: 'Archive Failed',
        message: 'Failed to update the note. Please check your connection and try again.',
      });
    }
  }

  updateSyncStatus(text) {
    this.syncStatus.textContent = text;
  }

  isDirty() {
    return this._isDirty;
  }

  setAutosave(enabled) {
    this.autosave = enabled;
  }

  setDefaultOpenMode(mode) {
    this.defaultOpenMode = mode || 'edit';
  }

  setImageCompressionMode(mode) {
    this.imageCompressionMode = mode || 'compressed';
  }

  setDefaultImageSizePercent(percent) {
    this.defaultImageSizePercent = percent || 50;
  }

  onDelete(callback) {
    this.onDeleteCallback = callback;
  }

  // ── v0.3.1: Textarea & Scroll Gradient Helpers ──────────────────────────────

  /**
   * Auto-resize a textarea to fit its content.
   * The textarea has overflow:hidden; the scroll container (parent) clips at max-height.
   */
  autoResizeTextarea(textarea) {
    textarea.style.height = 'auto';
    textarea.style.height = `${textarea.scrollHeight}px`;
  }

  /**
   * Update gradient visibility based on scroll position of a .checklist-item-scroll container.
   * - Top gradient: visible when scrolled down (content above hidden)
   * - Bottom gradient: visible when more content below
   */
  updateScrollGradients(scrollContainer) {
    const wrapper = scrollContainer.closest('.checklist-item-text-wrapper');
    if (!wrapper) return;

    const topGradient = wrapper.querySelector('.checklist-gradient-top');
    const bottomGradient = wrapper.querySelector('.checklist-gradient-bottom');
    if (!topGradient || !bottomGradient) return;

    const { scrollTop, scrollHeight, clientHeight } = scrollContainer;
    const hasOverflow = scrollHeight > clientHeight + 1;

    // Top: visible when user has scrolled down
    topGradient.classList.toggle('visible', scrollTop > 0 && hasOverflow);

    // Bottom: visible when more content exists below current view
    bottomGradient.classList.toggle('visible', hasOverflow && scrollTop + clientHeight < scrollHeight - 1);
  }

  /**
   * Initialize all checklist textareas: auto-resize to fit content,
   * then check scroll overflow and set initial gradient visibility.
   * Must be called after attachChecklistListeners().
   */
  initChecklistTextareas() {
    const textareas = this.checklistContainer.querySelectorAll('.checklist-item-text');
    textareas.forEach((textarea) => {
      this.autoResizeTextarea(textarea);
    });

    // Check gradients after browser has laid out the resized textareas
    requestAnimationFrame(() => {
      const scrollContainers = this.checklistContainer.querySelectorAll('.checklist-item-scroll');
      scrollContainers.forEach((container) => {
        this.updateScrollGradients(container);
      });
    });
  }

  // ── /v0.3.1 Helpers ─────────────────────────────────────────────────────────

  escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }
}
