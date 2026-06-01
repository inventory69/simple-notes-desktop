import { NOTE_COLORS } from './noteColors.js';

class ColorPicker {
  constructor() {
    this.popup = null;
    this._onSelect = null;
    this._outsideClickHandler = null;
  }

  _ensurePopup() {
    if (!this.popup) {
      this.popup = document.getElementById('color-picker-popup');
      // Kreise einmalig rendern
      const container = this.popup.querySelector('.color-circles');
      for (const c of NOTE_COLORS) {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'color-circle';
        btn.dataset.color = c.light;
        btn.title = c.name;
        btn.style.backgroundColor = c.light;
        btn.setAttribute('aria-label', c.name);
        container.appendChild(btn);
      }
      // Delegierter Klick-Handler für alle Kreise (auch "none")
      container.addEventListener('click', (e) => {
        const btn = e.target.closest('.color-circle');
        if (!btn) return;
        const color = btn.dataset.color || null; // '' → null
        this.hide();
        this._onSelect?.(color);
      });
    }
  }

  show(anchorEl, currentColor, onSelect) {
    this._ensurePopup();
    this._onSelect = onSelect;

    // Aktiven Kreis markieren
    this.popup.querySelectorAll('.color-circle').forEach((btn) => {
      btn.classList.toggle('active', btn.dataset.color === (currentColor ?? ''));
    });

    // Popup positionieren (unter dem Anchor-Button)
    const rect = anchorEl.getBoundingClientRect();
    this.popup.style.top = `${rect.bottom + 4}px`;
    this.popup.style.left = `${rect.left}px`;
    this.popup.classList.remove('hidden');

    // Outside-click zum Schließen
    setTimeout(() => {
      this._outsideClickHandler = (e) => {
        if (!this.popup.contains(e.target) && e.target !== anchorEl) {
          this.hide();
        }
      };
      document.addEventListener('click', this._outsideClickHandler);
    }, 0);
  }

  isVisible() {
    return this.popup != null && !this.popup.classList.contains('hidden');
  }

  hide() {
    this.popup?.classList.add('hidden');
    if (this._outsideClickHandler) {
      document.removeEventListener('click', this._outsideClickHandler);
      this._outsideClickHandler = null;
    }
  }
}

export const colorPicker = new ColorPicker();
