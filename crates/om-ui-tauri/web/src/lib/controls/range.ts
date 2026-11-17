import type { ControlSpec } from '../ipc';

/** The parts of a control spec its travel needs. */
export type Travel = Pick<ControlSpec, 'min' | 'max' | 'detent' | 'law'>;

/** One `[position, value]` point of a knotted law. */
export type Knot = [number, number];

/**
 * Where `value` sits along a control's travel, `0..1`. A knotted law (a GPUI fader taper, sent by
 * the shell) is inverted between its knots; a linear one maps proportionally, except that a detent
 * strictly inside the range takes the middle and each side is linear (om-ui-gpui
 * `knob_paint::sweep_fraction`). An empty range is position 0.
 */
export function toPosition(value: number, t: Travel): number {
  if (t.law.kind === 'knots') return clamp01(knotPosition(t.law.knots, value));
  if (!(t.max > t.min)) return 0;
  const v = Math.min(Math.max(value, t.min), t.max);
  const d = usableDetent(t);
  if (d === null) return (v - t.min) / (t.max - t.min);
  return v <= d ? (0.5 * (v - t.min)) / (d - t.min) : 0.5 + (0.5 * (v - d)) / (t.max - d);
}

/** The engine value at travel `position`; the inverse of `toPosition`. Positions clamp to `0..1`. */
export function toValue(position: number, t: Travel): number {
  const f = clamp01(position);
  if (t.law.kind === 'knots') return knotValue(t.law.knots, f);
  if (!(t.max > t.min)) return t.min;
  const d = usableDetent(t);
  if (d === null) return t.min + (t.max - t.min) * f;
  return f <= 0.5 ? t.min + ((d - t.min) * f) / 0.5 : d + ((t.max - d) * (f - 0.5)) / 0.5;
}

/**
 * The value at `position` on knots with rising positions: linear between the two around it, the
 * end values outside them. The segment found always has a width, so stacked knots never divide by
 * zero. No knots is 0.
 */
export function knotValue(knots: readonly Knot[], position: number): number {
  const i = knots.findIndex(([p]) => p >= position);
  if (i < 0) return knots.at(-1)?.[1] ?? 0;
  if (i === 0 || knots[i][0] === position) return knots[i][1];
  return lerp(knots[i - 1], knots[i], position);
}

/** The position of `value` on knots with rising values: the inverse of `knotValue`, the same way. */
export function knotPosition(knots: readonly Knot[], value: number): number {
  const i = knots.findIndex(([, v]) => v >= value);
  if (i < 0) return knots.at(-1)?.[0] ?? 0;
  if (i === 0 || knots[i][1] === value) return knots[i][0];
  const [p0, v0] = knots[i - 1];
  const [p1, v1] = knots[i];
  return lerp([v0, p0], [v1, p1], value);
}

// Along the segment `a`..`b` at `x`, where a[0] < x <= b[0].
function lerp([x0, y0]: Knot, [x1, y1]: Knot, x: number): number {
  return y0 + ((y1 - y0) * (x - x0)) / (x1 - x0);
}

function usableDetent(t: Travel): number | null {
  return t.detent !== null && t.detent > t.min && t.detent < t.max ? t.detent : null;
}

function clamp01(v: number): number {
  return Math.min(Math.max(v, 0), 1);
}
