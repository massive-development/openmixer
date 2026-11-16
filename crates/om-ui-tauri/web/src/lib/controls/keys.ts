/** One arrow press moves a slider this share of its travel. */
export const STEP = 0.01;
/** Shift with an arrow, or a page key, moves it this share. */
export const BIG_STEP = 0.1;

/** The parts of a key event a slider reads. */
export type SliderKeyEvent = Pick<KeyboardEvent, 'key' | 'shiftKey' | 'ctrlKey' | 'altKey' | 'metaKey'>;

/**
 * What a key does to a slider at travel `position` (`0..1`): the new position, `'reset'` for the
 * key that restores the default, or null for a key the slider leaves alone. Up and Right raise the
 * value, Home and End are the ends of the travel.
 */
export function sliderKey(event: SliderKeyEvent, position: number): number | 'reset' | null {
  if (event.ctrlKey || event.altKey || event.metaKey) return null;
  const arrow = event.shiftKey ? BIG_STEP : STEP;
  switch (event.key) {
    case 'ArrowUp':
    case 'ArrowRight':
      return nextMark(position, arrow, 1);
    case 'ArrowDown':
    case 'ArrowLeft':
      return nextMark(position, arrow, -1);
    case 'PageUp':
      return nextMark(position, BIG_STEP, 1);
    case 'PageDown':
      return nextMark(position, BIG_STEP, -1);
    case 'Home':
      return 0;
    case 'End':
      return 1;
    case 'Delete':
    case 'Backspace':
      return 'reset';
    default:
      return null;
  }
}

// Rounding slack of the division, far below one step.
const SLACK = 1e-6;

// The next multiple of `size` in direction `dir`, so a detent in the middle is landed on, not skipped.
function nextMark(position: number, size: number, dir: 1 | -1): number {
  const at = position / size;
  const mark = dir > 0 ? Math.floor(at + SLACK) + 1 : Math.ceil(at - SLACK) - 1;
  return Math.min(Math.max(mark * size, 0), 1);
}
