import { Channel, invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { BenchStats } from './bench';
import type { KnobReadout } from './controls/readout';
import type { LayoutMap } from './drive';
import { tryDecode, type Snapshot } from './packet';
import { decodePeaks, type Peaks } from './peaks';

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
  /** How travel maps to the value: the taper GPUI's fader uses, else linear. */
  law: ControlLaw;
  /** The dB marks GPUI draws beside the control, loudest first; empty where it draws none. */
  ticks: ControlTick[];
  /** The detent pull GPUI applies while the control is dragged; null where it has none. */
  snap: ControlSnap | null;
  /** How the knob's value prints under its label, by GPUI's readout; null for a control that is no knob. */
  readout: KnobReadout | null;
  /** A composite key's writes, in order (its `id` is then empty); empty for a single control. */
  writes: ControlWrite[];
}

/** One write of a composite key: a control key and the value it sends. */
export interface ControlWrite {
  key: ControlKey;
  value: number;
}

/**
 * Travel `0..1` to value: linear (a detent strictly inside the range still takes the middle), or
 * piecewise linear through `[position, value]` knots, positions rising from 0 to 1.
 */
export type ControlLaw = { kind: 'linear' } | { kind: 'knots'; knots: [number, number][] };

/** One scale mark: travel position, the text GPUI prints there (empty for none), long mark, 0 dB mark. */
export interface ControlTick {
  pos: number;
  label: string;
  major: boolean;
  unity: boolean;
}

/** A dragged knob's pull: a value within `width` of `at`, inclusive, becomes `at`. */
export interface ControlSnap {
  at: number;
  width: number;
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
  /** Counts every change to the deck: track, waveform, hot cues. */
  generation: number;
  /** Counts waveform changes only; the peaks are fetched again only when it moves. */
  peaksGeneration: number;
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

/** How the shell runs the window: smoke, frame bench, frame cap, the notice, the frame counter. */
export interface RunConfig {
  /** A smoke run must see a snapshot before it calls `ready`. */
  smoke: boolean;
  /** Seconds to record frames for (`--ui-bench`); null when not benching. */
  benchSeconds: number | null;
  /** The lanes' frame cap (`--ui-fps-cap`); 0 is uncapped. */
  fpsCap: number;
  /** The notice the top bar shows (the mock device); null for none. */
  banner: string | null;
  /** Whether the top bar shows the frame counter (`--ui-bench`, `--debug-panel`); hidden, it is not computed. */
  hud: boolean;
  /** A `--ui-drive-out` run: the front end sends its layout map through `uiLayout`. */
  drive: boolean;
}

/** The shell's run options. */
export const runConfig = (): Promise<RunConfig> => invoke<RunConfig>('run_config');

/** Hands the controls' rectangles to the shell, which writes them for a driver (`--ui-drive-out` only). */
export const uiLayout = (map: LayoutMap): Promise<void> => invoke<void>('ui_layout', { map });

/** Hands the bench result to the shell, which writes it and exits. */
export const benchDone = (stats: BenchStats): Promise<void> => invoke<void>('bench_done', { stats });

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

/** Deck `deck`'s packed waveform peaks and their generation; no bytes while it has none. */
export async function deckPeaks(deck: number): Promise<Peaks> {
  const peaks = decodePeaks(await invoke<ArrayBuffer>('deck_peaks', { deck }));
  if (!peaks) throw new Error(`deck ${deck}: peaks answer too short`);
  return peaks;
}

/** Payload of the `load-failed` event: a load the shell could not do, and why. */
export interface LoadFailed {
  deck: number;
  file: string;
  reason: string;
}

/** Queues a load of the audio file at `path` into `deck`; the deck changes, or `load-failed` says why not. */
export const loadFile = (deck: number, path: string): Promise<void> => invoke<void>('load_file', { deck, path });

/** Calls `cb` whenever a load the front end asked for did not happen. */
export function onLoadFailed(cb: (failure: LoadFailed) => void): Promise<UnlistenFn> {
  return listen<LoadFailed>('load-failed', (event) => cb(event.payload));
}

/** Calls `cb` whenever a deck's track, waveform or cues change. */
export function onDeckChanged(cb: (change: DeckChanged) => void): Promise<UnlistenFn> {
  return listen<DeckChanged>('deck-changed', (event) => cb(event.payload));
}

/** The audio settings page: what the device offers and the current pick (`SettingsHost`). */
export interface AudioPage {
  deviceTypes: string[];
  deviceType: string;
  devices: string[];
  device: string;
  sampleRates: number[];
  sampleRate: number;
  bufferSizes: number[];
  bufferSize: number;
  /** Output pair labels of the open device ("Outputs 1-2"). */
  pairs: string[];
  masterPair: number;
  /** The headphone cue pair; null for none. */
  cuePair: number | null;
  /** The latency and load readout, or why no device runs. */
  stats: string;
  /** The silent mock runs: no change reaches a sound card. */
  mock: boolean;
}

/** One edit of the audio page, as the shell reads it. */
export type AudioChange =
  | { field: 'deviceType'; value: string }
  | { field: 'device'; value: string }
  | { field: 'sampleRate'; value: number }
  | { field: 'bufferSize'; value: number }
  | { field: 'masterPair'; value: number }
  | { field: 'cuePair'; value: number | null };

/** The audio settings page as it stands. */
export const audioSettings = (): Promise<AudioPage> => invoke<AudioPage>('audio_settings');

/** Applies one edit (saved and pushed into the engine); the page afterwards, or the reason it was refused. */
export const audioChange = (change: AudioChange): Promise<AudioPage> => invoke<AudioPage>('audio_change', { change });
