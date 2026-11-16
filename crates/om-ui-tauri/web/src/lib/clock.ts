import type { DeckFrame, Snapshot } from './packet';

/**
 * The deck's playhead in frames at `now` (same clock as `snap.receivedAt`): the snapshot's position
 * advanced by the time since the engine read it, kept inside the track.
 */
export function interpolate(deck: DeckFrame, snap: Snapshot, now: number): number {
  const elapsedMs = snap.ageMs + now - snap.receivedAt;
  const advance = deck.playing ? (elapsedMs / 1000) * deck.fileRate * deck.rate : 0;
  return clamp(deck.position + advance, 0, Math.max(0, deck.length - 1));
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}
