import type { ControlKey, Controls } from '../ipc';

/** One control write: a key and the value it sends, in engine units. */
export type Write = [key: ControlKey, value: number];

/**
 * What deck `deck`'s Cue sends, in order: the writes the shell lists for `deck<d>.cue`, read from
 * GPUI's Cue (stop, then back to the start). Empty when the shell lists no cue for the deck.
 */
export function cueWrites(controls: Controls, deck: number): Write[] {
  const key = `deck${deck}.cue`;
  if (!Object.hasOwn(controls, key)) return [];
  return controls[key].writes.map((w): Write => [w.key, w.value]);
}

/** Sends `writes` one after another, each after the one before has been accepted; a failure stops the rest. */
export async function sendInOrder(writes: Write[], send: (key: ControlKey, value: number) => Promise<void>): Promise<void> {
  for (const [key, value] of writes) await send(key, value);
}
