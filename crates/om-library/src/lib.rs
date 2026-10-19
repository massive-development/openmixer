//! Library database: SQLite schema and migrations, track/cue/playlist/watch-folder DAOs, and the
//! track record and key helpers (Todo 20). Metadata readers, folder ingest and watch folders
//! (Todo 21). Search, importers and the prep queue come later (Todos 22-24). Port of
//! `src/library/`.
//!
//! Threading: one writer per database file. A `Database` is used by one thread at a time; the UI
//! opens its own read-only connection to the same file (WAL lets it read while the writer commits).

#![forbid(unsafe_code)]

mod cue_dao;
mod database;
mod error;
mod file_url;
mod ingest;
mod library;
mod metadata;
mod migrations;
mod ordered_list;
mod playlist_dao;
mod track_dao;
mod track_file_dao;
mod track_key;
mod track_record;
mod watch_folder_dao;
mod watch_folders;

pub use cue_dao::{CueDao, CuePoint, SavedLoop};
pub use database::{Database, OpenMode, Transaction};
pub use error::{LibraryError, Result};
pub use file_url::file_url_to_path;
pub use ingest::{IngestSummary, Ingestor};
pub use library::Library;
pub use metadata::{
    Artwork, LoftyMetadataReader, MetadataReader, MetadataReaderRegistry, TrackMetadata,
};
pub use migrations::{core_migrations, current_schema_version, migrate, Migration};
pub use ordered_list::OrderedList;
pub use playlist_dao::{Playlist, PlaylistDao, PlaylistEntry};
pub use track_dao::{read_row, TrackDao, SELECT_COLUMNS};
pub use track_file_dao::{TrackFileDao, TrackFileEntry, TrackFileState};
pub use track_key::{normalise_key, set_key};
pub use track_record::TrackRecord;
pub use watch_folder_dao::WatchFolderDao;
pub use watch_folders::{
    Clock, ErrorCallback, ScanDoneCallback, WatchFolderOptions, WatchFolders,
};
