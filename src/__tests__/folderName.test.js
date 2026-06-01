import { describe, expect, it } from 'vitest';
import { isValid, sanitize } from '../utils/folderName.js';

describe('folderName.isValid', () => {
  it('accepts normal names', () => {
    expect(isValid('Work')).toBe(true);
    expect(isValid('My Project')).toBe(true);
    expect(isValid('notes-2024')).toBe(true);
    expect(isValid('a'.repeat(64))).toBe(true);
  });

  it('rejects empty or whitespace-only', () => {
    expect(isValid('')).toBe(false);
    expect(isValid('   ')).toBe(false);
  });

  it('rejects "." and ".."', () => {
    expect(isValid('.')).toBe(false);
    expect(isValid('..')).toBe(false);
  });

  it('rejects names longer than 64 chars', () => {
    expect(isValid('a'.repeat(65))).toBe(false);
  });

  it('rejects forbidden filesystem chars', () => {
    for (const ch of ['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
      expect(isValid(`test${ch}name`)).toBe(false);
    }
  });

  it('rejects control characters in the middle of a name', () => {
    expect(isValid('test\x00name')).toBe(false);
    expect(isValid('test\x1fname')).toBe(false);
    expect(isValid('test\nname')).toBe(false);
    expect(isValid('test\tname')).toBe(false);
  });
});

describe('folderName.sanitize', () => {
  it('returns normal names unchanged', () => {
    expect(sanitize('Work')).toBe('Work');
  });

  it('URL-decodes percent-encoded segments', () => {
    expect(sanitize('My%20Notes')).toBe('My Notes');
  });

  it('strips leading/trailing slashes', () => {
    expect(sanitize('/Work/')).toBe('Work');
  });

  it('strips forbidden chars', () => {
    expect(sanitize('test:name')).toBe('testname');
    expect(sanitize('test/name')).toBe('testname');
  });

  it('returns null for empty input', () => {
    expect(sanitize('')).toBeNull();
    expect(sanitize(null)).toBeNull();
    expect(sanitize('/')).toBeNull();
    expect(sanitize(':*?')).toBeNull();
  });

  it('truncates to 64 chars', () => {
    const result = sanitize('a'.repeat(80));
    expect(result.length).toBe(64);
  });
});
