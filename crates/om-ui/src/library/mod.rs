//! Library UI logic, without a window: the services that own the library, analysis, the prep queue
//! and the watch folders; the track table and its sources; the browser; the track loader; and the
//! library settings page. The gpui front end renders these models.

mod services;
mod track_drag;
mod track_source;
mod track_table;
mod tree;

pub use services::{
    default_options, LibraryEvent, LibraryServices, Options, TrackAnalysis, LIBRARY_DB_ENV,
    MEMORY_PATH,
};
pub use track_drag::{describe as describe_track_drag, parse as parse_track_drag};
pub use track_source::{PrepQueueSource, SearchTrackSource, TrackSource};
pub use track_table::{columns, Column, TrackTableModel, MAX_CACHED_PAGES, PAGE_SIZE};
mod browser;
mod control;
mod track_loader;

pub use browser::{LibraryBrowser, LoadHandler, SEARCH_PLACEHOLDER};
pub use track_loader::{LoadError, TrackLoader};
