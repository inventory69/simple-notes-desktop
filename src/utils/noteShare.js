/**
 * Pure helpers for "Share" / "Copy as text" / selection-copy: turn a note into Markdown or
 * plain text, list which assets it references, and render Markdown to HTML with images
 * resolved to `data:` URLs (for the rich clipboard) instead of the app-only `snasset:` scheme.
 * No DOM, no Tauri — testable in isolation.
 */
import { Marked } from 'marked';
import { IMAGE_REGEX, imagePlaceholderText, parseImageAlt } from './imageAltTokens.js';
import { tolerantTables } from './markdownTable.js';

/** Matches a `.assets/<name>` image href (mirrors NoteEditor's ASSET_HREF_REGEX). */
const ASSET_HREF_REGEX = /^\.assets\/([A-Za-z0-9][A-Za-z0-9._-]*)$/;

function escapeAttr(str) {
  return String(str).replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/** Same filter (drop blank items) + sort (by `order`) rules as calendarExport's formatContent(),
 *  but rendered as real Markdown checkboxes instead of "✓ "/"• " bullets. */
function noteBody(note) {
  if (note.noteType === 'CHECKLIST' && Array.isArray(note.checklistItems)) {
    return note.checklistItems
      .slice()
      .sort((a, b) => a.order - b.order)
      .filter((item) => item.text.trim() !== '')
      .map((item) => `- [${item.isChecked ? 'x' : ' '}] ${item.text.trim()}`)
      .join('\n');
  }
  return (note.content ?? '').trim();
}

/** `# <title>` + blank line + body. Title heading is omitted for an untitled note. */
export function noteToMarkdown(note) {
  const title = (note.title ?? '').trim();
  const body = noteBody(note);
  return title ? `# ${title}\n\n${body}` : body;
}

/** Same as noteToMarkdown, but images become `[🖼 alt]` placeholders and the title is a plain
 *  first line (no `#`) — for the plaintext clipboard fallback / "Copy as text". */
export function noteToPlainText(note) {
  const title = (note.title ?? '').trim();
  const body = imagePlaceholderText(noteBody(note));
  return title ? `${title}\n\n${body}` : body;
}

/** All distinct `.assets/<name>` references in a content/markdown string. */
export function collectAssetNames(content) {
  const names = [...content.matchAll(IMAGE_REGEX)].map((m) => m[2]);
  return [...new Set(names)];
}

/**
 * Renders Markdown to HTML for the rich clipboard: `.assets/<name>` images resolve to their
 * `data:` URL from `dataUrls` (Map<name, string|null>) at their original position in the text;
 * a missing/null entry degrades to a `[🖼 alt]` text placeholder instead of a broken `<img>`.
 * External image URLs are left to Marked's default renderer.
 */
export function markdownToShareHtml(markdown, dataUrls) {
  const shareMarked = new Marked(tolerantTables).use({
    renderer: {
      image({ href, text }) {
        const name = ASSET_HREF_REGEX.exec(href || '')?.[1];
        if (!name) return false;
        const { cleanAlt } = parseImageAlt(text || '');
        const dataUrl = dataUrls.get(name);
        if (!dataUrl) return cleanAlt ? `[🖼 ${escapeAttr(cleanAlt)}]` : '[🖼]';
        return `<img src="${dataUrl}" alt="${escapeAttr(cleanAlt)}" style="max-width:100%">`;
      },
    },
  });
  return shareMarked.parse(markdown);
}
