/**
 * Folder name validation and sanitization.
 * Port of Android's FolderNameValidator.kt for cross-app parity.
 */

const MAX_LENGTH = 64;
const FORBIDDEN_CHARS = /[/\\:*?"<>|]/;
const hasControlChar = (s) => [...s].some((c) => c.charCodeAt(0) < 0x20);

/**
 * Validate a folder name.
 * Rules (matching Android FolderNameValidator.kt):
 *  - Non-empty after trim
 *  - Max 64 chars
 *  - Not "." or ".."
 *  - No forbidden filesystem chars: / \ : * ? " < > |
 *  - No control characters (code < 0x20)
 * @param {string} name
 * @returns {boolean}
 */
export function isValid(name) {
  const trimmed = name.trim();
  if (!trimmed || trimmed.length > MAX_LENGTH) return false;
  if (trimmed === '.' || trimmed === '..') return false;
  if (FORBIDDEN_CHARS.test(trimmed)) return false;
  if (hasControlChar(trimmed)) return false;
  return true;
}

/**
 * Sanitize a raw directory name (e.g. from a WebDAV href segment).
 * Trims, URL-decodes, strips forbidden chars, truncates to 64 chars.
 * Returns null if nothing valid remains.
 * @param {string} raw
 * @returns {string|null}
 */
export function sanitize(raw) {
  if (!raw) return null;

  let decoded;
  try {
    decoded = decodeURIComponent(raw);
  } catch {
    decoded = raw;
  }

  const trimmed = decoded.trim().replace(/^\/+|\/+$/g, '');
  if (!trimmed) return null;

  const cleaned = trimmed
    .split('')
    .filter((c) => !FORBIDDEN_CHARS.test(c) && c.charCodeAt(0) >= 0x20)
    .join('')
    .slice(0, MAX_LENGTH);

  return cleaned || null;
}
