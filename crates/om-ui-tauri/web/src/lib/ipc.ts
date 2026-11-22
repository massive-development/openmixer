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

/** The top bar's Automix strip, as `om_ui::automix_bar` decides it. */
export interface AutomixBarState {
  enabled: boolean;
  /** A pick is loading, armed or mixing: the state word, progress, Skip and Cancel show. */
  running: boolean;
  state: string;
  /** `Next: <title> · <bpm> BPM · <key>`, or `Next: Nothing queued`. */
  next: string;
  /** 0 to 1 through the running transition. */
  progress: number;
  /** `12 beats left`; empty when no deck leads. */
  remaining: string;
  /** The last error or the reason a plan was dropped; empty when none. */
  error: string;
  lengthBeats: number;
  /** `16 beats`, or `Auto`. */
  lengthLabel: string;
  /** The length a press of the chip steps to. */
  nextLengthBeats: number;
  canStart: boolean;
  canSkip: boolean;
  canCancel: boolean;
}

/** The Record button. */
export interface RecordBarState {
  recording: boolean;
  failed: boolean;
  /** `Record`, the elapsed time while recording, or `Stopped` after a failure. */
  label: string;
  /** `12.3 MB` written so far. */
  size: string;
  /** `12 frames dropped`; empty when none were. */
  warning: string;
  droppedFrames: number;
  peakLeft: number;
  peakRight: number;
  error: string;
}

export interface HeaderState {
  automix: AutomixBarState;
  record: RecordBarState;
}

export type AutomixAction = 'start' | 'skip' | 'cancel';

/** Both header parts as the app has them now; rejects when the run has no automix or recorder. */
export const headerState = (): Promise<HeaderState> => invoke<HeaderState>('header_state');

/** The Off | On switch. */
export const automixEnable = (on: boolean): Promise<void> => invoke<void>('automix_enable', { on });

/** The length chip: a transition length in beats (0 is Auto). */
export const automixLength = (beats: number): Promise<void> => invoke<void>('automix_length', { beats });

/** Start, Skip or Cancel. */
export const automixPress = (action: AutomixAction): Promise<void> => invoke<void>('automix_press', { action });

/** Starts or stops the mix recording. */
export const recordToggle = (): Promise<void> => invoke<void>('record_toggle');
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

/** One theme the picker offers. */
export interface ThemeChoice {
  id: string;
  name: string;
}

/** The appearance page: the themes and the density choice. */
export interface AppearancePage {
  themes: ThemeChoice[];
  theme: string;
  density: 'automatic' | 'roomy' | 'compact';
}

/** One edit of the appearance page, as the shell reads it. */
export type AppearanceChange = { field: 'theme'; value: string } | { field: 'density'; value: AppearancePage['density'] };

/** What an appearance edit leaves the window to apply. */
export interface AppearanceResult {
  page: AppearancePage;
  css: string;
  stripInks: StripInk[];
}

/** The appearance settings page as it stands. */
export const appearanceSettings = (): Promise<AppearancePage> => invoke<AppearancePage>('appearance_settings');

/** Applies one edit; the page and the theme CSS to show, or the reason it was refused. */
export const appearanceChange = (change: AppearanceChange): Promise<AppearanceResult> =>
  invoke<AppearanceResult>('appearance_change', { change });

/** Where the automix queue picks from. */
export interface QueueSource {
  kind: 'library' | 'playlist' | 'search';
  /** The playlist id, for `playlist`. */
  playlist?: number;
}

/** The Automix settings page: the stored settings and the words for their choices. */
export interface AutomixPage {
  enabled: boolean;
  style: number;
  styles: string[];
  lengthBeats: number;
  lengths: { beats: number; label: string }[];
  queue: QueueSource;
  smart: boolean;
  minBpmGapPercent: number;
  maxBpmGapPercent: number;
  autostartAtEnd: boolean;
}

/** One edit of the Automix page. */
export type AutomixChange =
  | { field: 'enabled'; value: boolean }
  | { field: 'style'; value: number }
  | { field: 'lengthBeats'; value: number }
  | { field: 'queue'; value: QueueSource }
  | { field: 'smart'; value: boolean }
  | { field: 'minBpmGapPercent'; value: number }
  | { field: 'autostartAtEnd'; value: boolean };

/** A playlist the automix queue can play, labelled `Folder / Name`. */
export interface PlaylistChoice {
  id: number;
  label: string;
}

export const automixSettings = (): Promise<AutomixPage> => invoke<AutomixPage>('automix_settings');

/** Applies one edit (stored, pushed to the controller, saved); the page as it stands, or why it was refused. */
export const automixChange = (change: AutomixChange): Promise<AutomixPage> =>
  invoke<AutomixPage>('automix_change', { change });

/** The library's playlists, for the queue source; rejects when no library is open. */
export const automixPlaylists = (): Promise<PlaylistChoice[]> => invoke<PlaylistChoice[]>('automix_playlists');

/** The Recording settings page. */
export interface RecordPage {
  /** The stored folder; empty means the default. */
  folder: string;
  /** Where an empty folder records to. */
  defaultFolder: string;
  format: string;
  formats: { id: string; label: string }[];
  stopOnLowDisk: boolean;
}

/** One edit of the Recording page. */
export type RecordChange =
  | { field: 'folder'; value: string }
  | { field: 'format'; value: string }
  | { field: 'stopOnLowDisk'; value: boolean };

export const recordSettings = (): Promise<RecordPage> => invoke<RecordPage>('record_settings');

/** Applies one edit (stored, pushed to the recorder, saved); the page as it stands, or why it was refused. */
export const recordChange = (change: RecordChange): Promise<RecordPage> => invoke<RecordPage>('record_change', { change });

/** The stems preferences. */
export interface StemsPrefs {
  maxCacheGb: number;
  maxCacheLimit: number;
  autoPrepare: boolean;
  autoLoad: boolean;
}

/** One edit of the stems preferences. */
export type StemsPrefsChange =
  | { field: 'maxCacheGb'; value: number }
  | { field: 'autoPrepare'; value: boolean }
  | { field: 'autoLoad'; value: boolean };

/** What the stem host reports: the model, its licence and the cache. */
export interface StemsHostFacts {
  available: boolean;
  state: 'unavailable' | 'notInstalled' | 'licenceNotAccepted' | 'downloading' | 'preparing' | 'installed' | 'failed';
  /** The state in words. */
  line: string;
  /** Download progress, 0..1. */
  progress: number | null;
  ready: boolean;
  licence: { name: string; text: string } | null;
  cacheBytes: number;
  /** Separations queued or running. */
  inFlight: number;
}

/** The stems settings page. */
export interface StemsPage {
  prefs: StemsPrefs;
  host: StemsHostFacts;
  /** `1.5 GB of 20.0 GB used`. */
  usage: string;
}

export const stemsSettings = (): Promise<StemsPage> => invoke<StemsPage>('stems_settings');
export const stemsSettingsChange = (change: StemsPrefsChange): Promise<StemsPage> =>
  invoke<StemsPage>('stems_settings_change', { change });
/** The user accepted the licence: the model download starts. */
export const stemsAcceptDownload = (): Promise<void> => invoke<void>('stems_accept_download');
export const stemsCancelDownload = (): Promise<void> => invoke<void>('stems_cancel_download');
/** Removes every stem set; how many went. */
export const stemsClearCache = (): Promise<number> => invoke<number>('stems_clear_cache');
