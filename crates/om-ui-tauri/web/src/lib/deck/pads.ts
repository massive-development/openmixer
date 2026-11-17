import type { Controls } from '../ipc';

/** One loop size button: its control key, length in beats and printed name. */
export interface LoopButton {
  key: string;
  beats: number;
  label: string;
}

/** One hot cue pad: the key that sets or jumps, the key that clears, and its printed number. */
export interface HotcueKeys {
  set: string;
  clear: string;
  label: string;
}

/** Deck `deck`'s loop sizes as the shell lists them (`deck<N>.loop.<beats>`), shortest first. */
export function loopButtons(controls: Controls, deck: number): LoopButton[] {
  const prefix = `deck${deck}.loop.`;
  const sizes = Object.keys(controls)
    .filter((key) => key.startsWith(prefix))
    .map((key) => ({ key, beats: Number(key.slice(prefix.length)), label: controls[key].label }))
    .filter((b) => Number.isFinite(b.beats));
  return sizes.sort((a, b) => a.beats - b.beats);
}

/** Deck `deck`'s hot cue pads, in order, while both of a pad's ids are listed. */
export function hotcueKeys(controls: Controls, deck: number): HotcueKeys[] {
  const pads: HotcueKeys[] = [];
  for (let i = 0; ; i++) {
    const set = `deck${deck}.hotcue${i}`;
    const clear = `${set}.clear`;
    if (!Object.hasOwn(controls, set) || !Object.hasOwn(controls, clear)) return pads;
    pads.push({ set, clear, label: controls[set].label });
  }
}

/** A loop's length in beats on a constant-tempo grid; 0 without a loop, a tempo or a rate. */
export function loopBeats(start: number, end: number, bpm: number | null, sampleRate: number): number {
  if (bpm === null || !(bpm > 0) || !(sampleRate > 0) || !(end > start)) return 0;
  return ((end - start) * bpm) / 60 / sampleRate;
}
