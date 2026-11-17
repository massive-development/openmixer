/** `NN fps · N.NN ms`: frames drawn per second over `elapsedMs`, and the mean work per drawn frame. */
export function hudText(frames: number, workMs: number, elapsedMs: number): string {
  const fps = elapsedMs > 0 ? (frames * 1000) / elapsedMs : 0;
  const mean = frames > 0 ? workMs / frames : 0;
  return `${fps.toFixed(0)} fps · ${mean.toFixed(2)} ms`;
}

/**
 * The frame counter's window: counts drawn frames and their work, and gives the read-out once every
 * `periodMs`. The ticker makes one only when the run shows the counter.
 */
export class HudWindow {
  private frames = 0;
  private workMs = 0;
  private start = -1;

  constructor(private readonly periodMs: number) {}

  /** One frame at `now`; the read-out when a period has passed, else null. */
  frame(now: number, drawn: boolean, workMs: number): string | null {
    if (drawn) {
      this.frames++;
      this.workMs += workMs;
    }
    if (this.start < 0) this.start = now;
    if (now - this.start < this.periodMs) return null;
    const text = hudText(this.frames, this.workMs, now - this.start);
    this.start = now;
    this.frames = 0;
    this.workMs = 0;
    return text;
  }
}
