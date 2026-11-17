import type { ControlSnap } from '../ipc';

/**
 * `value` with a knob's detent pull applied (om-ui `control::snap_with_bypass`): within `width` of
 * the detent, inclusive, it becomes the detent. Shift bypasses it; no snap leaves it alone.
 */
export function snapToDetent(value: number, snap: ControlSnap | null, shift: boolean): number {
  if (shift || snap === null) return value;
  return Math.abs(value - snap.at) <= snap.width ? snap.at : value;
}

// GPUI's drag treats anything closer than this to the pressed value as not moved.
const UNMOVED = 1e-9;

/**
 * A dragged knob's value: the pull applies only once the value has moved off `anchor`, the value
 * the press found, as in GPUI, so a press alone never moves a knob. `shift` is Shift at the press.
 */
export function dragSnap(value: number, anchor: number, snap: ControlSnap | null, shift: boolean): number {
  return Math.abs(value - anchor) > UNMOVED ? snapToDetent(value, snap, shift) : value;
}
