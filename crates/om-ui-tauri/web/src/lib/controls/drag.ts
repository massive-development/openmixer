/** Pointer travel for a knob's whole sweep, in CSS px. */
export const KNOB_TRAVEL_PX = 160;

// The engine has applied a value once this many snapshots arrived after the drag ended.
const SNAPSHOTS_TO_SETTLE = 2;

/** A knob position after the pointer moved `dyPx` down (negative is up): up turns it clockwise. */
export function knobDelta(value: number, dyPx: number, rangePx = KNOB_TRAVEL_PX): number {
  return clamp01(value - dyPx / rangePx);
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

function clamp01(v: number): number {
  return Math.min(Math.max(v, 0), 1);
}
