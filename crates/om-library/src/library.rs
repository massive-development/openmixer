//! Composition root for the library database: opens, migrates and hands out the DAOs.
//! Threading: a read-write `Library` belongs to one thread. A read-only open (for the UI) needs a
//! writer to have opened and migrated the file first.

use crate::artwork_dao::ArtworkDao;
use crate::cue_dao::CueDao;
use crate::database::{fts5_module_available, Database, OpenMode};
use crate::error::{LibraryError, Result};
use crate::migrations::{migrate, Migration};
use crate::playlist_dao::PlaylistDao;
use crate::prep_queue_dao::PrepQueueDao;
use crate::track_dao::TrackDao;
use crate::track_file_dao::TrackFileDao;
use crate::watch_folder_dao::WatchFolderDao;
use std::sync::Arc;

/// An open library database. DAOs are cheap views created on demand.
pub struct Library {
    db: Arc<Database>,
}

impl Library {
    /// Opens `path` and, for a read-write open, applies `migrations` newer than the schema.
    /// Pass [`crate::core_migrations`] for the library's own schema.
    ///
    /// Fails with [`LibraryError::Fts5Required`] when the schema has the FTS5 triggers but this
    /// SQLite build has no FTS5 (see the migration 7 note in the crate docs).
    pub fn open(path: &str, mode: OpenMode, migrations: &[Migration]) -> Result<Self> {
        let db = Database::open(path, mode)?;
        let fts5_module = fts5_module_available(&db);
        Self::finish_open(db, mode, migrations, fts5_module)
    }

    /// The part of [`Library::open`] after the connection exists. `fts5_module` says whether this
    /// SQLite build has FTS5; tests pass `false` to exercise the no-FTS5 build.
    fn finish_open(
        db: Database,
        mode: OpenMode,
        migrations: &[Migration],
        fts5_module: bool,
    ) -> Result<Self> {
        // Checked before migrating: a no-FTS5 build cannot run the triggers, so refuse early.
        require_fts5_for_schema(&db, fts5_module)?;
        // A read-only connection cannot migrate; the writer already did.
        if mode == OpenMode::ReadWrite {
            migrate(&db, migrations)?;
        }
        // Shared only by handles on the owner thread (a Database is Send but not Sync, and Rc would
        // make the Library itself non-Send).
        #[allow(clippy::arc_with_non_send_sync)]
        Ok(Self { db: Arc::new(db) })
    }

    pub fn database(&self) -> &Database {
        &self.db
    }

    /// A second handle on the same connection, for readers that must own their database (the UI
    /// search source and the prep queue views).
    pub fn shared_database(&self) -> Arc<Database> {
        Arc::clone(&self.db)
    }

    pub fn tracks(&self) -> TrackDao<'_> {
        TrackDao::new(&self.db)
    }

    pub fn cues(&self) -> CueDao<'_> {
        CueDao::new(&self.db)
    }

    pub fn artwork(&self) -> ArtworkDao<'_> {
        ArtworkDao::new(&self.db)
    }

    pub fn playlists(&self) -> PlaylistDao<'_> {
        PlaylistDao::new(&self.db)
    }

    pub fn track_files(&self) -> TrackFileDao<'_> {
        TrackFileDao::new(&self.db)
    }

    pub fn watch_folders(&self) -> WatchFolderDao<'_> {
        WatchFolderDao::new(&self.db)
    }

    pub fn prep_queue(&self) -> PrepQueueDao<'_> {
        PrepQueueDao::new(&self.db)
    }
}

/// Refuses a schema whose `tracks` triggers write to `tracks_fts` when this build has no FTS5: every
/// insert, update or delete of a track would fail. A schema without the triggers is fine.
fn require_fts5_for_schema(db: &Database, fts5_module: bool) -> Result<()> {
    if fts5_module {
        return Ok(());
    }
    let has_fts_triggers = db
        .query_optional(
            "SELECT 1 FROM sqlite_master WHERE type = 'trigger' \
             AND name IN ('tracks_fts_ai', 'tracks_fts_ad', 'tracks_fts_au');",
            [],
            |_| Ok(()),
        )?
        .is_some();
    if has_fts_triggers {
        Err(LibraryError::Fts5Required)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::core_migrations;
    use tempfile::TempDir;

    fn db_path(dir: &TempDir) -> String {
        dir.path()
            .join("library.db")
            .to_str()
            .expect("utf-8 path")
            .to_string()
    }

    #[test]
    fn schema_with_fts_triggers_is_refused_without_fts5() {
        let dir = TempDir::new().expect("temp dir");
        let path = db_path(&dir);
        Library::open(&path, OpenMode::ReadWrite, core_migrations()).expect("create v7");

        let db = Database::open(&path, OpenMode::ReadWrite).expect("raw open");
        let err = Library::finish_open(db, OpenMode::ReadWrite, core_migrations(), false)
            .err()
            .expect("no-FTS5 build must refuse the FTS5 schema");
        assert!(matches!(err, LibraryError::Fts5Required), "got {err:?}");

        let read_only = Database::open(&path, OpenMode::ReadOnly).expect("raw read-only");
        assert!(
            matches!(
                Library::finish_open(read_only, OpenMode::ReadOnly, core_migrations(), false),
                Err(LibraryError::Fts5Required)
            ),
            "read-only opens are refused too"
        );

        let db = Database::open(&path, OpenMode::ReadWrite).expect("raw open");
        Library::finish_open(db, OpenMode::ReadWrite, core_migrations(), true)
            .expect("an FTS5 build opens the same file");
    }

    #[test]
    fn schema_before_migration_seven_opens_without_fts5() {
        let dir = TempDir::new().expect("temp dir");
        let path = db_path(&dir);
        Library::open(&path, OpenMode::ReadWrite, &core_migrations()[..6]).expect("create v6");

        // Read-only, so this exercises only the check: a read-write open would run migration 7 with
        // the real FTS5 module present.
        let db = Database::open(&path, OpenMode::ReadOnly).expect("raw read-only open");
        Library::finish_open(db, OpenMode::ReadOnly, core_migrations(), false)
            .expect("no FTS triggers, so a no-FTS5 build may open it");
    }
}
