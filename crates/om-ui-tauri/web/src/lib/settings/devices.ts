import { invoke } from '@tauri-apps/api/core';

/** One line of feedback a page shows after a change. */
export interface Notice {
  text: string;
  error: boolean;
}

/** A page as it stands, and what the last change said about itself. */
export interface View<P> {
  page: P;
  notice: Notice | null;
}

/** An id the shell understands and the words to show for it. */
export interface Choice {
  id: string;
  label: string;
}

// ---- MIDI ----

export interface MidiPage {
  /** False when no MIDI backend started. */
  available: boolean;
  inputs: string[];
  enableAll: boolean;
  enabledInputs: string[];
  /** The map file in use. */
  mapFile: string;
  /** What MIDI learn can bind. */
  targets: Choice[];
  /** The target an armed learn waits on. */
  learning: string | null;
}

export type MidiChange =
  | { field: 'enableAll'; value: boolean }
  | { field: 'toggleInput'; value: string }
  | { field: 'loadMap'; value: string }
  | { field: 'loadCurrentMap' }
  | { field: 'useDefaultMap' }
  | { field: 'learn'; value: string }
  | { field: 'cancelLearn' };

/** The MIDI page as it stands; a learn that ended reports its notice here. */
export const midiPage = (): Promise<View<MidiPage>> => invoke('midi_page');

export const midiChange = (change: MidiChange): Promise<View<MidiPage>> => invoke('midi_change', { change });

// ---- Mapping ----

export interface MappingFilter {
  text: string;
  group: string | null;
  channel: number | null;
}

export interface MappingRow {
  index: number;
  text: string;
  group: string;
  conflicted: boolean;
}

/** One binding's form: every field as typed, and why it does not parse yet. */
export interface MappingForm {
  kind: string;
  channel: string;
  number: string;
  target: string;
  mode: string;
  min: string;
  max: string;
  step: string;
  initial: string;
  invert: boolean;
  lsb: string;
  error: string | null;
}

export interface MappingConflict {
  first: number;
  second: number;
  control: string;
}

export interface MappingPage {
  available: boolean;
  mapFile: string;
  dirty: boolean;
  learning: boolean;
  filter: MappingFilter;
  groups: string[];
  rows: MappingRow[];
  selected: number | null;
  form: MappingForm | null;
  conflicts: MappingConflict[];
  /** Newest first, each with its age. */
  monitor: string[];
  kinds: Choice[];
  modes: Choice[];
  targets: Choice[];
}

export type MappingChange =
  | { field: 'refresh' }
  | { field: 'setFilter'; value: MappingFilter }
  | { field: 'select'; value: number | null }
  | { field: 'new' }
  | { field: 'setForm'; value: MappingForm }
  | { field: 'apply' }
  | { field: 'duplicate' }
  | { field: 'delete' }
  | { field: 'learnSelected' }
  | { field: 'learnNew'; value: string }
  | { field: 'cancelLearn' }
  | { field: 'undo' }
  | { field: 'redo' }
  | { field: 'save' }
  | { field: 'import'; value: string }
  | { field: 'export'; value: string }
  | { field: 'reset' }
  | { field: 'resolve'; value: { first: number; second: number; keep: 'first' | 'second' } };

export const mappingPage = (): Promise<View<MappingPage>> => invoke('mapping_page');

export const mappingChange = (change: MappingChange): Promise<View<MappingPage>> =>
  invoke('mapping_change', { change });

/** The modes that bind a second (low) byte, so the form shows its field. */
export const usesLsb = (mode: string): boolean => mode === 'absolute14';

// ---- Plugins ----

export interface PluginParam {
  id: string;
  name: string;
  min: number;
  max: number;
  default: number;
  value: number;
  /** The value as the panel prints it. */
  text: string;
}

export interface PluginRow {
  id: string;
  /** `Latency 128 fr / 2.9 ms`; null when the plugin reports none. */
  latency: string | null;
  hasWindow: boolean;
  params: PluginParam[];
}

export interface ClapSection {
  scan: 'unasked' | 'allowed' | 'declined';
  standardFolders: string[];
  files: string[];
  /** `.clap` files that wait for the in-process warning. */
  waiting: number;
  warningAccepted: boolean;
}

export interface PluginsPage {
  available: boolean;
  folder: string;
  loaded: PluginRow[];
  /** Built-in plugins that are not loaded. */
  builtins: Choice[];
  clap: ClapSection | null;
  themeCount: number;
}

export type PluginsChange =
  | { field: 'refresh' }
  | { field: 'reload' }
  | { field: 'loadBuiltin'; value: string }
  | { field: 'unload'; value: string }
  | { field: 'setParam'; value: { id: string; param: string; value: number } }
  | { field: 'clapScan'; value: boolean }
  | { field: 'clapAcceptWarning' };

export const pluginsPage = (): Promise<View<PluginsPage>> => invoke('plugins_page');

export const pluginsChange = (change: PluginsChange): Promise<View<PluginsPage>> =>
  invoke('plugins_change', { change });

/** Opens a loaded plugin's own window; says what happened, or rejects with why not. */
export const openPluginWindow = (id: string): Promise<string> => invoke('open_plugin_window', { id });

/** A value held between a slider's two ends, `0..1`, for the range input. */
export const paramFraction = (p: Pick<PluginParam, 'min' | 'max' | 'value'>): number => {
  const span = p.max - p.min;
  return span > 0 ? Math.min(1, Math.max(0, (p.value - p.min) / span)) : 0;
};

/** The value a `0..1` slider position stands for. */
export const paramValue = (p: Pick<PluginParam, 'min' | 'max'>, fraction: number): number =>
  p.min + (p.max - p.min) * Math.min(1, Math.max(0, fraction));

/** The text a notice-less call leaves: a failure of the call itself, as the page shows it. */
export const failure = (error: unknown): Notice => ({ text: String(error), error: true });
