//! Persisted track prep queue: which tracks are waiting to be analysed, in what order, and how
//! each ended. Processing order is priority (highest first), then position; position is the
//! user's list order. Non-owning view of a `Database`; use from the thread that owns it.
//! Port of `src/library/PrepQueueDao.*`.

use crate::database::{Database, Transaction};
use crate::error::{LibraryError, Result};
use rusqlite::{params, Row};

/// Where a queued track is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrepStatus {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl PrepStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "running" => Self::Running,
            "done" => Self::Done,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => Self::Queued,
        }
    }
}

/// One row of the prep queue.
#[derive(Debug, Clone, PartialEq)]
pub struct PrepEntry {
    pub track_id: i64,
    /// User order, dense 0..n-1 after `move_to`.
    pub position: i32,
    /// Above 0 jumps ahead of normal entries.
    pub priority: i32,
    pub status: PrepStatus,
    /// 0..1. Live only in `PrepQueue::list`; 1 once Done.
    pub progress: f32,
    pub error: String,
}

const COLUMNS: &str = "track_id, position, priority, status, error";

fn read_entry(row: &Row<'_>) -> rusqlite::Result<PrepEntry> {
    let status = PrepStatus::parse(&row.get::<_, String>(3)?);
    Ok(PrepEntry {
        track_id: row.get(0)?,
        position: row.get(1)?,
        priority: row.get(2)?,
        status,
        progress: if status == PrepStatus::Done { 1.0 } else { 0.0 },
        error: row.get(4)?,
    })
}

/// Prep queue reads and writes.
pub struct PrepQueueDao<'a> {
    db: &'a Database,
}

impl<'a> PrepQueueDao<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Appends at the end; an existing finished entry is re-queued. A running entry is left
    /// alone. Fails if the track does not exist.
    pub fn enqueue(&self, track_id: i64) -> Result<()> {
        self.db.connection().execute(
            "INSERT INTO prep_queue (track_id, position, priority, status) \
             VALUES (?1, (SELECT COALESCE(MAX(position), -1) + 1 FROM prep_queue), 0, 'queued') \
             ON CONFLICT(track_id) DO UPDATE SET position=excluded.position, priority=0, \
             status='queued', error='' WHERE prep_queue.status != 'running';",
            params![track_id],
        )?;
        Ok(())
    }

    /// Runs next among queued entries (the latest call goes first).
    pub fn prioritise(&self, track_id: i64) -> Result<()> {
        self.db.connection().execute(
            "UPDATE prep_queue SET priority=(SELECT COALESCE(MAX(priority), 0) + 1 FROM prep_queue) \
             WHERE track_id=?1 AND status='queued';",
            params![track_id],
        )?;
        Ok(())
    }

    /// Moves within the user order; clamps to the list.
    pub fn move_to(&self, track_id: i64, index: i32) -> Result<()> {
        let mut ids = self.ordered_ids()?;
        let from = ids
            .iter()
            .position(|&id| id == track_id)
            .ok_or(LibraryError::Rejected("track is not in the prep queue"))?;
        ids.remove(from);
        let at = index.clamp(0, ids.len() as i32) as usize;
        ids.insert(at, track_id);

        // All positions or none.
        let tx = Transaction::begin(self.db)?;
        for (position, id) in ids.iter().enumerate() {
            self.db.connection().execute(
                "UPDATE prep_queue SET position=?2 WHERE track_id=?1;",
                params![id, position as i64],
            )?;
        }
        tx.commit()
    }

    pub fn remove(&self, track_id: i64) -> Result<()> {
        self.db.connection().execute(
            "DELETE FROM prep_queue WHERE track_id=?1;",
            params![track_id],
        )?;
        Ok(())
    }

    /// Drops Done, Failed and Cancelled rows.
    pub fn clear_finished(&self) -> Result<()> {
        self.db
            .exec("DELETE FROM prep_queue WHERE status IN ('done', 'failed', 'cancelled');")
    }

    pub fn set_status(&self, track_id: i64, status: PrepStatus, error: &str) -> Result<()> {
        self.db.connection().execute(
            "UPDATE prep_queue SET status=?2, error=?3 WHERE track_id=?1;",
            params![track_id, status.as_str(), error],
        )?;
        Ok(())
    }

    /// Crash recovery: Running -> Queued.
    pub fn reset_running(&self) -> Result<()> {
        self.db
            .exec("UPDATE prep_queue SET status='queued' WHERE status='running';")
    }

    pub fn find(&self, track_id: i64) -> Result<Option<PrepEntry>> {
        self.db.query_optional(
            &format!("SELECT {COLUMNS} FROM prep_queue WHERE track_id=?1;"),
            params![track_id],
            read_entry,
        )
    }

    /// Every entry, in user order.
    pub fn all(&self) -> Result<Vec<PrepEntry>> {
        self.db.query_rows(
            &format!("SELECT {COLUMNS} FROM prep_queue ORDER BY position, id;"),
            [],
            read_entry,
        )
    }

    /// The entry that runs next, if any.
    pub fn next_queued(&self) -> Result<Option<PrepEntry>> {
        self.db.query_optional(
            &format!(
                "SELECT {COLUMNS} FROM prep_queue WHERE status='queued' \
                 ORDER BY priority DESC, position, id LIMIT 1;"
            ),
            [],
            read_entry,
        )
    }

    fn ordered_ids(&self) -> Result<Vec<i64>> {
        self.db.query_rows(
            "SELECT track_id FROM prep_queue ORDER BY position, id;",
            [],
            |row| row.get(0),
        )
    }
}
