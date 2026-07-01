import { describe, expect, it } from 'vitest';
import { parseChangelog, shouldShowChangelogTeaser } from '../utils/changelogParser.js';

const SAMPLE = `# Changelog

## [Unreleased]

## [0.9.1] - 2026-06-30

### Fixed

- Fixed a thing

## [0.9.0] - 2026-06-30

### Added

- Added a thing
`;

describe('parseChangelog', () => {
  it('splits into one entry per version header', () => {
    const versions = parseChangelog(SAMPLE);
    expect(versions.map((v) => v.version)).toEqual(['0.9.1', '0.9.0']);
  });

  it('extracts version and date from the header', () => {
    const [first] = parseChangelog(SAMPLE);
    expect(first).toMatchObject({ version: '0.9.1', date: '2026-06-30' });
  });

  it('skips empty sections like [Unreleased]', () => {
    const versions = parseChangelog(SAMPLE);
    expect(versions.some((v) => v.version === 'Unreleased')).toBe(false);
  });

  it('keeps the section body as raw markdown', () => {
    const [first] = parseChangelog(SAMPLE);
    expect(first.body).toContain('Fixed a thing');
  });
});

describe('shouldShowChangelogTeaser', () => {
  it('shows when last-seen is null (fresh install)', () => {
    expect(shouldShowChangelogTeaser('0.9.1', null)).toBe(true);
  });

  it('shows when version differs', () => {
    expect(shouldShowChangelogTeaser('0.9.1', '0.9.0')).toBe(true);
  });

  it('does not show when version matches', () => {
    expect(shouldShowChangelogTeaser('0.9.1', '0.9.1')).toBe(false);
  });
});
