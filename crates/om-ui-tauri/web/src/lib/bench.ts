/** Mean, nearest-rank percentiles and maximum of a list of samples (ms). */
export interface Summary {
  n: number;
  mean: number;
  p50: number;
  p95: number;
  p99: number;
  max: number;
}

/** What the shell writes for a `--ui-bench` run (`bench_done`). */
export interface BenchStats {
  benchSeconds: number;
  fpsCap: number;
  /** CSS width, CSS height, device pixel ratio. */
  viewport: [number, number, number];
  frameMs: Summary;
  workMs: Summary;
  /** 1000 / mean frame interval; null with no frames. */
  fps: number | null;
  /** Intervals above twice the median interval (a missed refresh). */
  framesOver2xRefresh: number;
  decksPlaying: number;
}

/** The run's settings and the window's state, which the recorder does not see. */
export interface BenchContext {
  benchSeconds: number;
  fpsCap: number;
  viewport: [number, number, number];
  decksPlaying: number;
}

// Room for a 240 Hz display: the arrays never grow while recording.
const MAX_FPS = 240;

/** Summarises `samples` without changing them; all zero for an empty list. */
export function summarise(samples: ArrayLike<number>): Summary {
  const n = samples.length;
  if (n === 0) return { n: 0, mean: 0, p50: 0, p95: 0, p99: 0, max: 0 };
  const sorted = Float64Array.from(samples).sort();
  let sum = 0;
  for (const s of sorted) sum += s;
  const rank = (p: number): number => sorted[Math.max(0, Math.ceil(p * n) - 1)];
  return { n, mean: sum / n, p50: rank(0.5), p95: rank(0.95), p99: rank(0.99), max: sorted[n - 1] };
}

/** Per drawn frame: the interval since the previous drawn frame and the frame's work, in ms. */
export class FrameRecorder {
  readonly capacity: number;
  private readonly intervals: Float64Array;
  private readonly work: Float64Array;
  private count = 0;
  private lost = 0;

  constructor(seconds: number) {
    this.capacity = Math.max(1, Math.ceil(seconds * MAX_FPS));
    this.intervals = new Float64Array(this.capacity);
    this.work = new Float64Array(this.capacity);
  }

  /** Records one frame; once full, frames are counted as dropped instead. */
  push(intervalMs: number, workMs: number): void {
    if (this.count >= this.capacity) {
      this.lost++;
      return;
    }
    this.intervals[this.count] = intervalMs;
    this.work[this.count] = workMs;
    this.count++;
  }

  get length(): number {
    return this.count;
  }

  get dropped(): number {
    return this.lost;
  }

  frameIntervals(): Float64Array {
    return this.intervals.subarray(0, this.count);
  }

  frameWork(): Float64Array {
    return this.work.subarray(0, this.count);
  }
}

/** The run's report from what `recorder` holds. */
export function benchStats(recorder: FrameRecorder, context: BenchContext): BenchStats {
  const intervals = recorder.frameIntervals();
  const frameMs = summarise(intervals);
  let slow = 0;
  for (const ms of intervals) if (ms > 2 * frameMs.p50) slow++;
  return {
    ...context,
    frameMs,
    workMs: summarise(recorder.frameWork()),
    fps: frameMs.n > 0 && frameMs.mean > 0 ? 1000 / frameMs.mean : null,
    framesOver2xRefresh: slow,
  };
}
