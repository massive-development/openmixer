//! Dense 0..n-1 `position` column maintenance for rows grouped by a scope column (library-internal).
//! Shared by playlist nesting (scope = `parent_id`) and playlist entries (scope = `playlist_id`).

use crate::database::Database;
use crate::error::Result;
use rusqlite::params;

/// Position maintenance for one (table, scope column) pair. Both names are code constants.
pub struct OrderedList<'a> {
    db: &'a Database,
    table: &'static str,
    scope: &'static str,
}

impl<'a> OrderedList<'a> {
    pub fn new(db: &'a Database, table: &'static str, scope: &'static str) -> Self {
        Self { db, table, scope }
    }

    /// Rows in `scope_id` (`None` selects the NULL scope, for top-level playlists).
    pub fn count(&self, scope_id: Option<i64>) -> Result<i32> {
        let sql = format!(
            "SELECT COUNT(*) FROM {} WHERE {} IS ?1;",
            self.table, self.scope
        );
        Ok(self
            .db
            .connection()
            .query_row(&sql, params![scope_id], |row| row.get(0))?)
    }

    /// Clamps to `[0, count]`; a negative request appends.
    pub fn insert_index(&self, scope_id: Option<i64>, requested: i32) -> Result<i32> {
        let n = self.count(scope_id)?;
        Ok(if requested < 0 { n } else { requested.min(n) })
    }

    /// Clamps to `[0, count - 1]`.
    pub fn move_target(&self, scope_id: Option<i64>, requested: i32) -> Result<i32> {
        let last = (self.count(scope_id)? - 1).max(0);
        Ok(requested.clamp(0, last))
    }

    /// Makes room at `index` by shifting every later row up by one.
    pub fn open_gap(&self, scope_id: Option<i64>, index: i32) -> Result<()> {
        self.shift(scope_id, 1, index, i32::MAX)
    }

    /// Fills the hole left at `position` by shifting every later row down by one.
    pub fn close_gap(&self, scope_id: Option<i64>, position: i32) -> Result<()> {
        self.shift(scope_id, -1, position + 1, i32::MAX)
    }

    /// Shifts the rows between the old and new index. The caller then stores `new_pos` on the
    /// moved row.
    pub fn shift_for_move(&self, scope_id: Option<i64>, old_pos: i32, new_pos: i32) -> Result<()> {
        if old_pos < new_pos {
            self.shift(scope_id, -1, old_pos + 1, new_pos)
        } else if old_pos > new_pos {
            self.shift(scope_id, 1, new_pos, old_pos - 1)
        } else {
            Ok(())
        }
    }

    fn shift(&self, scope_id: Option<i64>, delta: i32, from: i32, to: i32) -> Result<()> {
        let sql = format!(
            "UPDATE {} SET position = position + ?2 WHERE {} IS ?1 AND position BETWEEN ?3 AND ?4;",
            self.table, self.scope
        );
        self.db
            .connection()
            .execute(&sql, params![scope_id, delta, from, to])?;
        Ok(())
    }
}
