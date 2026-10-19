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
    /// A file or folder the library needs could not be created or read.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Connection setup failed (for example, WAL is unavailable).
    #[error("{0}")]
    Setup(String),
    /// A metadata reader could not decode the file (corrupt, unsupported, unreadable).
    #[error("{0}")]
    Metadata(String),
    /// A folder scan was refused (not a folder) or stopped before it finished.
    #[error("{0}")]
    Ingest(String),
    /// An import file could not be read, was not recognised, or did not parse.
    #[error(transparent)]
    Import(#[from] ImportError),
    /// A search query was rejected (unknown field, unreadable value, unsortable field).
    #[error("{0}")]
    Query(String),
}

/// Why an import file was rejected. Its display text is the message shown in the UI status line.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The file could not be read from disk.
    #[error("Cannot read {}: {source}", path.display())]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The XML is not well formed (a reader or attribute error from quick-xml).
    #[error(transparent)]
    Xml(#[from] quick_xml::Error),
    /// The XML is well formed but its structure is wrong (unbalanced tags, no root, a bad reference).
    #[error("{what}")]
    Format { what: &'static str },
    /// The document parsed but is not an iTunes property list.
    #[error("Not an iTunes property list")]
    Plist,
    /// No registered importer recognises the document root.
    #[error("Unrecognised library format: {file}")]
    Unrecognised { file: String },
}

/// Result alias for library operations.
pub type Result<T> = std::result::Result<T, LibraryError>;
