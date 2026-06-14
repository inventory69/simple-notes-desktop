import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';

const mdHighlightStyle = HighlightStyle.define([
  { tag: t.heading1, fontWeight: 'bold', fontSize: '1.5em' },
  { tag: t.heading2, fontWeight: 'bold', fontSize: '1.3em' },
  { tag: t.heading3, fontWeight: 'bold', fontSize: '1.15em' },
  { tag: t.heading, fontWeight: 'bold' },
  { tag: t.strong, fontWeight: 'bold' },
  { tag: t.emphasis, fontStyle: 'italic' },
  { tag: t.strikethrough, textDecoration: 'line-through' },
  { tag: t.monospace, fontFamily: 'monospace', background: 'var(--cm-code-bg)' },
  { tag: t.link, textDecoration: 'underline' },
  { tag: t.url, textDecoration: 'underline' },
  // Dim syntax markers (**, __, ~~, `, #, >, list bullets) — matches Android markerColor
  { tag: t.processingInstruction, color: 'var(--cm-marker)' },
  { tag: t.contentSeparator, color: 'var(--cm-marker)', fontWeight: 'bold' },
]);

export const markdownHighlightExtensions = [syntaxHighlighting(mdHighlightStyle)];
