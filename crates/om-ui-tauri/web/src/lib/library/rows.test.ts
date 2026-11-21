import { expect, test } from 'vitest';
import type { Column } from './ipc';
import { clock, columnWidths, filledStars, OVERSCAN, prepSummary, rowWindow, shownColumns, sortGlyph } from './rows';

const column = (id: number, width: number, minWindowW = 0): Column => ({
  id,
  title: `c${id}`,
  sortable: true,
  width,
  numeric: false,
  minWindowW,
});

test('the window is the viewport plus overscan, within the list', () => {
  expect(rowWindow(0, 100, 20, 1000)).toEqual({ first: 0, count: 5 + OVERSCAN });
  expect(rowWindow(2000, 100, 20, 1000)).toEqual({ first: 100 - OVERSCAN, count: 5 + 2 * OVERSCAN });
  expect(rowWindow(19_900, 100, 20, 1000)).toEqual({ first: 995 - OVERSCAN, count: 5 + OVERSCAN });
});

test('an empty list or a missing row height asks for nothing', () => {
  expect(rowWindow(0, 100, 20, 0).count).toBe(0);
  expect(rowWindow(0, 100, 0, 50).count).toBe(0);
});

test('a scroll past the end still asks for a valid window', () => {
  const w = rowWindow(50_000, 100, 20, 10);
  expect(w.first).toBeLessThanOrEqual(10);
  expect(w.first + w.count).toBeLessThanOrEqual(10);
});

test('narrow windows hide the columns that ask for width', () => {
  const columns = [column(1, 0), column(7, 120, 1400)];
  expect(shownColumns(columns, 1399).map((c) => c.id)).toEqual([1]);
  expect(shownColumns(columns, 1400).map((c) => c.id)).toEqual([1, 7]);
});

test('flexible columns share what the fixed ones leave, never under the minimum', () => {
  const columns = [column(1, 0), column(2, 0), column(3, 100)];
  expect(columnWidths(columns, 700)).toEqual([300, 300, 100]);
  expect(columnWidths(columns, 200)).toEqual([120, 120, 100]);
});

test('only the sorted column has a glyph, by direction', () => {
  expect(sortGlyph(null, 3)).toBe('');
  expect(sortGlyph([3, true], 3)).toBe('▲');
  expect(sortGlyph([3, false], 3)).toBe('▼');
  expect(sortGlyph([3, false], 4)).toBe('');
});

test('stars are counted from the shell text', () => {
  expect(filledStars('***--')).toBe(3);
  expect(filledStars('-----')).toBe(0);
});

test('the prep summary names what the queue is doing', () => {
  expect(prepSummary(0, 0, 0)).toBe('');
  expect(prepSummary(3, 2, 1)).toBe('Analysing 2 · 3 queued · 1 failed');
});

test('a clock reads minutes and seconds, and hours from an hour', () => {
  expect(clock(0)).toBe('--:--');
  expect(clock(215)).toBe('3:35');
  expect(clock(3725)).toBe('1:02:05');
});
