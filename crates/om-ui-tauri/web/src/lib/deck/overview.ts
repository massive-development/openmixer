import type { PeakLevel } from '../ipc';

/** Bytes per peak point: body, low, mid and high heights (`om_ui_bridge::BYTES_PER_POINT`). */
const BYTES_PER_POINT = 4;

/** Each overview column's body height and high-band height, `0..1` of the half-height. */
export interface OverviewColumns {
  body: Float32Array;
  high: Float32Array;
}

/** The coarsest level that still has a point for every one of `widthPx` columns; -1 for none. */
export function overviewLevel(levels: readonly PeakLevel[], numSamples: number, widthPx: number): number {
  if (levels.length === 0 || !(widthPx > 0)) return -1;
  const perColumn = numSamples / widthPx;
  let best = 0;
  for (let i = 0; i < levels.length; i++) {
    if (levels[i].samplesPerPoint <= perColumn) best = i;
  }
  return best;
}

/** First point of level `index` in the packed bytes (levels are stored one after another). */
export function levelOffset(levels: readonly PeakLevel[], index: number): number {
  let at = 0;
  for (let i = 0; i < index; i++) at += levels[i].len;
  return at;
}

/**
 * The loudest point under each of `widthPx` columns of a `numSamples` track, read from `count`
 * points starting at point `first`. The high band is a share of its body (GPUI draws body x high).
 */
export function overviewColumns(
  bytes: Uint8Array,
  first: number,
  count: number,
  samplesPerPoint: number,
  numSamples: number,
  widthPx: number,
): OverviewColumns {
  const width = Math.max(0, Math.floor(widthPx));
  const cols = { body: new Float32Array(width), high: new Float32Array(width) };
  if (count <= 0 || !(samplesPerPoint > 0) || !(numSamples > 0)) return cols;
  const pointsPerColumn = numSamples / samplesPerPoint / width;
  for (let x = 0; x < width; x++) {
    const a = Math.floor(x * pointsPerColumn);
    const b = Math.min(count, Math.max(a + 1, Math.floor((x + 1) * pointsPerColumn)));
    column(cols, x, bytes, first, a, b);
  }
  return cols;
}

function column(cols: OverviewColumns, x: number, bytes: Uint8Array, first: number, a: number, b: number): void {
  for (let i = a; i < b; i++) {
    const at = (first + i) * BYTES_PER_POINT;
    if (at + 3 >= bytes.length) return;
    const body = bytes[at] / 255;
    cols.body[x] = Math.max(cols.body[x], body);
    cols.high[x] = Math.max(cols.high[x], body * (bytes[at + 3] / 255));
  }
}
