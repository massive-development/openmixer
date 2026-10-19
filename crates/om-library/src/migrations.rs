//! Forward-only schema migrations. The table and column names are kept from the original schema, so databases
//! written by earlier builds open unchanged. Later subsystems append their own entries.

use crate::database::{Database, Transaction};
use crate::error::{LibraryError, Result};

/// One numbered schema step. Versions are strictly increasing and start at 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration {
    pub version: i32,
    pub name: &'static str,
    /// One or more statements.
    pub sql: &'static str,
}

const VERSION_TABLE: &str = "CREATE TABLE IF NOT EXISTS schema_version (
 version INTEGER PRIMARY KEY,
 name TEXT NOT NULL,
 applied_at INTEGER NOT NULL);";

const SCHEMA_V1: &str = r#"
CREATE TABLE tracks (
  id INTEGER PRIMARY KEY,
  path TEXT NOT NULL UNIQUE,
  content_hash TEXT NOT NULL DEFAULT '',
  title TEXT NOT NULL DEFAULT '',
  artist TEXT NOT NULL DEFAULT '',
  album TEXT NOT NULL DEFAULT '',
  genre TEXT NOT NULL DEFAULT '',
  year INTEGER NOT NULL DEFAULT 0,
  duration_sec REAL NOT NULL DEFAULT 0,
  bpm REAL,
  musical_key TEXT NOT NULL DEFAULT '',
  camelot TEXT NOT NULL DEFAULT '',
  rating INTEGER NOT NULL DEFAULT 0 CHECK (rating BETWEEN 0 AND 5),
  play_count INTEGER NOT NULL DEFAULT 0,
  date_added_ms INTEGER NOT NULL DEFAULT 0,
  last_played_ms INTEGER,
  comment TEXT NOT NULL DEFAULT ''
);
CREATE INDEX idx_tracks_hash ON tracks (content_hash);
CREATE INDEX idx_tracks_artist ON tracks (artist);

CREATE TABLE cues (
  id INTEGER PRIMARY KEY,
  track_id INTEGER NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
  slot INTEGER NOT NULL CHECK (slot >= 0),
  position_frames INTEGER NOT NULL CHECK (position_frames >= 0),
  colour_argb INTEGER NOT NULL DEFAULT 0,
  label TEXT NOT NULL DEFAULT '',
  UNIQUE (track_id, slot)
);

CREATE TABLE loops (
  id INTEGER PRIMARY KEY,
  track_id INTEGER NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
  slot INTEGER NOT NULL CHECK (slot >= 0),
  start_frames INTEGER NOT NULL CHECK (start_frames >= 0),
  end_frames INTEGER NOT NULL,
  colour_argb INTEGER NOT NULL DEFAULT 0,
  label TEXT NOT NULL DEFAULT '',
  CHECK (end_frames > start_frames),
  UNIQUE (track_id, slot)
);

CREATE TABLE playlists (
  id INTEGER PRIMARY KEY,
  parent_id INTEGER REFERENCES playlists (id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  is_folder INTEGER NOT NULL DEFAULT 0,
  position INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_playlists_parent ON playlists (parent_id, position);

CREATE TABLE playlist_entries (
  id INTEGER PRIMARY KEY,
  playlist_id INTEGER NOT NULL REFERENCES playlists (id) ON DELETE CASCADE,
  track_id INTEGER NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
  position INTEGER NOT NULL
);
CREATE INDEX idx_entries_playlist ON playlist_entries (playlist_id, position);
CREATE INDEX idx_entries_track ON playlist_entries (track_id);
"#;

const SCHEMA_V2: &str = r#"
CREATE TABLE track_files (
  track_id INTEGER PRIMARY KEY REFERENCES tracks (id) ON DELETE CASCADE,
  mtime_ms INTEGER NOT NULL,
  size_bytes INTEGER NOT NULL,
  missing INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_track_files_missing ON track_files (missing);

CREATE TABLE watch_folders (
  id INTEGER PRIMARY KEY,
  path TEXT NOT NULL UNIQUE
);
"#;

const SCHEMA_V3: &str = r#"
CREATE INDEX idx_tracks_bpm ON tracks (bpm);
CREATE INDEX idx_tracks_rating ON tracks (rating);
CREATE INDEX idx_tracks_added ON tracks (date_added_ms);
CREATE INDEX idx_tracks_genre ON tracks (genre);
CREATE INDEX idx_tracks_camelot ON tracks (camelot);
"#;

const SCHEMA_V4: &str = r#"
CREATE TABLE prep_queue (
  id INTEGER PRIMARY KEY,
  track_id INTEGER NOT NULL UNIQUE REFERENCES tracks (id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  priority INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'queued'
    CHECK (status IN ('queued', 'running', 'done', 'failed', 'cancelled')),
  error TEXT NOT NULL DEFAULT ''
);
CREATE INDEX idx_prep_queue_order ON prep_queue (status, priority DESC, position);
"#;

const CORE_MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "tracks, cues, loops, playlists",
        sql: SCHEMA_V1,
    },
    Migration {
        version: 2,
        name: "track file state, watch folders",
        sql: SCHEMA_V2,
    },
    Migration {
        version: 3,
        name: "search indexes",
        sql: SCHEMA_V3,
    },
    Migration {
        version: 4,
        name: "track prep queue",
        sql: SCHEMA_V4,
    },
];

/// The library's own migrations, in order.
pub fn core_migrations() -> &'static [Migration] {
    CORE_MIGRATIONS
}

/// Applies every migration newer than the database's schema version, each in its own savepoint.
/// Fails if the list is not strictly increasing or the database is newer than the list (no
/// downgrades).
pub fn migrate(db: &Database, migrations: &[Migration]) -> Result<()> {
    if !is_strictly_increasing(migrations) {
        return Err(LibraryError::Migration(
            "migration versions must be strictly increasing".into(),
        ));
    }
    let current = current_schema_version(db)?;
    if let Some(last) = migrations.last() {
        if current > i64::from(last.version) {
            return Err(LibraryError::Migration(format!(
                "library database is newer than this app (schema {current})"
            )));
        }
    }
    for m in migrations {
        if i64::from(m.version) > current {
            apply(db, m)?;
        }
    }
    Ok(())
}

/// 0 for a database that has never been migrated.
pub fn current_schema_version(db: &Database) -> Result<i64> {
    db.exec(VERSION_TABLE)?;
    Ok(db.connection().query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version;",
        [],
        |row| row.get(0),
    )?)
}

fn is_strictly_increasing(list: &[Migration]) -> bool {
    list.windows(2)
        .all(|pair| pair[0].version < pair[1].version)
        && list.first().is_none_or(|m| m.version >= 1)
}

fn apply(db: &Database, m: &Migration) -> Result<()> {
    let tx = Transaction::begin(db)?;
    db.exec(m.sql).map_err(|e| {
        LibraryError::Migration(format!("migration {} ({}): {e}", m.version, m.name))
    })?;
    db.connection().execute(
        "INSERT INTO schema_version (version, name, applied_at) VALUES (?1, ?2, ?3);",
        rusqlite::params![m.version, m.name, now_ms()],
    )?;
    tx.commit()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}
