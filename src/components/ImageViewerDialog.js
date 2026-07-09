import { assetUrl } from '../services/tauri.js';

const MIN_SCALE = 1;
const MAX_SCALE = 5;
const DOUBLE_TAP_SCALE = 2.5;

/**
 * Fullscreen image viewer: click backdrop or Esc closes, double-click toggles 1x/2.5x,
 * wheel zooms 1x-5x, drag pans (clamped) while scale > 1. Port of Android `ImageViewerDialog.kt`.
 */
class ImageViewerDialog {
  constructor() {
    this.overlay = null;
    this.scale = MIN_SCALE;
    this.offsetX = 0;
    this.offsetY = 0;
    this._dragging = false;
  }

  _ensureOverlay() {
    if (this.overlay) return;
    this.overlay = document.getElementById('image-viewer-overlay');
    this.img = this.overlay.querySelector('#image-viewer-img');

    this.overlay.addEventListener('click', (e) => {
      if (e.target === this.overlay) this.hide();
    });
    this.overlay.addEventListener('dblclick', () => {
      if (this.scale > MIN_SCALE) {
        this._setTransform(MIN_SCALE, 0, 0);
      } else {
        this._setTransform(DOUBLE_TAP_SCALE, 0, 0);
      }
    });
    this.overlay.addEventListener('wheel', (e) => {
      e.preventDefault();
      const next = Math.min(MAX_SCALE, Math.max(MIN_SCALE, this.scale - e.deltaY * 0.01));
      this._setTransform(next, this.offsetX, this.offsetY);
    });
    this.img.addEventListener('mousedown', (e) => {
      if (this.scale <= MIN_SCALE) return;
      this._dragging = true;
      this._dragStart = { x: e.clientX - this.offsetX, y: e.clientY - this.offsetY };
    });
    window.addEventListener('mousemove', (e) => {
      if (!this._dragging) return;
      this._setTransform(this.scale, e.clientX - this._dragStart.x, e.clientY - this._dragStart.y);
    });
    window.addEventListener('mouseup', () => {
      this._dragging = false;
    });
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && this.isVisible()) this.hide();
    });
  }

  _setTransform(scale, offsetX, offsetY) {
    this.scale = scale;
    const rect = this.overlay.getBoundingClientRect();
    const maxX = ((scale - 1) * rect.width) / 2;
    const maxY = ((scale - 1) * rect.height) / 2;
    this.offsetX = Math.min(maxX, Math.max(-maxX, offsetX));
    this.offsetY = Math.min(maxY, Math.max(-maxY, offsetY));
    this.img.style.transform = `translate(${this.offsetX}px, ${this.offsetY}px) scale(${this.scale})`;
  }

  show(assetName) {
    this._ensureOverlay();
    this.img.src = assetUrl(assetName);
    this._setTransform(MIN_SCALE, 0, 0);
    this.overlay.classList.remove('hidden');
  }

  isVisible() {
    return this.overlay != null && !this.overlay.classList.contains('hidden');
  }

  hide() {
    this.overlay?.classList.add('hidden');
    if (this.img) this.img.src = '';
  }
}

export const imageViewerDialog = new ImageViewerDialog();
