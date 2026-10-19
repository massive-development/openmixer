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
}

/// Result alias for library operations.
pub type Result<T> = std::result::Result<T, LibraryError>;
