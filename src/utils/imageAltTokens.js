/**
 * Image attachments v2: size/alignment live as Obsidian-style pipe tokens in the alt text,
 * e.g. `![Sunset|25%|right](.assets/x.webp)`. Port of Android `ImageAltTokens.kt` — keeps the
 * alt group `[^\]]*`, so every existing asset regex (GC, sync, extract) keeps working unchanged.
 */

export const DEFAULT_SIZE_PERCENT = 50;
export const DEFAULT_ALIGN = 'center';
const MIN_SIZE_PERCENT = 1;
const MAX_SIZE_PERCENT = 100;
const SIZE_TOKEN_REGEX = /^\d{1,3}%$/;
const ALIGN_VALUES = new Set(['left', 'center', 'right', 'inline']);

export const SIZE_PRESETS = [
  { label: 'S', value: 25 },
  { label: 'M', value: 50 },
  { label: 'L', value: 75 },
  { label: 'XL', value: 100 },
];

/** Matches `![alt](.assets/<name>)` image references (mirrors the Rust EXTRACT_REGEX). */
export const IMAGE_REGEX = /!\[([^\]]*)]\(\.assets\/([A-Za-z0-9][A-Za-z0-9._-]*)\)/g;

/** Splits alt on `|`; pulls out size/align tokens (last wins), rest stays clean alt. */
export function parseImageAlt(rawAlt) {
  if (!rawAlt) return { cleanAlt: '', sizePercent: DEFAULT_SIZE_PERCENT, align: DEFAULT_ALIGN };

  let sizePercent = DEFAULT_SIZE_PERCENT;
  let align = DEFAULT_ALIGN;
  const cleanSegments = [];

  for (const segment of rawAlt.split('|')) {
    const lower = segment.toLowerCase();
    if (SIZE_TOKEN_REGEX.test(segment)) {
      const value = Number.parseInt(segment.slice(0, -1), 10);
      sizePercent = Math.min(MAX_SIZE_PERCENT, Math.max(MIN_SIZE_PERCENT, value));
    } else if (ALIGN_VALUES.has(lower)) {
      align = lower;
    } else {
      cleanSegments.push(segment);
    }
  }

  return { cleanAlt: cleanSegments.join('|'), sizePercent, align };
}

/** Inverse of parseImageAlt(). Leaves out default tokens (50%, center). */
export function buildImageAlt(cleanAlt, sizePercent, align) {
  const tokens = [];
  if (sizePercent !== DEFAULT_SIZE_PERCENT) tokens.push(`${sizePercent}%`);
  if (align !== DEFAULT_ALIGN) tokens.push(align);
  return tokens.length === 0 ? cleanAlt : [cleanAlt, ...tokens].join('|');
}

/**
 * Computes the text replacement for the `ordinal`-th image link (index in
 * `IMAGE_REGEX`-matches over the whole text) with a new size/alignment/alt.
 * Asset-name mismatch or out-of-range ordinal → `null` (silent no-op instead of a wrong link,
 * e.g. if the text changed between opening the menu and picking an option).
 */
export function computeImageRewrite(content, ordinal, assetName, sizePercent, align, cleanAlt) {
  const matches = [...content.matchAll(IMAGE_REGEX)];
  const match = matches[ordinal];
  if (!match || match[2] !== assetName) return null;

  const resolvedCleanAlt = cleanAlt ?? parseImageAlt(match[1]).cleanAlt;
  const newAlt = buildImageAlt(resolvedCleanAlt, sizePercent, align);
  return {
    from: match.index,
    to: match.index + match[0].length,
    insert: `![${newAlt}](.assets/${assetName})`,
  };
}

/** Replaces every image tag with `🖼 <cleanAlt>` — for the notes-list preview line. */
export function imagePreviewText(content) {
  return content.replace(IMAGE_REGEX, (_match, alt) => `🖼 ${parseImageAlt(alt).cleanAlt}`.trim());
}
