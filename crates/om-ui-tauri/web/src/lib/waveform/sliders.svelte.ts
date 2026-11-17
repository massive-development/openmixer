import type { LaneRect } from './view';

/** A lane's place for assistive technology, in whole seconds. */
export interface LaneValue {
  seconds: number;
  total: number;
}

/**
 * What each lane's keyboard slider needs from the lanes: where the lane is (CSS px from the top of
 * the waveforms region), its paced position, and the key handler. The sliders sit in the page's tab
 * order after each deck's overview, apart from the lanes, so the two share this.
 */
export class LaneSliders {
  boxes: LaneRect[] = $state.raw([]);
  /** Written at most once a second per lane, or at once after a key. */
  values: LaneValue[] = $state([]);
  /** The lane whose slider has keyboard focus, which draws the focus ring; -1 for none. */
  focused = $state(-1);
  /** Set by the lanes: acts on a key for lane `deck`; true when the key did something. */
  onKey: ((deck: number, key: string, shift: boolean) => boolean) | null = null;
}
