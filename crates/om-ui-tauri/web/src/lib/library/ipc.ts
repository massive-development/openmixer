import { invoke } from '@tauri-apps/api/core';

/** Which sidebar line this is; headings (`playlists`, `folder`, `watches`) are not selectable. */
export type SidebarKind = 'all' | 'playlists' | 'folder' | 'playlist' | 'watches' | 'watch' | 'prep';

export interface SidebarLine {
  /** Position in the flattened tree; `selectSidebar` takes it back. */
  index: number;
  depth: number;
  kind: SidebarKind;
  label: string;
  playlistId: number;
  selected: boolean;
  selectable: boolean;
}

export interface Column {
  id: number;
  title: string;
  sortable: boolean;
  /** Width in px; 0 shares what the fixed columns leave. */
  width: number;
  numeric: boolean;
  /** Hidden in windows narrower than this; 0 always shows. */
  minWindowW: number;
}

/** How well a row's track mixes with the reference deck: key and tempo, key only, or neither. */
export type MixLevel = 'strong' | 'key' | 'none';

export interface Row {
  row: number;
  trackId: number;
  /** One string per column of the view. */
  cells: string[];
  compatible: boolean;
  mix: MixLevel;
  selected: boolean;
  /** The deck the track is loaded on. */
  onDeck: number | null;
  /** Why the track would not load, in words. */
  error: string | null;
  /** Analysis progress `0..1` while queued or running; null otherwise. */
  preparing: number | null;
}

export interface Naming {
  prompt: string;
  text: string;
}

export interface TrackAnalysis {
  title: string;
  artist: string;
  bpm: number | null;
  key: string;
  camelot: string;
  durationSec: number;
  analysed: boolean;
  prep: 'queued' | 'running' | 'failed' | null;
}

/** The prep queue's state and what is known of the first selected track. */
export interface Analysis {
  queued: number;
  running: number;
  failed: number;
  /** Mean progress of the running tracks, `0..1`. */
  progress: number;
  track: TrackAnalysis | null;
}

/** Everything the browser shows, in one answer. */
export interface LibraryView {
  sidebar: SidebarLine[];
  columns: Column[];
  search: string;
  placeholder: string;
  naming: Naming | null;
  status: string;
  statusIsError: boolean;
  rowCount: number;
  selected: number[];
  /** Column id and direction (true ascending) of the last header click. */
  sort: [number, boolean] | null;
  stemsAvailable: boolean;
  stemsInFlight: number;
  /** The index of `rows[0]`. */
  first: number;
  rows: Row[];
  analysis: Analysis;
}

/** The rows to draw: `count` from `first`. */
export interface RowWindow {
  first: number;
  count: number;
}

/** What a key did in the browser. */
export type KeyOutcome = 'search' | 'moved' | 'activated' | 'edited' | 'ignored';

/** The name box's actions: start one, change its text, finish or drop it. */
export type NameAction = 'new-playlist' | 'new-folder' | 'rename' | 'text' | 'submit' | 'cancel';

type View = Promise<LibraryView>;

export const libraryView = (window: RowWindow): View => invoke('library_view', { window });
export const librarySearch = (text: string, commit: boolean, window: RowWindow): View =>
  invoke('library_search', { text, commit, window });
export const libraryClearSearch = (window: RowWindow): View => invoke('library_clear_search', { window });
export const librarySelectSidebar = (index: number, window: RowWindow): View =>
  invoke('library_select_sidebar', { index, window });
export const librarySort = (column: number, window: RowWindow): View => invoke('library_sort', { column, window });
export const librarySelectRow = (row: number, extend: boolean, window: RowWindow): View =>
  invoke('library_select_row', { row, extend, window });
export const libraryKey = (key: string, window: RowWindow): Promise<[KeyOutcome, LibraryView]> =>
  invoke('library_key', { key, window });
export const libraryActivate = (row: number, window: RowWindow): View => invoke('library_activate', { row, window });
export const libraryLoad = (deck: number, trackId: number, window: RowWindow): View =>
  invoke('library_load', { deck, trackId, window });
export const libraryAnalyse = (window: RowWindow): View => invoke('library_analyse', { window });
export const libraryStems = (cancel: boolean, window: RowWindow): View => invoke('library_stems', { cancel, window });
export const libraryName = (action: NameAction, text: string | null, window: RowWindow): View =>
  invoke('library_name', { action, text, window });
export const libraryDeletePlaylist = (window: RowWindow): View => invoke('library_delete_playlist', { window });
export const libraryRemoveFromPlaylist = (window: RowWindow): View =>
  invoke('library_remove_from_playlist', { window });
export const libraryDropOnPlaylist = (playlistId: number, trackId: number, window: RowWindow): View =>
  invoke('library_drop_on_playlist', { playlistId, trackId, window });
export const libraryDropPlaylist = (playlistId: number, folder: number | null, window: RowWindow): View =>
  invoke('library_drop_playlist', { playlistId, folder, window });
export const libraryDropOnRow = (source: number, target: number, window: RowWindow): View =>
  invoke('library_drop_on_row', { source, target, window });
export const libraryCopy = (): Promise<string> => invoke('library_copy');
