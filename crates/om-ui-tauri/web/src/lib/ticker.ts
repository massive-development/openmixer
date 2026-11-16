import type { FrameRecorder } from './bench';
import { hudText } from './hud';

/** One frame's work; a woken task returns true to run again next frame. */
export type FrameTask = (now: number) => boolean;

// The HUD refreshes twice a second.
const HUD_PERIOD_MS = 500;

/**
 * The one `requestAnimationFrame` loop of the window. Persistent tasks (the waveform lanes) run
 * every frame; woken tasks run until they return false, so idle controls cost nothing. Tasks call
 * `drew()` when they changed the page; the HUD counts those frames and the work they took.
 */
class Ticker {
  private readonly always = new Set<FrameTask>();
  private readonly woken = new Set<FrameTask>();
  private raf = 0;
  private drawn = false;
  private frames = 0;
  private workMs = 0;
  private windowStart = -1;
  private hud: ((text: string) => void) | null = null;
  private lastDrawnAt = -1;
  private recorder: FrameRecorder | null = null;

  /** Runs `task` every frame until the returned function is called. */
  add(task: FrameTask): () => void {
    this.always.add(task);
    this.start();
    return () => this.always.delete(task);
  }

  /** Runs `task` from the next frame until it returns false; waking a running task is a no-op. */
  wake(task: FrameTask): void {
    this.woken.add(task);
    this.start();
  }

  /** The current task changed the page this frame. */
  drew(): void {
    this.drawn = true;
  }

  /** Where the HUD text goes. */
  onHud(write: (text: string) => void): void {
    this.hud = write;
  }

  /** Records every drawn frame into `recorder` (a `--ui-bench` run) until called with null. */
  record(recorder: FrameRecorder | null): void {
    this.recorder = recorder;
  }

  private start(): void {
    if (this.raf === 0) this.raf = requestAnimationFrame(this.frame);
  }

  private readonly frame = (now: number): void => {
    this.raf = 0;
    const t0 = performance.now();
    this.drawn = false;
    for (const task of this.always) task(now);
    for (const task of this.woken) if (!task(now)) this.woken.delete(task);
    this.count(now, performance.now() - t0);
    if (this.always.size > 0 || this.woken.size > 0) this.start();
  };

  private count(now: number, workMs: number): void {
    if (this.drawn) {
      this.frames++;
      this.workMs += workMs;
      // The first drawn frame has no interval yet.
      if (this.recorder && this.lastDrawnAt >= 0) this.recorder.push(now - this.lastDrawnAt, workMs);
      this.lastDrawnAt = now;
    }
    if (this.windowStart < 0) this.windowStart = now;
    if (now - this.windowStart < HUD_PERIOD_MS) return;
    this.hud?.(hudText(this.frames, this.workMs, now - this.windowStart));
    this.windowStart = now;
    this.frames = 0;
    this.workMs = 0;
  }
}

/** The window's one frame loop. */
export const ticker = new Ticker();
