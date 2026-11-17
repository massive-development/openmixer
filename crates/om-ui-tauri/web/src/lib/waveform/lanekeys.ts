import { DEFAULT_VISIBLE_SECONDS, zoomBy } from './view';

/** What a focused lane knows when a key arrives. Frames are source frames. */
export interface LaneKeyState {
  /** The frame under the playhead. */
  frame: number;
  /** The track's length; 0 without a track. */
  length: number;
  sampleRate: number;
  /** The grid's tempo and its beat 0; either null means no grid. */
  bpm: number | null;
  firstBeatFrame: number | null;
  /** The view's length now, in seconds. */
  seconds: number;
}

/** A lane key's effect: seek the deck to a frame, or set the view's length. */
export type LaneAction = { kind: 'seek'; frame: number } | { kind: 'zoom'; seconds: number };

// Without a grid an arrow moves this far, in seconds; Shift moves the long step.
const STEP_SECONDS = 0.5;
const LONG_STEP_SECONDS = 2;
// With a grid an arrow moves one beat; Shift moves a bar's worth.
const LONG_STEP_BEATS = 4;

/**
 * What `key` (with Shift held or not) does to a focused lane: arrows step the playhead (a beat on a
 * grid, else half a second; Shift four beats or 2 s), Home and End go to the track's ends, `+`/`=`
 * and `-` zoom by one wheel notch, `0` resets the zoom. Null for any other key, and for a seek
 * without a track.
 */
export function laneKey(key: string, shift: boolean, s: LaneKeyState): LaneAction | null {
  switch (key) {
    case '+':
    case '=':
      return { kind: 'zoom', seconds: zoomBy(s.seconds, -1) };
    case '-':
      return { kind: 'zoom', seconds: zoomBy(s.seconds, 1) };
    case '0':
      return { kind: 'zoom', seconds: DEFAULT_VISIBLE_SECONDS };
    case 'ArrowLeft':
      return seek(s, s.frame - step(s, shift));
    case 'ArrowRight':
      return seek(s, s.frame + step(s, shift));
    case 'Home':
      return seek(s, 0);
    case 'End':
      return seek(s, s.length);
    default:
      return null;
  }
}

// One arrow's travel in frames.
function step(s: LaneKeyState, shift: boolean): number {
  const bpm = s.bpm ?? 0;
  if (bpm > 0 && s.firstBeatFrame !== null) return (60 / bpm) * s.sampleRate * (shift ? LONG_STEP_BEATS : 1);
  return (shift ? LONG_STEP_SECONDS : STEP_SECONDS) * s.sampleRate;
}

// Clamped as the pointer scrub clamps (`scrubFrame`): the first to the last frame.
function seek(s: LaneKeyState, frame: number): LaneAction | null {
  if (!(s.length > 0) || !(s.sampleRate > 0)) return null;
  return { kind: 'seek', frame: Math.min(Math.max(frame, 0), s.length - 1) };
}
