import type { Pad } from '../ipc';

/** The pad a key press names, by the key each pad reports; null for any other key. */
export function padForKey(pads: readonly Pad[], key: string): number | null {
  const pad = pads.find((p) => p.key === key);
  return pad ? pad.pad : null;
}

/** Whether a key event's target takes typed text, so a digit there is not a pad. */
export function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName);
}

/** A pad's gain as a percentage, the way GPUI's caption prints it. */
export const gainText = (gain: number): string => `${Math.round(gain * 100)}%`;

/** A pad's pitch in semitones with its sign. */
export const pitchText = (semitones: number): string => `${semitones >= 0 ? '+' : '-'}${Math.abs(semitones).toFixed(1)} st`;

/** A stem's filter as a word: off at the centre, else the side and its depth. */
export function filterText(position: number): string {
  const percent = Math.round(Math.abs(position) * 100);
  if (percent === 0) return 'Off';
  return `${position < 0 ? 'Low-pass' : 'High-pass'} ${percent}%`;
}
