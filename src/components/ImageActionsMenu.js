import { dialogService } from '../services/DialogService.js';
import { getImageMetadata } from '../services/tauri.js';
import { SIZE_PRESETS } from '../utils/imageAltTokens.js';

/**
 * Right-click popup for a rendered `.md-img`: alignment, size presets, alt text, and (only when
 * the asset is original quality, not a re-encoded WebP) an info button.
 * Port of Android `ImageActionsMenu.kt`. Alignment/size commit immediately on click; alt text
 * commits on close (outside click / Escape), mirroring Android's `commitDismiss`.
 */
class ImageActionsMenu {
  constructor() {
    this.popup = null;
    this._state = null;
    this._onChange = null;
    this._outsideClickHandler = null;
    this._keydownHandler = null;
  }

  _ensurePopup() {
    if (this.popup) return;
    this.popup = document.getElementById('image-actions-menu');

    const sizeRow = this.popup.querySelector('.image-menu-sizes');
    for (const preset of SIZE_PRESETS) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'image-menu-size-btn';
      btn.dataset.size = String(preset.value);
      btn.textContent = preset.label;
      sizeRow.appendChild(btn);
    }

    this.altInput = this.popup.querySelector('.image-menu-alt-input');
    this.infoBtn = this.popup.querySelector('.image-menu-info-btn');

    this.popup.addEventListener('click', (e) => {
      const alignBtn = e.target.closest('[data-align]');
      if (alignBtn) {
        this._commit({ align: alignBtn.dataset.align });
        return;
      }
      const sizeBtn = e.target.closest('.image-menu-size-btn');
      if (sizeBtn) {
        this._commit({ sizePercent: Number(sizeBtn.dataset.size) });
        return;
      }
      if (e.target.closest('.image-menu-info-btn')) {
        const { assetName } = this._state;
        this.hide();
        getImageMetadata(assetName).then((meta) => {
          if (meta) dialogService.imageInfo(meta);
        });
      }
    });
  }

  /**
   * @param {number} x - viewport X to anchor the popup at
   * @param {number} y - viewport Y to anchor the popup at
   * @param {{assetName: string, sizePercent: number, align: string, cleanAlt: string, ordinal: number}} imageState
   * @param {(state: {sizePercent: number, align: string, cleanAlt: string}) => void} onChange
   */
  show(x, y, imageState, onChange) {
    this._ensurePopup();
    this._state = { ...imageState };
    this._onChange = onChange;

    this.altInput.value = imageState.cleanAlt;
    this._syncActiveButtons();
    this.infoBtn.classList.add('hidden');
    getImageMetadata(imageState.assetName).then((meta) => {
      if (this._state?.assetName === imageState.assetName) {
        // ponytail: re-encoded Compressed/Lossless assets are always `.webp` (see images.rs
        // process()); anything else came through untouched, so metadata is always meaningful.
        const isReencoded = imageState.assetName.endsWith('.webp');
        this.infoBtn.classList.toggle('hidden', !meta || isReencoded);
      }
    });

    this.popup.style.left = `${x}px`;
    this.popup.style.top = `${y}px`;
    this.popup.classList.remove('hidden');

    setTimeout(() => {
      this._outsideClickHandler = (e) => {
        if (!this.popup.contains(e.target)) this._commitDismiss();
      };
      document.addEventListener('click', this._outsideClickHandler);
      this._keydownHandler = (e) => {
        if (e.key === 'Escape') this._commitDismiss();
      };
      document.addEventListener('keydown', this._keydownHandler);
    }, 0);
  }

  isVisible() {
    return this.popup != null && !this.popup.classList.contains('hidden');
  }

  _syncActiveButtons() {
    this.popup.querySelectorAll('[data-align]').forEach((btn) => {
      btn.classList.toggle('active', btn.dataset.align === this._state.align);
    });
    this.popup.querySelectorAll('.image-menu-size-btn').forEach((btn) => {
      btn.classList.toggle('active', Number(btn.dataset.size) === this._state.sizePercent);
    });
  }

  _commit(partial) {
    if (!this._state) return;
    this._state = { ...this._state, ...partial, cleanAlt: this.altInput.value };
    this._syncActiveButtons();
    this._onChange?.(this._state);
  }

  _commitDismiss() {
    if (this._state && this.altInput.value !== this._state.cleanAlt) {
      this._state = { ...this._state, cleanAlt: this.altInput.value };
      this._onChange?.(this._state);
    }
    this.hide();
  }

  hide() {
    this.popup?.classList.add('hidden');
    this._state = null;
    if (this._outsideClickHandler) {
      document.removeEventListener('click', this._outsideClickHandler);
      this._outsideClickHandler = null;
    }
    if (this._keydownHandler) {
      document.removeEventListener('keydown', this._keydownHandler);
      this._keydownHandler = null;
    }
  }
}

export const imageActionsMenu = new ImageActionsMenu();
