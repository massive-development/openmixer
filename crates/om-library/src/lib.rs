//! Library database: SQLite schema and migrations, track/cue/playlist/watch-folder DAOs, and the
//! track record and key helpers (Todo 20). Metadata readers, folder ingest and watch folders
//! (Todo 21). Search and filter (Todo 23). Library import (iTunes, Rekordbox) (Todo 22) and the
//! track prep queue (Todo 24).
//!
//! Threading: one writer per database file. A `Database` is used by one thread at a time; the UI
//! opens its own read-only connection to the same file (WAL lets it read while the writer commits).

#![forbid(unsafe_code)]

mod cue_dao;
mod database;
mod error;
mod file_url;
mod import_cues;
mod import_merge;
mod import_model;
mod import_writer;
mod ingest;
mod itunes_importer;
mod lenient;
mod library;
mod library_import;
mod library_importer;
mod metadata;
mod migrations;
mod ordered_list;
mod playlist_dao;
mod plist_reader;
mod prep_queue;
mod prep_queue_dao;
mod prep_results;
mod rekordbox_importer;
mod search_compiler;
mod search_engine;
mod search_fields;
mod search_keys;
mod search_parser;
mod search_query;
mod track_dao;
mod track_file_dao;
mod track_key;
mod track_record;
mod watch_folder_dao;
mod watch_folders;
mod xml;

pub use cue_dao::{CueDao, CuePoint, SavedLoop};
pub use database::{Database, OpenMode, Transaction};
pub use error::{LibraryError, Result};
pub use file_url::file_url_to_path;
pub use import_cues::{import_cues, ASSUMED_SAMPLE_RATE, FIRST_MEMORY_SLOT};
pub use import_merge::merge_track_record;
pub use import_model::{
    CueKind, ImportReport, ImportedCue, ImportedLibrary, ImportedPlaylist, ImportedTrack,
};
pub use import_writer::ImportWriter;
pub use ingest::{IngestSummary, Ingestor};
pub use itunes_importer::ItunesImporter;
pub use library::Library;
pub use library_import::import_library_file;
pub use library_importer::{LibraryImporter, LibraryImporterRegistry};
pub use metadata::{
    Artwork, LoftyMetadataReader, MetadataReader, MetadataReaderRegistry, TrackMetadata,
};
pub use migrations::{core_migrations, current_schema_version, migrate, Migration};
pub use ordered_list::OrderedList;
pub use playlist_dao::{Playlist, PlaylistDao, PlaylistEntry};
pub use plist_reader::PlistDict;
pub use prep_queue::{ErrorHandler, Observer, PrepOptions, PrepQueue, SourceFactory};
pub use prep_queue_dao::{PrepEntry, PrepQueueDao, PrepStatus};
pub use prep_results::apply_prep_results;
pub use rekordbox_importer::RekordboxImporter;
pub use search_compiler::{compile_search, like_pattern, CompileContext, CompiledSearch};
pub use search_engine::SearchEngine;
pub use search_fields::{free_text_columns, FieldKind, SearchField, SearchFieldRegistry};
pub use search_keys::key_camelot_codes;
pub use search_parser::{parse_query, ParsedQuery};
pub use search_query::{FilterTerm, SearchQuery, SearchValue, SqlFragment};
pub use track_dao::{read_row, TrackDao, SELECT_COLUMNS};
pub use track_file_dao::{TrackFileDao, TrackFileEntry, TrackFileState};
pub use track_key::{normalise_key, set_key};
pub use track_record::TrackRecord;
pub use watch_folder_dao::WatchFolderDao;
pub use watch_folders::{Clock, ErrorCallback, ScanDoneCallback, WatchFolderOptions, WatchFolders};
pub use xml::{parse_xml, read_xml_file, XmlElement};
