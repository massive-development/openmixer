//! Which stem-cache directory belongs to which track (migration 8). The stems themselves live on
//! disk; this table only maps `(track, separator, version)` to a directory name relative to the
//! stem cache root, plus its size and creation time, so the cache can be capped and swept. The
//! directory is never removed by SQLite: `om-stems` deletes it (see `StemService::forget_track`).
//! Same threading rules as `TrackDao`.

use crate::database::Database;
use crate::error::Result;
use rusqlite::params;

/// One cached stem set of one track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackStemsRecord {
    pub track_id: i64,
    pub sep_id: String,
    pub version: i32,
    /// Directory name under the stem cache root, e.g. `<hash>-<sep>-v<n>`.
    pub dir: String,
    pub bytes: i64,
    pub created_ms: i64,
}

pub struct TrackStemsDao<'a> {
    db: &'a Database,
}

impl<'a> TrackStemsDao<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Inserts or replaces the row for `(track_id, sep_id, version)`.
    pub fn set(&self, record: &TrackStemsRecord) -> Result<()> {
        self.db.connection().execute(
            "INSERT INTO track_stems (track_id, sep_id, version, dir, bytes, created_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(track_id, sep_id, version) DO UPDATE SET \
             dir=?4, bytes=?5, created_ms=?6;",
            params![
                record.track_id,
                record.sep_id,
                record.version,
                record.dir,
                record.bytes,
                record.created_ms
            ],
        )?;
        Ok(())
    }

    pub fn get(
        &self,
        track_id: i64,
        sep_id: &str,
        version: i32,
    ) -> Result<Option<TrackStemsRecord>> {
        self.db.query_optional(
            "SELECT track_id, sep_id, version, dir, bytes, created_ms FROM track_stems \
             WHERE track_id=?1 AND sep_id=?2 AND version=?3;",
            params![track_id, sep_id, version],
            read_row,
        )
    }

    /// Removes one row. True when a row was removed.
    pub fn delete(&self, track_id: i64, sep_id: &str, version: i32) -> Result<bool> {
        let n = self.db.connection().execute(
            "DELETE FROM track_stems WHERE track_id=?1 AND sep_id=?2 AND version=?3;",
            params![track_id, sep_id, version],
        )?;
        Ok(n > 0)
    }

    /// Removes every row that points at `dir` (the directory was evicted or is gone).
    pub fn delete_by_dir(&self, dir: &str) -> Result<usize> {
        Ok(self
            .db
            .connection()
            .execute("DELETE FROM track_stems WHERE dir=?1;", params![dir])?)
    }

    /// Every row, least recently created first.
    pub fn list_all_bytes(&self) -> Result<Vec<TrackStemsRecord>> {
        self.db.query_rows(
            "SELECT track_id, sep_id, version, dir, bytes, created_ms FROM track_stems \
             ORDER BY created_ms, track_id;",
            [],
            read_row,
        )
    }

    /// Rows for one track.
    pub fn for_track(&self, track_id: i64) -> Result<Vec<TrackStemsRecord>> {
        self.db.query_rows(
            "SELECT track_id, sep_id, version, dir, bytes, created_ms FROM track_stems \
             WHERE track_id=?1 ORDER BY sep_id, version;",
            params![track_id],
            read_row,
        )
    }

    /// Sum of the recorded sizes.
    pub fn total_bytes(&self) -> Result<i64> {
        Ok(self.db.connection().query_row(
            "SELECT COALESCE(SUM(bytes), 0) FROM track_stems;",
            [],
            |row| row.get(0),
        )?)
    }
}

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrackStemsRecord> {
    Ok(TrackStemsRecord {
        track_id: row.get(0)?,
        sep_id: row.get(1)?,
        version: row.get(2)?,
        dir: row.get(3)?,
        bytes: row.get(4)?,
        created_ms: row.get(5)?,
    })
}
