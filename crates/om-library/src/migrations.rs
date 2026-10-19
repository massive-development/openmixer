//! Forward-only schema migrations. The table and column names are kept from the original schema, so databases
//! written by earlier builds open unchanged. Later subsystems append their own entries.

use crate::database::{fts5_module_available, Database, Transaction};
use crate::error::{LibraryError, Result};

/// One numbered schema step. Versions are strictly increasing and start at 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration {
    pub version: i32,
    pub name: &'static str,
    /// One or more statements.
    pub sql: &'static str,
    /// The SQL creates FTS5 objects. On a SQLite build without FTS5 the migration is left unapplied
    /// (and so is every later one, to keep the order); the library then searches with LIKE.
    pub needs_fts5: bool,
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

const SCHEMA_V5: &str = r#"
CREATE TABLE play_history (
  id INTEGER PRIMARY KEY,
  track_id INTEGER NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
  played_ms INTEGER NOT NULL,
  deck INTEGER
);
CREATE INDEX idx_play_history_track ON play_history (track_id, played_ms);
"#;

const SCHEMA_V6: &str = r#"
CREATE TABLE artwork (
  track_id INTEGER PRIMARY KEY REFERENCES tracks (id) ON DELETE CASCADE,
  mime TEXT NOT NULL,
  width INTEGER,
  height INTEGER,
  data BLOB NOT NULL
);
"#;

/// Free-text search index. External-content FTS5 over the text columns free-text search covers
/// (`title artist album genre comment`), kept in step with `tracks` by triggers. Diacritics are
/// folded (`emile` finds `Émile`); prefixes of 2 and 3 characters are indexed so `*` queries are
/// fast. Existing rows are indexed by the final `rebuild`. Forward-only.
const SCHEMA_V7: &str = r#"
CREATE VIRTUAL TABLE tracks_fts USING fts5(
  title, artist, album, genre, comment,
  content = 'tracks', content_rowid = 'id',
  tokenize = 'unicode61 remove_diacritics 2', prefix = '2 3'
);

CREATE TRIGGER tracks_fts_ai AFTER INSERT ON tracks BEGIN
  INSERT INTO tracks_fts (rowid, title, artist, album, genre, comment)
  VALUES (new.id, new.title, new.artist, new.album, new.genre, new.comment);
END;

CREATE TRIGGER tracks_fts_ad AFTER DELETE ON tracks BEGIN
  INSERT INTO tracks_fts (tracks_fts, rowid, title, artist, album, genre, comment)
  VALUES ('delete', old.id, old.title, old.artist, old.album, old.genre, old.comment);
END;

CREATE TRIGGER tracks_fts_au AFTER UPDATE OF title, artist, album, genre, comment ON tracks BEGIN
  INSERT INTO tracks_fts (tracks_fts, rowid, title, artist, album, genre, comment)
  VALUES ('delete', old.id, old.title, old.artist, old.album, old.genre, old.comment);
  INSERT INTO tracks_fts (rowid, title, artist, album, genre, comment)
  VALUES (new.id, new.title, new.artist, new.album, new.genre, new.comment);
END;

INSERT INTO tracks_fts (tracks_fts) VALUES ('rebuild');
"#;

const CORE_MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "tracks, cues, loops, playlists",
        sql: SCHEMA_V1,
        needs_fts5: false,
    },
    Migration {
        version: 2,
        name: "track file state, watch folders",
        sql: SCHEMA_V2,
        needs_fts5: false,
    },
    Migration {
        version: 3,
        name: "search indexes",
        sql: SCHEMA_V3,
        needs_fts5: false,
    },
    Migration {
        version: 4,
        name: "track prep queue",
        sql: SCHEMA_V4,
        needs_fts5: false,
    },
    Migration {
        version: 5,
        name: "play history",
        sql: SCHEMA_V5,
        needs_fts5: false,
    },
    Migration {
        version: 6,
        name: "track artwork",
        sql: SCHEMA_V6,
        needs_fts5: false,
    },
    Migration {
        version: 7,
        name: "free-text search index (FTS5)",
        sql: SCHEMA_V7,
        needs_fts5: true,
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
            if m.needs_fts5 && !fts5_module_available(db) {
                tracing::warn!(
                    "SQLite has no FTS5: migration {} ({}) is not applied; search uses LIKE",
                    m.version,
                    m.name
                );
                break;
            }
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
