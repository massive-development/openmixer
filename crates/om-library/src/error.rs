//! Error type for the library: SQLite failures plus the rejections the DAOs report.

/// Why a library operation failed.
#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    /// A statement failed to prepare, step or commit (busy, constraint, missing table, read-only).
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    /// The addressed row does not exist.
    #[error("{0}")]
    NotFound(&'static str),
    /// The request breaks a DAO rule (a track added to a folder, a cycle, an invalid destination).
    #[error("{0}")]
    Rejected(&'static str),
    /// The migration list is invalid, a migration failed, or the database is newer than the app.
    #[error("{0}")]
    Migration(String),
    /// Connection setup failed (for example, WAL is unavailable).
    #[error("{0}")]
    Setup(String),
    /// A metadata reader could not decode the file (corrupt, unsupported, unreadable).
    #[error("{0}")]
    Metadata(String),
    /// A folder scan was refused (not a folder) or stopped before it finished.
    #[error("{0}")]
    Ingest(String),
    /// An import file could not be read, was not recognised, or its XML did not parse.
    #[error("{0}")]
    Import(String),
    /// A search query was rejected (unknown field, unreadable value, unsortable field).
    #[error("{0}")]
    Query(String),
}

/// Result alias for library operations.
pub type Result<T> = std::result::Result<T, LibraryError>;
