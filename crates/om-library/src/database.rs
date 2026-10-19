//! SQLite connection with foreign keys, a 5 s busy timeout and WAL on writable files,
//! and nested savepoint transactions.

use crate::error::{LibraryError, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Params, Row, Statement};
use std::cell::Cell;
use std::time::Duration;

/// Read-write opens create and migrate the file; read-only opens neither create nor migrate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    ReadWrite,
    ReadOnly,
}

/// One SQLite connection. Not `Sync`: use it from the thread that owns it.
pub struct Database {
    conn: Connection,
    savepoints: Cell<u64>,
}

impl Database {
    /// Opens `path` (`":memory:"` for a private in-memory database). WAL is enabled for writable
    /// files only; the in-memory and read-only cases skip it.
    pub fn open(path: &str, mode: OpenMode) -> Result<Self> {
        let flags = match mode {
            OpenMode::ReadOnly => OpenFlags::SQLITE_OPEN_READ_ONLY,
            OpenMode::ReadWrite => {
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
            }
        };
        let conn = Connection::open_with_flags(path, flags)?;
        let db = Self {
            conn,
            savepoints: Cell::new(0),
        };
        // Reader and writer connections may briefly contend for the write lock.
        db.conn.busy_timeout(Duration::from_millis(5000))?;
        // Cascading deletes (cues, playlist entries) depend on this; it is off by default in SQLite.
        db.exec("PRAGMA foreign_keys = ON;")?;
        if mode == OpenMode::ReadWrite && path != ":memory:" {
            db.enable_wal()?;
        }
        Ok(db)
    }

    fn enable_wal(&self) -> Result<()> {
        let mode: String = self
            .conn
            .query_row("PRAGMA journal_mode = WAL;", [], |row| row.get(0))?;
        if mode.eq_ignore_ascii_case("wal") {
            Ok(())
        } else {
            Err(LibraryError::Setup(format!("WAL unavailable: {mode}")))
        }
    }

    /// The underlying connection, for reads that the DAOs do not cover.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Prepares one statement. Fails when the SQL does not compile.
    pub fn prepare(&self, sql: &str) -> Result<Statement<'_>> {
        Ok(self.conn.prepare(sql)?)
    }

    /// Runs one or more statements with no parameters.
    pub fn exec(&self, sql: &str) -> Result<()> {
        self.conn.execute_batch(sql)?;
        Ok(())
    }

    /// Runs a query and collects every row. Any step error fails the whole read, so an empty
    /// result is never a hidden failure.
    pub fn query_rows<T, P, F>(&self, sql: &str, params: P, map: F) -> Result<Vec<T>>
    where
        P: Params,
        F: FnMut(&Row<'_>) -> rusqlite::Result<T>,
    {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt
            .query_map(params, map)?
            .collect::<rusqlite::Result<Vec<T>>>()?;
        Ok(rows)
    }

    /// Runs a query expected to return at most one row.
    pub fn query_optional<T, P, F>(&self, sql: &str, params: P, map: F) -> Result<Option<T>>
    where
        P: Params,
        F: FnOnce(&Row<'_>) -> rusqlite::Result<T>,
    {
        Ok(self.conn.query_row(sql, params, map).optional()?)
    }

    /// Row id of the most recent successful INSERT on this connection.
    pub fn last_insert_rowid(&self) -> i64 {
        self.conn.last_insert_rowid()
    }

    fn next_savepoint_id(&self) -> u64 {
        let id = self.savepoints.get() + 1;
        self.savepoints.set(id);
        id
    }
}

/// Optional SQLite features the library uses, detected when an engine opens the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LibraryCapabilities {
    /// The FTS5 module is compiled in and the `tracks_fts` index exists. Free-text search uses it;
    /// otherwise it uses LIKE.
    pub fts5: bool,
}

impl LibraryCapabilities {
    /// Probes `db`. Read-only connections probe too: the FTS5 check uses a temporary table.
    pub fn detect(db: &Database) -> Self {
        let has_index = db
            .query_optional(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'tracks_fts';",
                [],
                |_| Ok(()),
            )
            .ok()
            .flatten()
            .is_some();
        Self {
            fts5: has_index && fts5_module_available(db),
        }
    }
}

/// True when this SQLite build can create FTS5 tables. Uses a temporary table, so it is safe on a
/// read-only connection and leaves nothing behind.
pub fn fts5_module_available(db: &Database) -> bool {
    let created = db
        .exec("CREATE VIRTUAL TABLE temp.om_fts5_probe USING fts5(x);")
        .is_ok();
    let _ = db.exec("DROP TABLE IF EXISTS temp.om_fts5_probe;");
    created
}

/// Rolls back on drop unless `commit` ran. Nests via savepoints, so DAO calls compose.
pub struct Transaction<'a> {
    db: &'a Database,
    name: String,
    /// The outermost transaction is a real `BEGIN IMMEDIATE`, not a savepoint: a deferred start
    /// can pin a snapshot and then fail its first write with SQLITE_BUSY_SNAPSHOT, with no wait.
    outermost: bool,
    finished: bool,
}

impl<'a> Transaction<'a> {
    /// Opens a transaction on `db`: `BEGIN IMMEDIATE` when none is open, else a savepoint.
    pub fn begin(db: &'a Database) -> Result<Self> {
        let name = format!("om_sp{}", db.next_savepoint_id());
        let outermost = db.connection().is_autocommit();
        if outermost {
            db.exec("BEGIN IMMEDIATE;")?;
        } else {
            db.exec(&format!("SAVEPOINT {name};"))?;
        }
        Ok(Self {
            db,
            name,
            outermost,
            finished: false,
        })
    }

    /// Ends the transaction, keeping its changes.
    pub fn commit(mut self) -> Result<()> {
        self.finished = true;
        if self.outermost {
            self.db.exec("COMMIT;")
        } else {
            self.db.exec(&format!("RELEASE {};", self.name))
        }
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if !self.finished {
            // Nothing sensible to do with a failure while unwinding; the changes are discarded.
            let _ = if self.outermost {
                self.db.exec("ROLLBACK;")
            } else {
                self.db
                    .exec(&format!("ROLLBACK TO {0}; RELEASE {0};", self.name))
            };
        }
    }
}
