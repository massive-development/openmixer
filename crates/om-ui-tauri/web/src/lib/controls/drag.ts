/** Pointer travel for a knob's whole sweep, in CSS px. */
export const KNOB_TRAVEL_PX = 160;

// The engine has applied a value once this many snapshots arrived after the drag ended.
const SNAPSHOTS_TO_SETTLE = 2;

/** Fine mode divides a knob drag's travel by this (GPUI's `knob_paint::FINE_DIVISOR`). */
export const FINE_DIVISOR = 10;

/**
 * A knob position after the pointer moved `dyPx` down (negative is up): up turns it clockwise.
 * `fine` (Shift held at the press, as GPUI reads it) moves it a tenth as far.
 */
export function knobDelta(value: number, dyPx: number, fine = false, rangePx = KNOB_TRAVEL_PX): number {
  return clamp01(value - dyPx / rangePx / (fine ? FINE_DIVISOR : 1));
}

/**
 * A fader position `0..1` for a pointer at `pointerPx` on a track starting at `trackStartPx`;
 * `inverted` counts from the track's far end (a vertical fader's top is 1). `0` for a zero-length track.
 */
export function faderValue(pointerPx: number, trackStartPx: number, trackLengthPx: number, inverted: boolean): number {
  if (!(trackLengthPx > 0)) return 0;
  const along = clamp01((pointerPx - trackStartPx) / trackLengthPx);
  return inverted ? 1 - along : along;
}

/**
 * A control's local value while the pointer holds it: it shows instead of the snapshot's from the
 * press until two snapshots follow the release, so the control does not flick back to a stale value.
 */
export class Held {
  private value: number | null = null;
  private dragging = false;
  private since = 0;

  begin(value: number): void {
    this.value = value;
    this.dragging = true;
    this.since = 0;
  }

  set(value: number): void {
    this.value = value;
    this.since = 0;
  }

  end(): void {
    this.dragging = false;
    this.since = 0;
  }

  get active(): boolean {
    return this.dragging;
  }

  /** A snapshot arrived; after a release the hold ends once enough have. */
  onSnapshot(): void {
    if (this.value === null || this.dragging) return;
    if (++this.since >= SNAPSHOTS_TO_SETTLE) this.value = null;
  }

  /** The value to show, given the snapshot's. */
  shown(snapshotValue: number): number {
    return this.value ?? snapshotValue;
  }
}

/** CSS px a press may wander and still be a click; further, it is a drag. */
export const CLICK_SLOP_PX = 3;

/**
 * How far the pointer went during the last press: a double-click whose second press dragged is a
 * drag, so it must not reset the control when the browser reports the double-click at its release.
 */
export class PressTravel {
  private x = 0;
  private y = 0;
  private far = false;

  begin(x: number, y: number): void {
    this.x = x;
    this.y = y;
    this.far = false;
  }

  move(x: number, y: number): void {
    if (Math.hypot(x - this.x, y - this.y) > CLICK_SLOP_PX) this.far = true;
  }

  /** The pointer left the click slop at some point of the last press. */
  get dragged(): boolean {
    return this.far;
  }
}

function clamp01(v: number): number {
  return Math.min(Math.max(v, 0), 1);
}
