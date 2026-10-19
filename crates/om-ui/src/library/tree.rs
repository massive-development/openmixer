//! The sidebar's view of the library: playlists, folders and watch folders, read from the library
//! database. Implements [`crate::sidebar::LibraryTree`] for [`om_library::Library`].

use om_library::Library;

use crate::sidebar::{LibraryTree, PlaylistRow};

impl LibraryTree for Library {
    fn playlist_children(&self, parent: Option<i64>) -> Result<Vec<PlaylistRow>, String> {
        self.playlists()
            .children(parent)
            .map(|rows| {
                rows.into_iter()
                    .map(|p| PlaylistRow {
                        id: p.id,
                        name: p.name,
                        is_folder: p.is_folder,
                    })
                    .collect()
            })
            .map_err(|e| e.to_string())
    }

    fn watch_folders(&self) -> Vec<String> {
        self.watch_folders().list().unwrap_or_default()
    }
}
