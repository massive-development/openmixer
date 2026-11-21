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
  /** Start with the sampler drawer open (`--demo-scene=sampler`). */
  samplerOpen: boolean;
  /** Start deck A on its stems tab (`--demo-scene=stems`). */
  stemsTab: boolean;
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

/** One sampler pad as the strip draws it. */
export interface Pad {
  pad: number;
  /** The key that presses it (`1` to `8`). */
  key: string;
  name: string;
  loaded: boolean;
  missing: boolean;
  loading: boolean;
  playing: boolean;
  gain: number;
  /** Semitones. */
  pitch: number;
  mode: 'one-shot' | 'gate' | 'loop';
  selected: boolean;
}

/** The sampler: its pads, the selected pad's caption and the ranges its controls take. */
export interface SamplerState {
  /** False when the app has no sample bank. */
  available: boolean;
  pads: Pad[];
  selected: number;
  caption: string;
  modeHint: string;
  /** The last problem a sampler action had, until the next action. */
  status: string | null;
  maxGain: number;
  maxPitch: number;
  /** The velocity a click or key press hits a pad with. */
  pressVelocity: number;
}

/** One stem of a deck. */
export interface Stem {
  name: string;
  muted: boolean;
  soloed: boolean;
  volume: number;
  /** -1 low-pass to +1 high-pass, 0 off. */
  filter: number;
  /** How loud the stem sounds, 0..1. */
  level: number;
}

/** A deck's stems: whether it plays from them and what the track's separation is doing. */
export interface StemsState {
  deck: number;
  live: boolean;
  /** The loaded track has a complete cached set to load. */
  cached: boolean;
  /** The track can be separated (a stem host and a library track). */
  canPrepare: boolean;
  /** `None`, `Preparing 42%`, `Ready` or `Failed`. */
  label: string;
  preparing: number | null;
  /** Why the last stems action did not happen. */
  note: string | null;
  stems: Stem[];
  maxVolume: number;
  filterDetent: number;
}

const failed =
  (name: string) =>
  (error: unknown): void =>
    console.error(`openmixer: ${name}`, error);

/** Runs a shell command whose failure is only logged; the shell's state event carries the reason. */
function fire(name: string, args?: Record<string, unknown>): void {
  invoke<void>(name, args).catch(failed(name));
}

export const samplerState = (): Promise<SamplerState> => invoke<SamplerState>('sampler_state');
export const samplerPress = (pad: number, velocity: number): void => fire('sampler_press', { pad, velocity });
export const samplerRelease = (): void => fire('sampler_release');
export const samplerSelect = (pad: number): void => fire('sampler_select', { pad });
export const samplerCycleMode = (): void => fire('sampler_cycle_mode');
export const samplerSetGain = (gain: number): void => fire('sampler_set_gain', { gain });
export const samplerSetPitch = (semitones: number): void => fire('sampler_set_pitch', { semitones });
export const samplerLoad = (pad: number, path: string): void => fire('sampler_load', { pad, path });
export const samplerClear = (pad: number): void => fire('sampler_clear', { pad });
export const samplerStopAll = (): void => fire('sampler_stop_all');

export const stemsState = (deck: number): Promise<StemsState | null> => invoke<StemsState | null>('stems_state', { deck });
export const stemVolume = (deck: number, stem: number, gain: number): void => fire('stem_volume', { deck, stem, gain });
export const stemFilter = (deck: number, stem: number, position: number): void =>
  fire('stem_filter', { deck, stem, position });
export const stemMute = (deck: number, stem: number): void => fire('stem_mute', { deck, stem });
export const stemSolo = (deck: number, stem: number): void => fire('stem_solo', { deck, stem });
export const stemAllOn = (deck: number): void => fire('stem_all_on', { deck });
export const stemToggle = (deck: number): void => fire('stem_toggle', { deck });
export const stemPrepare = (deck: number): void => fire('stem_prepare', { deck });

/** The sampler changed. */
export function onSamplerState(cb: (state: SamplerState) => void): Promise<UnlistenFn> {
  return listen<SamplerState>('sampler-state', (event) => cb(event.payload));
}

/** A deck's stems changed. */
export function onStemsState(cb: (state: StemsState) => void): Promise<UnlistenFn> {
  return listen<StemsState>('stems-state', (event) => cb(event.payload));
}
