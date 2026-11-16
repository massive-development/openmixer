import type { ControlSpec } from '../ipc';

/** The parts of a control spec its travel needs. */
export type Travel = Pick<ControlSpec, 'min' | 'max' | 'detent'>;

/**
 * Where `value` sits along a control's travel, `0..1`: linear, except that a detent strictly inside
 * the range takes the middle and each side is linear (om-ui-gpui `knob_paint::sweep_fraction`).
 * An empty range is position 0.
 */
export function toPosition(value: number, t: Travel): number {
  if (!(t.max > t.min)) return 0;
  const v = Math.min(Math.max(value, t.min), t.max);
  const d = usableDetent(t);
  if (d === null) return (v - t.min) / (t.max - t.min);
  return v <= d ? (0.5 * (v - t.min)) / (d - t.min) : 0.5 + (0.5 * (v - d)) / (t.max - d);
}

/** The engine value at travel `position`; the inverse of `toPosition`. Positions clamp to `0..1`. */
export function toValue(position: number, t: Travel): number {
  if (!(t.max > t.min)) return t.min;
  const f = Math.min(Math.max(position, 0), 1);
  const d = usableDetent(t);
  if (d === null) return t.min + (t.max - t.min) * f;
  return f <= 0.5 ? t.min + ((d - t.min) * f) / 0.5 : d + ((t.max - d) * (f - 0.5)) / 0.5;
}

function usableDetent(t: Travel): number | null {
  return t.detent !== null && t.detent > t.min && t.detent < t.max ? t.detent : null;
}
