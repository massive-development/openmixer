/**
 * The waveform lanes' geometry, pure: which peak level to sample, how many samples a pixel covers,
 * where frames land on screen and how far bar numbers thin out. Device pixels unless named CSS.
 */

/** One peak level's resolution and length, as `deckInfo` reports it. */
export interface LevelSize {
  samplesPerPoint: number;
  len: number;
}

/** A lane's vertical place in the canvas, in device px from the top. */
export interface LaneRect {
  top: number;
  height: number;
}

/** Shortest and longest view, in seconds of track. */
export const MIN_VISIBLE_SECONDS = 1;
export const MAX_VISIBLE_SECONDS = 64;
/** The view's length at start and after a reset, in seconds. */
export const DEFAULT_VISIBLE_SECONDS = 8;
/** One wheel notch zooms by this factor. */
export const ZOOM_STEP = 1.15;
/** Bars repeat every four beats from beat 0 (`om_ui::waveform::bar_number`). */
export const BEATS_PER_BAR = 4;
/** Beat lines closer than this many device px are left out; bar lines always show. */
export const MIN_BEAT_SPACING_PX = 10;
// `om_ui::waveform::bar_label_step` stops doubling here, so a zero-width bar cannot loop forever.
const MAX_BAR_STEP = 1 << 20;
// Slack under the frame cap's interval, so a callback that lands a little early is not skipped.
const FRAME_SLACK_MS = 2;

/**
 * Index of the coarsest level whose `samplesPerPoint <= samplesPerPixel` (levels finest first);
 * `0` when none qualifies; `-1` for an empty list.
 */
export function pickLevel(levels: readonly LevelSize[], samplesPerPixel: number): number {
  if (levels.length === 0) return -1;
  let best = 0;
  for (let i = 0; i < levels.length; i++) {
    if (levels[i].samplesPerPoint <= samplesPerPixel) best = i;
  }
  return best;
}

/** Samples per device pixel for `visibleSeconds` across `widthPx`; `0` when nothing can be drawn. */
export function viewScale(visibleSeconds: number, sampleRate: number, widthPx: number): number {
  if (!(widthPx > 0) || !(sampleRate > 0)) return 0;
  return (visibleSeconds * sampleRate) / widthPx;
}

/**
 * `om_ui::waveform::bar_label_step`: the smallest power of two `n` with `pixelsPerBar * n >= minPx`,
 * so bar numbers keep `minPx` between them.
 */
export function barStep(pixelsPerBar: number, minPx: number): number {
  const px = Math.max(Math.abs(pixelsPerBar), Number.EPSILON);
  let step = 1;
  while (px * step < minPx && step < MAX_BAR_STEP) step *= 2;
  return step;
}

/** The x of `frame` in a lane `widthPx` wide whose centre (the playhead) shows `centreFrame`. */
export function frameToX(frame: number, centreFrame: number, samplesPerPixel: number, widthPx: number): number {
  return widthPx / 2 + (frame - centreFrame) / samplesPerPixel;
}

/** The first bar (0-based, a multiple of `step`) at or after `leftFrame`; never before bar 0. */
export function firstLabelledBar(leftFrame: number, firstBeatFrame: number, framesPerBar: number, step: number): number {
  const bar = Math.ceil((leftFrame - firstBeatFrame) / framesPerBar / step) * step;
  return Math.max(0, bar);
}

/** `visibleSeconds` after one wheel event: longer for a downward scroll, inside 1..64 s. */
export function zoomBy(visibleSeconds: number, deltaY: number): number {
  const next = visibleSeconds * ZOOM_STEP ** Math.sign(deltaY);
  return Math.min(Math.max(next, MIN_VISIBLE_SECONDS), MAX_VISIBLE_SECONDS);
}

/** Whether an animation callback at `now` may draw under `fpsCap` (`0` is uncapped). */
export function shouldDraw(now: number, lastDrawn: number, fpsCap: number): boolean {
  return fpsCap <= 0 || now - lastDrawn >= 1000 / fpsCap - FRAME_SLACK_MS;
}

/** The deck seek control's value for `frame`: a fraction of the track (`DeckParam::Seek`). */
export function seekValue(frame: number, length: number): number {
  return length > 0 ? Math.min(Math.max(frame / length, 0), 1) : 0;
}

/** The frame under the playhead after dragging `dxPx` device px right from `startFrame`. */
export function scrubFrame(startFrame: number, dxPx: number, samplesPerPixel: number, length: number): number {
  const frame = startFrame - dxPx * samplesPerPixel;
  return Math.min(Math.max(frame, 0), Math.max(0, length - 1));
}

/** Lane `index` of `count` sharing `heightPx`, with `separatorPx` between neighbours. */
export function laneRect(index: number, count: number, heightPx: number, separatorPx: number): LaneRect {
  if (count <= 0) return { top: 0, height: 0 };
  const height = Math.max(0, Math.floor((heightPx - (count - 1) * separatorPx) / count));
  return { top: index * (height + separatorPx), height };
}
