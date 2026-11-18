/**
 * The window's glyphs: Lucide icons (ISC, see LICENSE-LUCIDE beside this file), the same set the
 * GPUI window draws from. Each entry is the inner markup of the Lucide SVG of that name, copied
 * unchanged: a 24 px grid, 2 px stroke, round caps and joins. `Icon.svelte` draws them in
 * `currentColor`. An original glyph added here keeps that grid and stroke.
 */
export const ICONS = {
  /** Lucide `play`: the Play button, beside its word. */
  play:
    '<path d="M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z" />',
  /** Lucide `pause`: the same button while the deck plays. */
  pause:
    '<rect x="14" y="3" width="5" height="18" rx="1" />' +
    '<rect x="5" y="3" width="5" height="18" rx="1" />',
  /** Lucide `refresh-cw`: the Sync button. */
  sync:
    '<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" />' +
    '<path d="M21 3v5h-5" />' +
    '<path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" />' +
    '<path d="M8 16H3v5" />',
  /** Lucide `repeat`: the Loop row. */
  loop:
    '<path d="m17 2 4 4-4 4" />' +
    '<path d="M3 11v-1a4 4 0 0 1 4-4h14" />' +
    '<path d="m7 22-4-4 4-4" />' +
    '<path d="M21 13v1a4 4 0 0 1-4 4H3" />',
  /** Lucide `flag`: a hot cue pad that holds a cue. */
  cue:
    '<path d="M4 22V4a1 1 0 0 1 .4-.8A6 6 0 0 1 8 2c3 0 5 2 7.333 2q2 0 3.067-.8A1 1 0 0 1 20 4v10a1 1 0 0 1-.4.8A6 6 0 0 1 16 16c-3 0-5-2-8-2a6 6 0 0 0-4 1.528" />',
  /** Lucide `folder-open`: a deck's Load button. */
  load:
    '<path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2" />',
} as const;

/** The name of a glyph in `ICONS`. */
export type IconName = keyof typeof ICONS;
