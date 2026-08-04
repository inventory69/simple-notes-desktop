import { describe, expect, it } from 'vitest';
import {
  buildImageAlt,
  computeImageRewrite,
  imagePlaceholderText,
  imagePreviewText,
  parseImageAlt,
} from '../utils/imageAltTokens.js';

describe('parseImageAlt', () => {
  it('returns defaults for empty alt', () => {
    expect(parseImageAlt('')).toEqual({ cleanAlt: '', sizePercent: 50, align: 'center' });
  });

  it('extracts a size token', () => {
    expect(parseImageAlt('Sunset|25%')).toEqual({ cleanAlt: 'Sunset', sizePercent: 25, align: 'center' });
  });

  it('extracts an align token', () => {
    expect(parseImageAlt('Sunset|right')).toEqual({ cleanAlt: 'Sunset', sizePercent: 50, align: 'right' });
  });

  it('extracts both size and align, any order', () => {
    expect(parseImageAlt('Sunset|right|25%')).toEqual({ cleanAlt: 'Sunset', sizePercent: 25, align: 'right' });
  });

  it('is case-insensitive for align tokens', () => {
    expect(parseImageAlt('x|RIGHT')).toEqual({ cleanAlt: 'x', sizePercent: 50, align: 'right' });
  });

  it('clamps size to 1-100', () => {
    expect(parseImageAlt('x|0%').sizePercent).toBe(1);
    expect(parseImageAlt('x|500%').sizePercent).toBe(100);
  });

  it('last-wins on duplicate tokens', () => {
    expect(parseImageAlt('x|25%|75%').sizePercent).toBe(75);
    expect(parseImageAlt('x|left|right').align).toBe('right');
  });

  it('leaves non-token segments in clean alt, rejoined with |', () => {
    expect(parseImageAlt('a|b|50%').cleanAlt).toBe('a|b');
  });
});

describe('buildImageAlt', () => {
  it('omits default tokens (50%, center)', () => {
    expect(buildImageAlt('Sunset', 50, 'center')).toBe('Sunset');
  });

  it('adds a non-default size token', () => {
    expect(buildImageAlt('Sunset', 25, 'center')).toBe('Sunset|25%');
  });

  it('adds a non-default align token', () => {
    expect(buildImageAlt('Sunset', 50, 'right')).toBe('Sunset|right');
  });

  it('adds both when both are non-default', () => {
    expect(buildImageAlt('Sunset', 25, 'right')).toBe('Sunset|25%|right');
  });

  it('round-trips through parseImageAlt', () => {
    const built = buildImageAlt('My photo', 75, 'left');
    expect(parseImageAlt(built)).toEqual({ cleanAlt: 'My photo', sizePercent: 75, align: 'left' });
  });
});

describe('computeImageRewrite', () => {
  it('rewrites the matching ordinal', () => {
    const content = 'A ![a](.assets/one.webp) B ![b](.assets/two.webp)';
    const rewrite = computeImageRewrite(content, 1, 'two.webp', 25, 'right', 'b');
    expect(rewrite).not.toBeNull();
    const result = content.slice(0, rewrite.from) + rewrite.insert + content.slice(rewrite.to);
    expect(result).toBe('A ![a](.assets/one.webp) B ![b|25%|right](.assets/two.webp)');
  });

  it('hits the right ordinal among identical links', () => {
    const content = '![x](.assets/dup.webp) and ![x](.assets/dup.webp)';
    const rewrite = computeImageRewrite(content, 1, 'dup.webp', 100, 'center', 'x');
    expect(rewrite.from).toBeGreaterThan(content.indexOf('and'));
  });

  it('returns null on asset-name mismatch', () => {
    const content = '![a](.assets/one.webp)';
    expect(computeImageRewrite(content, 0, 'wrong.webp', 50, 'center', 'a')).toBeNull();
  });

  it('returns null when ordinal is out of range', () => {
    const content = '![a](.assets/one.webp)';
    expect(computeImageRewrite(content, 5, 'one.webp', 50, 'center', 'a')).toBeNull();
  });
});

describe('imagePreviewText', () => {
  it('replaces an image tag with an emoji + clean alt', () => {
    expect(imagePreviewText('See ![Sunset|25%|right](.assets/x.webp) here')).toBe('See 🖼 Sunset here');
  });

  it('handles multiple images', () => {
    expect(imagePreviewText('![a](.assets/x.webp)![b](.assets/y.webp)')).toBe('🖼 a🖼 b');
  });

  it('leaves plain text untouched', () => {
    expect(imagePreviewText('no images here')).toBe('no images here');
  });
});

describe('imagePlaceholderText', () => {
  it('replaces an image with alt text', () => {
    expect(imagePlaceholderText('before ![Sunset](.assets/abc123.webp) after')).toBe('before [🖼 Sunset] after');
  });

  it('replaces an image without alt text', () => {
    expect(imagePlaceholderText('![](.assets/abc123.webp)')).toBe('[🖼]');
  });

  it('strips size/align tokens from the alt text', () => {
    expect(imagePlaceholderText('![Sunset|25%|right](.assets/abc123.webp)')).toBe('[🖼 Sunset]');
  });

  it('collapses runs of 3+ newlines left behind by a removed image line', () => {
    const content = 'line one\n\n\n![alt](.assets/abc123.webp)\n\n\n\nline two';
    expect(imagePlaceholderText(content)).toBe('line one\n\n[🖼 alt]\n\nline two');
  });

  it('leaves plain text untouched', () => {
    expect(imagePlaceholderText('no images here')).toBe('no images here');
  });
});
