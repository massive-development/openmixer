//! Library database: SQLite schema and migrations, track/cue/playlist/watch-folder DAOs, and the
//! track record and key helpers. Port of `src/library/` (Todo 20). Ingest, watch-folder scanning,
//! search, importers and the prep queue come later (Todos 21-23).
//!
//! Threading: one writer per database file. A `Database` is used by one thread at a time; the UI
//! opens its own read-only connection to the same file (WAL lets it read while the writer commits).

#![forbid(unsafe_code)]

mod cue_dao;
mod database;
mod error;
mod file_url;
mod library;
mod migrations;
mod ordered_list;
mod playlist_dao;
mod track_dao;
mod track_file_dao;
mod track_key;
mod track_record;
mod watch_folder_dao;

pub use cue_dao::{CueDao, CuePoint, SavedLoop};
pub use database::{Database, OpenMode, Transaction};
pub use error::{LibraryError, Result};
pub use file_url::file_url_to_path;
pub use library::Library;
pub use migrations::{core_migrations, current_schema_version, migrate, Migration};
pub use ordered_list::OrderedList;
pub use playlist_dao::{Playlist, PlaylistDao, PlaylistEntry};
pub use track_dao::{read_row, TrackDao, SELECT_COLUMNS};
pub use track_file_dao::{TrackFileDao, TrackFileEntry, TrackFileState};
pub use track_key::{normalise_key, set_key};
pub use track_record::TrackRecord;
pub use watch_folder_dao::WatchFolderDao;
