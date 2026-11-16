import { Channel, invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { tryDecode, type Snapshot } from './packet';

/** A front-end control key such as `deck0.play` or `mixer.crossfader`; `controls` lists them. */
export type ControlKey = string;

/** How a control is pressed or moved. */
export type ControlKind = 'toggle' | 'trigger' | 'continuous';

/** One control as the shell lists it, from the GPUI widget models; values are engine units. */
export interface ControlSpec {
  id: string;
  min: number;
  max: number;
  /** What a double-click restores. */
  default: number;
  /** The value a knob's pointer points straight up for; null when the model has none. */
  detent: number | null;
  kind: ControlKind;
  /** The word GPUI prints on the control; empty where it prints none. */
  label: string;
}

/** Every control the shell lists, by key. */
export type Controls = Record<ControlKey, ControlSpec>;

/** One level's place in the `deckPeaks` bytes. */
export interface PeakLevel {
  samplesPerPoint: number;
  len: number;
}

/** A deck's track as the shell reports it; `null` where the track has no value. */
export interface DeckInfo {
  title: string;
  artist: string;
  bpm: number | null;
  key: string | null;
  /** Source frame of the grid's beat 0, a downbeat. */
  firstBeatFrame: number | null;
  sampleRate: number;
  numSamples: number;
  /** False for a colourless silhouette (all height in the low byte). */
  spectral: boolean;
  levels: PeakLevel[];
  hotcues: (number | null)[];
  generation: number;
  /** The title as GPUI's deck header prints it: the track's, or its words for an empty deck. */
  titleText: string;
  /** The key as GPUI's deck header prints it, or `--`. */
  keyText: string;
}

/** Payload of the `deck-changed` event. */
export interface DeckChanged {
  deck: number;
  generation: number;
}

/** The waveform constants the GPUI strip uses. */
export interface WaveformStyle {
  fill: number;
  playedBrightness: number;
  /** A column with any height reaches at least this far from the centre line, in CSS px. */
  minHalfPx: number;
}

/**
 * One deck's waveform lane ink (`om_ui::waveform::StripInk`, Low-Band style): colours as `#rrggbb`
 * with their alphas apart, marker sizes in CSS px.
 */
export interface StripInk {
  well: string;
  body: string;
  highs: string;
  /** Opacity of the high band over the body (`LOW_BAND_HIGH_ALPHA`). */
  highsAlpha: number;
  /**
   * The overview covers its played part with this (the zoomed lanes do not shade it); null when the
   * style dims it by `playedBrightness` instead.
   */
  playedShade: string | null;
  playedShadeAlpha: number | null;
  gridBar: string;
  gridBarAlpha: number;
  gridBeat: string;
  gridBeatAlpha: number;
  playhead: string;
  playheadHalo: string | null;
  playheadHaloWidth: number;
  playheadWidth: number;
  cueGuide: string;
  cueGuideAlpha: number;
  mainCue: string;
  /** Hot cue 1..8 colours. */
  cues: string[];
  barNumberSize: number;
  barNumberMinSpacingPx: number;
  cueFlagWidth: number;
  cueFlagHeight: number;
}

/** Deck `deck`'s waveform lane ink. */
export const stripInk = (deck: number): Promise<StripInk> => invoke<StripInk>('strip_ink', { deck });

/** The active theme as one `:root` block of `--om-*` variables. */
export const themeCss = (): Promise<string> => invoke<string>('theme_css');

/** Tells the shell the window has painted; a smoke run exits on it. */
export const ready = (): Promise<void> => invoke<void>('ready');

/** Whether this is a smoke run, which must see a snapshot before it calls `ready`. */
export const isSmoke = (): Promise<boolean> => invoke<boolean>('smoke');

/** The waveform constants of the GPUI strip. */
export const waveformStyle = (): Promise<WaveformStyle> => invoke<WaveformStyle>('waveform_style');

/** Starts the 30 Hz snapshot stream (replacing any earlier one); bytes that do not decode are dropped. */
export function startStream(onSnapshot: (s: Snapshot) => void): Promise<void> {
  const channel = new Channel<ArrayBuffer>();
  channel.onmessage = (bytes) => {
    const snap = tryDecode(bytes, performance.now());
    if (snap) onSnapshot(snap);
  };
  return invoke<void>('start_stream', { channel });
}

/** Whether the smoke run must start deck A through `control` before `ready` (a library loads it). */
export const isSmokePlay = (): Promise<boolean> => invoke<boolean>('smoke_play');

/** The spec for `key`; throws for a key the shell did not list. */
export function resolveControl(controls: Controls, key: string): ControlSpec {
  if (!Object.hasOwn(controls, key)) throw new Error(`unknown control key: ${key}`);
  return controls[key];
}

let controlSpecs: Promise<Controls> | null = null;

/** Loads the control list once; later calls share it, and a failed load is retried. */
export function loadControls(): Promise<Controls> {
  controlSpecs ??= invoke<Controls>('controls').catch((error: unknown) => {
    controlSpecs = null;
    throw error;
  });
  return controlSpecs;
}

/** Writes one control by key, in engine units; rejects for an unknown key without calling the shell. */
export async function control(key: ControlKey, value: number): Promise<void> {
  const { id } = resolveControl(await loadControls(), key);
  await invoke<void>('control', { id, value });
}

/** Deck `deck`'s track info and peak layout. */
export const deckInfo = (deck: number): Promise<DeckInfo> => invoke<DeckInfo>('deck_info', { deck });

/** Deck `deck`'s packed waveform peaks; empty while it has none. */
export async function deckPeaks(deck: number): Promise<Uint8Array> {
  return new Uint8Array(await invoke<ArrayBuffer>('deck_peaks', { deck }));
}

/** Calls `cb` whenever a deck's track, waveform or cues change. */
export function onDeckChanged(cb: (change: DeckChanged) => void): Promise<UnlistenFn> {
  return listen<DeckChanged>('deck-changed', (event) => cb(event.payload));
}
