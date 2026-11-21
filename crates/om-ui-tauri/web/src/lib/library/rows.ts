import type { Column, LibraryView } from './ipc';

/** Rows drawn beyond each edge of the viewport, so a scroll shows rows at once. */
export const OVERSCAN = 6;

/** The rows to ask for: those in the viewport and `OVERSCAN` beyond, within `total`. */
export function rowWindow(scrollTop: number, viewport: number, rowHeight: number, total: number): { first: number; count: number } {
  if (rowHeight <= 0 || total <= 0) return { first: 0, count: 0 };
  const top = Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN);
  const bottom = Math.min(total, Math.ceil((scrollTop + viewport) / rowHeight) + OVERSCAN);
  return { first: Math.min(top, total), count: Math.max(0, bottom - top) };
}

/** The columns a window `width` px wide shows. */
export function shownColumns(columns: readonly Column[], width: number): Column[] {
  return columns.filter((c) => width >= c.minWindowW);
}

/**
 * Each shown column's width in px: fixed ones as given, flexible ones (width 0) sharing what is
 * left of `listWidth`, none narrower than `minFlex`.
 */
export function columnWidths(columns: readonly Column[], listWidth: number, minFlex = 120): number[] {
  const fixed = columns.reduce((sum, c) => sum + c.width, 0);
  const flexible = columns.filter((c) => c.width === 0).length;
  const share = flexible === 0 ? 0 : Math.max(minFlex, (listWidth - fixed) / flexible);
  return columns.map((c) => (c.width === 0 ? share : c.width));
}

/** The glyph on a column header: an arrow for the sorted column, else none. */
export function sortGlyph(sort: LibraryView['sort'], column: number): '' | '▲' | '▼' {
  if (!sort || sort[0] !== column) return '';
  return sort[1] ? '▲' : '▼';
}

/** The `aria-sort` of a column header. */
export function ariaSort(sort: LibraryView['sort'], column: number): 'none' | 'ascending' | 'descending' {
  if (!sort || sort[0] !== column) return 'none';
  return sort[1] ? 'ascending' : 'descending';
}

/** A rating from the shell's `***--` as a count of filled stars. */
export function filledStars(cell: string): number {
  return Array.from(cell).filter((c) => c === '*').length;
}

/** The deck letter: A, B, C, D. */
export const deckLetter = (deck: number): string => String.fromCharCode(65 + (deck % 26));

/** The analysis line under the table: what the prep queue is doing, in words; empty when idle. */
export function prepSummary(queued: number, running: number, failed: number): string {
  const parts: string[] = [];
  if (running > 0) parts.push(`Analysing ${running}`);
  if (queued > 0) parts.push(`${queued} queued`);
  if (failed > 0) parts.push(`${failed} failed`);
  return parts.join(' · ');
}

/** Seconds as `m:ss`, or `h:mm:ss` from an hour. */
export function clock(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return '--:--';
  const total = Math.round(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, '0');
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${m}:${s}`;
}
