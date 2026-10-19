//! Composition root for the library database: opens, migrates and hands out the DAOs.
//! Threading: a read-write `Library` belongs to one thread. A read-only open (for the UI) needs a
//! writer to have opened and migrated the file first.

use crate::cue_dao::CueDao;
use crate::database::{Database, OpenMode};
use crate::error::Result;
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
    pub fn open(path: &str, mode: OpenMode, migrations: &[Migration]) -> Result<Self> {
        let db = Database::open(path, mode)?;
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
