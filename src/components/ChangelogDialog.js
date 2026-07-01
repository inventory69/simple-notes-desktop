import DOMPurify from 'dompurify';
import { marked } from 'marked';
import changelogMd from '../../CHANGELOG.md?raw';
import shortBlurb from '../../changelog-short.txt?raw';
import * as tauri from '../services/tauri.js';
import { parseChangelog, shouldShowChangelogTeaser } from '../utils/changelogParser.js';

const LAST_SEEN_KEY = 'changelogLastSeenVersion';

/**
 * "What's new" post-update teaser + full changelog view.
 * Desktop port of the Android UpdateChangelogSheet/ChangelogScreen pair — there's no
 * Fastlane pipeline here, so the teaser text comes from a single manually-maintained
 * changelog-short.txt (current release only) instead of per-version files.
 */
export class ChangelogDialog {
  constructor() {
    this.teaserEl = document.getElementById('changelog-teaser');
    this.teaserTextEl = document.getElementById('changelog-teaser-text');
    this.teaserViewBtn = document.getElementById('changelog-teaser-view-btn');
    this.teaserDismissBtn = document.getElementById('changelog-teaser-dismiss');

    this.dialog = document.getElementById('changelog-dialog');
    this.closeBtn = document.getElementById('changelog-dialog-close-btn');
    this.cardsContainer = document.getElementById('changelog-cards');

    this._versions = parseChangelog(changelogMd);
    this._rendered = false;

    this.teaserViewBtn.addEventListener('click', () => {
      this.hideTeaser();
      this.showFull();
    });
    this.teaserDismissBtn.addEventListener('click', () => this.hideTeaser());
    this.closeBtn.addEventListener('click', () => this.hideFull());
  }

  async maybeShowTeaser() {
    let current;
    try {
      current = await tauri.getAppVersion();
    } catch (_e) {
      return;
    }
    const lastSeen = localStorage.getItem(LAST_SEEN_KEY);
    if (!shouldShowChangelogTeaser(current, lastSeen)) return;

    // The full changelog always has more detail than this short list (sub-bullets, other
    // categories, full history) — hardcode the hint rather than counting bullets to compare.
    this.teaserTextEl.textContent = `${shortBlurb.trim()}\n…and more`;
    this.teaserEl.classList.remove('hidden');
    // Marked as seen at show-time (not dismiss-time): simplest option, and avoids the
    // teaser re-appearing on every relaunch if the user closes the app without dismissing it.
    localStorage.setItem(LAST_SEEN_KEY, current);
  }

  hideTeaser() {
    this.teaserEl.classList.add('hidden');
  }

  showFull() {
    if (!this._rendered) {
      this._renderCards();
      this._rendered = true;
    }
    this.dialog.classList.remove('hidden');
  }

  hideFull() {
    this.dialog.classList.add('hidden');
  }

  _renderCards() {
    this.cardsContainer.innerHTML = '';
    for (const { version, date, body } of this._versions) {
      const card = document.createElement('div');
      card.className = 'changelog-card';
      const bodyHtml = DOMPurify.sanitize(marked.parse(body));
      card.innerHTML = `
        <div class="changelog-card-header">
          <span class="changelog-version-badge">v${version}</span>
          <span class="changelog-date">${date}</span>
        </div>
        <div class="changelog-card-body">${bodyHtml}</div>
      `;
      this.cardsContainer.appendChild(card);
    }
  }
}
