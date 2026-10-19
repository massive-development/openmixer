//! Temporary directories for fixtures.

use std::path::{Path, PathBuf};

/// A fresh directory under the system temp dir, removed (with its contents) on drop.
#[derive(Debug)]
pub struct TempDir {
    dir: tempfile::TempDir,
}

impl TempDir {
    /// Creates a directory whose name starts with `prefix`.
    ///
    /// # Panics
    /// Panics if the directory cannot be created; tests have nothing to recover to.
    pub fn new(prefix: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(prefix)
            .tempdir()
            .expect("create temp dir");
        Self { dir }
    }

    /// The directory itself.
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// A path inside the directory (the file is not created).
    pub fn file(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
}
