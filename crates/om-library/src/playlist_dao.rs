//! Playlists, folders (nesting) and ordered entries. Siblings and entries keep dense positions
//! 0..n-1. Non-owning view of a `Database`; use from the thread that owns it.

use crate::database::{Database, Transaction};
use crate::error::{LibraryError, Result};
use crate::ordered_list::OrderedList;
use rusqlite::params;

/// One node of the playlist tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    pub id: i64,
    /// `None` for top level.
    pub parent_id: Option<i64>,
    pub name: String,
    /// Folders hold playlists and folders; playlists hold tracks.
    pub is_folder: bool,
    /// Among siblings.
    pub position: i32,
}

/// One track in a playlist. The same track may appear more than once, so the entry id is the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaylistEntry {
    pub id: i64,
    pub playlist_id: i64,
    pub track_id: i64,
    pub position: i32,
}

/// Playlist tree and entry reads and writes.
pub struct PlaylistDao<'a> {
    db: &'a Database,
    nodes: OrderedList<'a>,
    items: OrderedList<'a>,
}

impl<'a> PlaylistDao<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self {
            db,
            nodes: OrderedList::new(db, "playlists", "parent_id"),
            items: OrderedList::new(db, "playlist_entries", "playlist_id"),
        }
    }

    /// Appends under `parent_id` (`None` = top level). The parent must be a folder.
    pub fn create(&self, name: &str, parent_id: Option<i64>, is_folder: bool) -> Result<i64> {
        if !self.accepts_children(parent_id)? {
            return Err(LibraryError::Rejected("invalid destination"));
        }
        let position = self.nodes.count(parent_id)?;
        self.db.connection().execute(
            "INSERT INTO playlists (parent_id, name, is_folder, position) VALUES (?1,?2,?3,?4);",
            params![parent_id, name, i64::from(is_folder), position],
        )?;
        Ok(self.db.last_insert_rowid())
    }

    pub fn rename(&self, id: i64, name: &str) -> Result<()> {
        let changed = self.db.connection().execute(
            "UPDATE playlists SET name=?2 WHERE id=?1;",
            params![id, name],
        )?;
        if changed > 0 {
            Ok(())
        } else {
            Err(LibraryError::NotFound("no such playlist"))
        }
    }

    /// Deletes the node, everything below it, and closes the sibling gap.
    pub fn remove(&self, id: i64) -> Result<()> {
        let node = self
            .find(id)?
            .ok_or(LibraryError::NotFound("no such playlist"))?;
        let tx = Transaction::begin(self.db)?;
        self.db
            .connection()
            .execute("DELETE FROM playlists WHERE id=?1;", params![node.id])?;
        self.nodes.close_gap(node.parent_id, node.position)?;
        tx.commit()
    }

    /// Re-parents and/or reorders a node. Fails when the new parent is not a folder or is the node
    /// itself or inside it.
    pub fn move_node(&self, id: i64, new_parent_id: Option<i64>, index: i32) -> Result<()> {
        let node = self
            .find(id)?
            .ok_or(LibraryError::NotFound("no such playlist"))?;
        if !self.accepts_children(new_parent_id)? || self.would_cycle(id, new_parent_id)? {
            return Err(LibraryError::Rejected("invalid destination"));
        }

        let tx = Transaction::begin(self.db)?;
        let same_parent = node.parent_id == new_parent_id;
        if !same_parent {
            self.nodes.close_gap(node.parent_id, node.position)?;
        }
        let target = if same_parent {
            self.nodes.move_target(new_parent_id, index)?
        } else {
            self.nodes.insert_index(new_parent_id, index)?
        };
        if same_parent {
            self.nodes
                .shift_for_move(new_parent_id, node.position, target)?;
        } else {
            self.nodes.open_gap(new_parent_id, target)?;
        }
        self.db.connection().execute(
            "UPDATE playlists SET parent_id=?2, position=?3 WHERE id=?1;",
            params![id, new_parent_id, target],
        )?;
        tx.commit()
    }

    pub fn find(&self, id: i64) -> Result<Option<Playlist>> {
        self.db.query_optional(
            "SELECT id, parent_id, name, is_folder, position FROM playlists WHERE id=?1;",
            params![id],
            read_playlist,
        )
    }

    /// Children of `parent_id` (`None` = top level), ordered by position.
    pub fn children(&self, parent_id: Option<i64>) -> Result<Vec<Playlist>> {
        self.db.query_rows(
            "SELECT id, parent_id, name, is_folder, position FROM playlists \
             WHERE parent_id IS ?1 ORDER BY position;",
            params![parent_id],
            read_playlist,
        )
    }

    /// Adds a track to a playlist at `index` (a negative index appends). Fails for folders and
    /// unknown tracks, leaving the existing entries unchanged.
    pub fn add_track(&self, playlist_id: i64, track_id: i64, index: i32) -> Result<i64> {
        let list = self.find(playlist_id)?;
        if list.is_none_or(|p| p.is_folder) {
            return Err(LibraryError::Rejected("not a playlist"));
        }

        let tx = Transaction::begin(self.db)?;
        let at = self.items.insert_index(Some(playlist_id), index)?;
        self.items.open_gap(Some(playlist_id), at)?;
        self.db.connection().execute(
            "INSERT INTO playlist_entries (playlist_id, track_id, position) VALUES (?1,?2,?3);",
            params![playlist_id, track_id, at],
        )?;
        let entry_id = self.db.last_insert_rowid();
        tx.commit()?;
        Ok(entry_id)
    }

    pub fn remove_entry(&self, entry_id: i64) -> Result<()> {
        let entry = self
            .find_entry(entry_id)?
            .ok_or(LibraryError::NotFound("no such entry"))?;
        let tx = Transaction::begin(self.db)?;
        self.db.connection().execute(
            "DELETE FROM playlist_entries WHERE id=?1;",
            params![entry_id],
        )?;
        self.items
            .close_gap(Some(entry.playlist_id), entry.position)?;
        tx.commit()
    }

    /// Moves an entry to `new_index`, clamped to the playlist's end.
    pub fn move_entry(&self, entry_id: i64, new_index: i32) -> Result<()> {
        let entry = self
            .find_entry(entry_id)?
            .ok_or(LibraryError::NotFound("no such entry"))?;
        let tx = Transaction::begin(self.db)?;
        let target = self.items.move_target(Some(entry.playlist_id), new_index)?;
        // The shift must run before the moved row takes its new position.
        self.items
            .shift_for_move(Some(entry.playlist_id), entry.position, target)?;
        self.db.connection().execute(
            "UPDATE playlist_entries SET position=?2 WHERE id=?1;",
            params![entry_id, target],
        )?;
        tx.commit()
    }

    pub fn find_entry(&self, entry_id: i64) -> Result<Option<PlaylistEntry>> {
        self.db.query_optional(
            "SELECT id, playlist_id, track_id, position FROM playlist_entries WHERE id=?1;",
            params![entry_id],
            read_entry,
        )
    }

    /// Entries of a playlist, ordered by position.
    pub fn entries(&self, playlist_id: i64) -> Result<Vec<PlaylistEntry>> {
        self.db.query_rows(
            "SELECT id, playlist_id, track_id, position FROM playlist_entries \
             WHERE playlist_id=?1 ORDER BY position;",
            params![playlist_id],
            read_entry,
        )
    }

    fn accepts_children(&self, parent_id: Option<i64>) -> Result<bool> {
        match parent_id {
            None => Ok(true),
            Some(id) => Ok(self.find(id)?.is_some_and(|p| p.is_folder)),
        }
    }

    /// True when `new_parent_id` is `id` or one of its descendants.
    fn would_cycle(&self, id: i64, new_parent_id: Option<i64>) -> Result<bool> {
        let mut cursor = new_parent_id;
        while let Some(current) = cursor {
            if current == id {
                return Ok(true);
            }
            cursor = self.find(current)?.and_then(|p| p.parent_id);
        }
        Ok(false)
    }
}

fn read_playlist(row: &rusqlite::Row<'_>) -> rusqlite::Result<Playlist> {
    Ok(Playlist {
        id: row.get(0)?,
        parent_id: row.get(1)?,
        name: row.get(2)?,
        is_folder: row.get::<_, i64>(3)? != 0,
        position: row.get(4)?,
    })
}

fn read_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlaylistEntry> {
    Ok(PlaylistEntry {
        id: row.get(0)?,
        playlist_id: row.get(1)?,
        track_id: row.get(2)?,
        position: row.get(3)?,
    })
}
