//! Library UI logic, without a window: the services that own the library, analysis, the prep queue
//! and the watch folders; the track table and its sources; the browser; the track loader; and the
//! library settings page. The gpui front end renders these models.

mod mix_match;
mod services;
mod track_drag;
mod track_source;
mod track_table;
mod tree;

pub use mix_match::{
    bpm_distance_percent, key_relation, score as mix_score, KeyRelation, MixLevel, MixMatch,
    MixReference, BPM_TOLERANCE_PERCENT,
};
pub use services::{
    default_options, LibraryEvent, LibraryServices, Options, TrackAnalysis, LIBRARY_DB_ENV,
    MEMORY_PATH,
};
pub use track_drag::{describe as describe_track_drag, parse as parse_track_drag};
pub use track_source::{PlaylistSource, PrepQueueSource,SearchTrackSource, TrackSource};
pub use track_table::{
    column_shown, columns, rating_stars, sort_glyph, Column, SortGlyph, TrackTableModel,
    MAX_CACHED_PAGES, PAGE_SIZE, STEMS_COLUMN,
};
mod browser;
mod control;
mod cue_saver;
mod play_tracker;
mod track_loader;

pub use browser::{LibraryBrowser, LoadHandler, SearchFeed, SEARCH_PLACEHOLDER};
pub use cue_saver::{CueSaver, Cues};
pub use play_tracker::{PlayTracker, MAX_STEP, PLAY_THRESHOLD};
pub use track_loader::{LoadError, StemSource, TrackLoader};
