//! Per-track file state used by ingest: last-seen mtime and size (to skip unchanged files) and a
//! "missing" flag set when the file vanishes from a watched folder. Same threading rules as
//! `TrackDao`.

use crate::database::Database;
use crate::error::Result;
use rusqlite::params;

/// Last-seen state of a track's file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrackFileState {
    pub mtime_ms: i64,
    pub size_bytes: i64,
    pub missing: bool,
}

/// A track whose path falls under a folder, with its missing flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackFileEntry {
    pub track_id: i64,
    pub path: String,
    pub missing: bool,
}

/// Per-track file state reads and writes.
pub struct TrackFileDao<'a> {
    db: &'a Database,
}

impl<'a> TrackFileDao<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Records the file as seen and present.
    pub fn set(&self, track_id: i64, mtime_ms: i64, size_bytes: i64) -> Result<()> {
        self.db.connection().execute(
            "INSERT INTO track_files (track_id, mtime_ms, size_bytes, missing) VALUES (?1, ?2, ?3, 0) \
             ON CONFLICT(track_id) DO UPDATE SET mtime_ms=?2, size_bytes=?3, missing=0;",
            params![track_id, mtime_ms, size_bytes],
        )?;
        Ok(())
    }

    pub fn set_missing(&self, track_id: i64, missing: bool) -> Result<()> {
        self.db.connection().execute(
            "UPDATE track_files SET missing=?2 WHERE track_id=?1;",
            params![track_id, i64::from(missing)],
        )?;
        Ok(())
    }

    pub fn find(&self, track_id: i64) -> Result<Option<TrackFileState>> {
        self.db.query_optional(
            "SELECT mtime_ms, size_bytes, missing FROM track_files WHERE track_id=?1;",
            params![track_id],
            |row| {
                Ok(TrackFileState {
                    mtime_ms: row.get(0)?,
                    size_bytes: row.get(1)?,
                    missing: row.get::<_, i64>(2)? != 0,
                })
            },
        )
    }

    /// Tracks whose path starts with `prefix` (callers pass a folder path ending in a separator).
    pub fn under(&self, prefix: &str) -> Result<Vec<TrackFileEntry>> {
        // substr compare instead of LIKE so '%' and '_' in folder names are literal
        self.db.query_rows(
            "SELECT t.id, t.path, COALESCE(f.missing, 0) FROM tracks t \
             LEFT JOIN track_files f ON f.track_id = t.id \
             WHERE substr(t.path, 1, length(?1)) = ?1 ORDER BY t.id;",
            params![prefix],
            |row| {
                Ok(TrackFileEntry {
                    track_id: row.get(0)?,
                    path: row.get(1)?,
                    missing: row.get::<_, i64>(2)? != 0,
                })
            },
        )
    }

    pub fn missing_count(&self) -> Result<i64> {
        Ok(self.db.connection().query_row(
            "SELECT COUNT(*) FROM track_files WHERE missing=1;",
            [],
            |row| row.get(0),
        )?)
    }
}
