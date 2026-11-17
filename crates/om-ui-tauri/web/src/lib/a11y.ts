import { captionText, type KnobReadout } from './controls/readout';

/** U+2212, which a screen reader speaks as "minus"; a hyphen is often skipped. */
const MINUS = '−';

/** `Deck A` for deck 0. */
export function deckName(deck: number): string {
  return `Deck ${letter(deck)}`;
}

/** The letter of deck or channel `index`: `A` for 0. */
export function letter(index: number): string {
  return String.fromCharCode(65 + index);
}

/**
 * The accessible name of the control at `key`: its deck or channel and what it is, containing the
 * word printed on it (`label`). An unknown key reads its label, or the key itself.
 */
export function controlName(key: string, label: string): string {
  const [head, ...rest] = key.split('.');
  const part = rest.join('.');
  const word = label.toLowerCase();
  const channel = /^ch(\d+)$/.exec(head);
  if (channel) return `Channel ${letter(Number(channel[1]))} ${channelPart(part, word)}`;
  const deck = /^deck(\d+)$/.exec(head);
  if (deck) return `${deckName(Number(deck[1]))} ${DECK_PARTS[part] ?? (word || part)}`;
  return MIXER_NAMES[key] ?? (word || key);
}

const DECK_PARTS: Record<string, string> = { tempo: 'tempo', seek: 'position' };
const MIXER_NAMES: Record<string, string> = { 'mixer.master': 'Master gain', 'mixer.crossfader': 'Crossfader' };

function channelPart(part: string, word: string): string {
  if (part.startsWith('eq.')) return `${word || part.slice(3)} EQ`;
  if (part === 'fader') return 'fader';
  return word || part;
}

/**
 * A fader's spoken form by the control it belongs to: `+2.00 %`, `75 %`, `centre`. A knob speaks
 * what it prints under its label instead (`controls/readout.ts`, `captionText`).
 */
export function valueText(key: string, value: number): string {
  if (key.endsWith('.tempo')) return `${signed((value - 1) * 100, 2)} %`;
  if (key.endsWith('.fader')) return `${Math.round(value * 100)} %`;
  if (key === 'mixer.crossfader') return crossfaderText(value);
  return String(value);
}

/** What a knob speaks and prints under its label: GPUI's caption when its spec names a readout. */
export function knobValueText(key: string, value: number, readout: KnobReadout | null): string {
  return readout ? captionText(readout, value) : valueText(key, value);
}

function crossfaderText(value: number): string {
  const lean = Math.round((value - 0.5) * 200);
  if (lean === 0) return 'centre';
  return `${lean < 0 ? 'A' : 'B'} ${Math.abs(lean)} %`;
}

// A number with its sign and `digits` decimals; a value that rounds to zero carries no sign.
function signed(value: number, digits: number): string {
  const text = Math.abs(value).toFixed(digits);
  if (Number(text) === 0) return text;
  return `${value < 0 ? MINUS : '+'}${text}`;
}

/** `m:ss.t` for a spoken time: `0:01.0`. Tenths are cut, as a clock counts. */
export function clockText(seconds: number): string {
  const tenths = Number.isFinite(seconds) ? Math.floor(Math.abs(seconds) * 10 + 1e-6) : 0;
  const whole = Math.floor(tenths / 10);
  return `${wholeClock(whole)}.${tenths % 10}`;
}

function wholeClock(seconds: number): string {
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

/** `Deck A hot cue 3, set at 0:01.0`, or `..., empty`; `seconds` is null for an empty pad. */
export function hotcueName(deck: number, label: string, seconds: number | null): string {
  const state = seconds === null ? 'empty' : `set at ${clockText(seconds)}`;
  return `${deckName(deck)} hot cue ${label}, ${state}`;
}

/** `Deck A loop 4 beats`; `label` is the size as printed (`1/4`). */
export function loopName(deck: number, beats: number, label: string): string {
  return `${deckName(deck)} loop ${label} ${beats > 1 ? 'beats' : 'beat'}`;
}

/** A meter's level in whole decibels; at or under the floor it is `silent`. */
export function meterText(db: number, floorDb: number): string {
  if (!(db > floorDb)) return 'silent';
  const whole = Math.round(db);
  return `${whole < 0 ? MINUS : ''}${Math.abs(whole)} dB`;
}

/** Where the playhead is in the track, in whole seconds: `1:02 of 2:04`. */
export function positionText(seconds: number, totalSeconds: number): string {
  if (!(totalSeconds > 0)) return 'no track';
  return `${wholeClock(Math.floor(Math.max(seconds, 0)))} of ${wholeClock(Math.floor(totalSeconds))}`;
}

/**
 * Paces writes that only assistive technology reads (a meter's level, the track position), so a
 * value that moves every frame reaches the accessibility tree a few times a second at most.
 */
export class Paced {
  private last = -Infinity;

  constructor(private readonly periodMs: number) {}

  /** True when a period has passed since the last time this returned true. */
  due(now: number): boolean {
    if (now - this.last < this.periodMs) return false;
    this.last = now;
    return true;
  }
}
