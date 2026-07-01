const VERSION_HEADER_RE = /^\[(.+?)\]\s*-\s*(.+)$/;

/**
 * Splits raw CHANGELOG.md text (Keep a Changelog format) into per-version sections.
 * Sections with no body content (e.g. an empty "[Unreleased]") are skipped.
 * @param {string} markdown - Raw CHANGELOG.md content
 * @returns {Array<{version: string, date: string, body: string}>}
 */
export function parseChangelog(markdown) {
  const chunks = markdown.split(/^## /m).slice(1);
  const versions = [];
  for (const chunk of chunks) {
    const newlineIdx = chunk.indexOf('\n');
    const headingText = (newlineIdx === -1 ? chunk : chunk.slice(0, newlineIdx)).trim();
    const body = (newlineIdx === -1 ? '' : chunk.slice(newlineIdx + 1)).trim();
    if (!body) continue;
    const match = headingText.match(VERSION_HEADER_RE);
    versions.push({
      version: match ? match[1] : headingText,
      date: match ? match[2].trim() : '',
      body,
    });
  }
  return versions;
}

/**
 * @param {string} currentVersion
 * @param {string|null} lastSeenVersion
 * @returns {boolean} true when the teaser should be shown (includes first-ever launch)
 */
export function shouldShowChangelogTeaser(currentVersion, lastSeenVersion) {
  return !!currentVersion && currentVersion !== lastSeenVersion;
}
