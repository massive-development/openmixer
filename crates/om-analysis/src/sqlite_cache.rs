//! [`ResultCache`] on SQLite (one table). Use `":memory:"` for tests. Port of
//! `src/analysis/SqliteResultCache.{h,cpp}`: same table, columns, primary key and `user_version`.
//! The connection sits behind a mutex, so any thread may call it.

use crate::ResultCache;
use rusqlite::{params, Connection};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// Value of `PRAGMA user_version` written by this build.
pub const SCHEMA_VERSION: i32 = 1;

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS analysis_results (
    content_hash TEXT NOT NULL,
    analyzer TEXT NOT NULL,
    version INTEGER NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY (content_hash, analyzer));";

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("cannot open analysis cache {path}: {source}")]
    Open {
        path: String,
        #[source]
        source: rusqlite::Error,
    },
    #[error("cannot create analysis cache schema: {0}")]
    Schema(#[source] rusqlite::Error),
}

#[derive(Debug)]
pub struct SqliteResultCache {
    db: Mutex<Connection>,
}

impl SqliteResultCache {
    /// Opens (creating if needed) the cache at `path`. The error says why it failed.
    pub fn open(path: &str) -> Result<Self, CacheError> {
        let db = Connection::open(path).map_err(|source| CacheError::Open {
            path: path.to_owned(),
            source,
        })?;
        db.execute_batch(SCHEMA).map_err(CacheError::Schema)?;
        db.pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(CacheError::Schema)?;
        Ok(Self { db: Mutex::new(db) })
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl ResultCache for SqliteResultCache {
    fn find(&self, content_hash: &str, analyzer_id: &str, version: i32) -> Option<String> {
        let db = self.lock();
        let mut statement = db
            .prepare_cached(
                "SELECT payload FROM analysis_results
                 WHERE content_hash = ?1 AND analyzer = ?2 AND version = ?3;",
            )
            .ok()?;
        statement
            .query_row(params![content_hash, analyzer_id, version], |row| {
                row.get::<_, String>(0)
            })
            .ok()
    }

    fn store(&self, content_hash: &str, analyzer_id: &str, version: i32, payload: &str) -> bool {
        self.lock()
            .execute(
                "INSERT OR REPLACE INTO analysis_results
                 (content_hash, analyzer, version, payload) VALUES (?1, ?2, ?3, ?4);",
                params![content_hash, analyzer_id, version, payload],
            )
            .is_ok()
    }

    fn erase(&self, content_hash: &str) -> usize {
        self.lock()
            .execute(
                "DELETE FROM analysis_results WHERE content_hash = ?1;",
                params![content_hash],
            )
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests;
