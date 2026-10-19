//! Persisted list of watched folders (absolute paths). Same threading rules as `TrackDao`.

use crate::database::Database;
use crate::error::Result;
use rusqlite::params;

/// Watched-folder reads and writes.
pub struct WatchFolderDao<'a> {
    db: &'a Database,
}

impl<'a> WatchFolderDao<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Adding an existing path is a no-op success.
    pub fn add(&self, path: &str) -> Result<()> {
        self.db.connection().execute(
            "INSERT OR IGNORE INTO watch_folders (path) VALUES (?1);",
            params![path],
        )?;
        Ok(())
    }

    pub fn remove(&self, path: &str) -> Result<()> {
        self.db
            .connection()
            .execute("DELETE FROM watch_folders WHERE path=?1;", params![path])?;
        Ok(())
    }

    /// Watched paths in insertion order.
    pub fn list(&self) -> Result<Vec<String>> {
        self.db
            .query_rows("SELECT path FROM watch_folders ORDER BY id;", [], |row| {
                row.get(0)
            })
    }
}
