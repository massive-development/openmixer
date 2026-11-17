import { scrubFrame } from './view';

// The engine has applied a seek once this many snapshots arrived after it was sent.
const SNAPSHOTS_TO_SETTLE = 2;

/**
 * One lane's drag-to-scrub: the frame held on screen while the engine catches up, and the seek
 * still to send. Positions in source frames, x in device px.
 */
export class Scrub {
  /** The frame shown under the playhead instead of the deck's own; null when not held. */
  held: number | null = null;
  private startFrame = 0;
  private startX = 0;
  private dragging = false;
  private pending = false;
  private sinceSend = 0;

  /** A press at `x` over the lane while it shows `frame`. */
  begin(frame: number, x: number): void {
    this.startFrame = frame;
    this.startX = x;
    this.held = frame;
    this.dragging = true;
    this.pending = false;
    this.sinceSend = 0;
  }

  /** The pointer moved to `x`; the track follows it under the fixed playhead. */
  move(x: number, samplesPerPixel: number, length: number): void {
    if (!this.dragging) return;
    this.held = scrubFrame(this.startFrame, x - this.startX, samplesPerPixel, length);
    this.pending = true;
  }

  /** A key moved the playhead to `frame`: held and sent as a drag's last move is. */
  jump(frame: number): void {
    this.held = frame;
    this.dragging = false;
    this.pending = true;
    this.sinceSend = 0;
  }

  /** The press ended; the hold stays until the engine reports the last seek. */
  end(): void {
    this.dragging = false;
  }

  /** The frame to seek to now, at most once per change; null when nothing is new. */
  takeSend(): number | null {
    if (!this.pending || this.held === null) return null;
    this.pending = false;
    this.sinceSend = 0;
    return this.held;
  }

  /** A snapshot arrived; the hold ends once two follow the last send of a finished drag. */
  onSnapshot(): void {
    if (this.held === null) return;
    this.sinceSend++;
    if (!this.dragging && !this.pending && this.sinceSend >= SNAPSHOTS_TO_SETTLE) this.held = null;
  }
}
