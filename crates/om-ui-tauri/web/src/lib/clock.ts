import type { DeckFrame, Snapshot } from './packet';

/**
 * The deck's playhead in frames at `now` (same clock as `snap.receivedAt`): the snapshot's position
 * advanced by the time since the engine read it, wrapped inside a loop it is playing round, and
 * kept inside the track.
 */
export function interpolate(deck: DeckFrame, snap: Snapshot, now: number): number {
  const elapsedMs = snap.ageMs + now - snap.receivedAt;
  const advance = deck.playing ? (elapsedMs / 1000) * deck.fileRate * deck.rate : 0;
  return clamp(wrapLoop(deck, deck.position + advance), 0, Math.max(0, deck.length - 1));
}

/** Whether the engine is playing round a valid loop the snapshot's position is inside. */
function inLoop(deck: DeckFrame): boolean {
  const valid = deck.looping && deck.loopEnd > deck.loopStart;
  return deck.playing && valid && deck.position >= deck.loopStart && deck.position < deck.loopEnd;
}

// The engine jumps back to the loop start at its end, so the prediction does too, either way round.
function wrapLoop(deck: DeckFrame, position: number): number {
  if (!inLoop(deck)) return position;
  const span = deck.loopEnd - deck.loopStart;
  return deck.loopStart + modulo(position - deck.loopStart, span);
}

// Never negative, unlike `%`, so reverse play wraps to the loop's end.
function modulo(value: number, span: number): number {
  return ((value % span) + span) % span;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}
