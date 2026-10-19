//! The sidebar's view of the library: playlists, folders and watch folders, read from the library
//! database. Implements [`crate::sidebar::LibraryTree`] for [`om_library::Library`].

use om_library::{Library, LibraryError};

use crate::sidebar::{LibraryTree, PlaylistRow};

impl LibraryTree for Library {
    fn playlist_children(&self, parent: Option<i64>) -> Result<Vec<PlaylistRow>, LibraryError> {
        self.playlists().children(parent).map(|rows| {
            rows.into_iter()
                .map(|p| PlaylistRow {
                    id: p.id,
                    name: p.name,
                    is_folder: p.is_folder,
                })
                .collect()
        })
    }

    fn watch_folders(&self) -> Result<Vec<String>, LibraryError> {
        self.watch_folders().list()
    }
}
