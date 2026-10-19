//! Hotcues and saved loops per track. Positions are engine frames (same unit as the deck's hotcue
//! bank). Non-owning view of a `Database`; use from the thread that owns it.

use crate::database::Database;
use crate::error::Result;
use rusqlite::params;

/// A hotcue: one slot of a track.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CuePoint {
    pub track_id: i64,
    /// Hotcue index, 0-based.
    pub slot: i32,
    pub position_frames: i64,
    pub colour_argb: u32,
    pub label: String,
}

/// A saved loop: one slot of a track. `end_frames` must be greater than `start_frames`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SavedLoop {
    pub track_id: i64,
    pub slot: i32,
    pub start_frames: i64,
    pub end_frames: i64,
    pub colour_argb: u32,
    pub label: String,
}

/// Hotcue and loop reads and writes.
pub struct CueDao<'a> {
    db: &'a Database,
}

impl<'a> CueDao<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Inserts or replaces the cue in (track_id, slot). Fails for an unknown track.
    pub fn set_cue(&self, cue: &CuePoint) -> Result<()> {
        self.db.connection().execute(
            "INSERT OR REPLACE INTO cues (track_id, slot, position_frames, colour_argb, label) \
             VALUES (?1,?2,?3,?4,?5);",
            params![
                cue.track_id,
                cue.slot,
                cue.position_frames,
                i64::from(cue.colour_argb),
                cue.label,
            ],
        )?;
        Ok(())
    }

    pub fn clear_cue(&self, track_id: i64, slot: i32) -> Result<()> {
        self.db.connection().execute(
            "DELETE FROM cues WHERE track_id=?1 AND slot=?2;",
            params![track_id, slot],
        )?;
        Ok(())
    }

    /// The track's hotcues, ordered by slot.
    pub fn cues(&self, track_id: i64) -> Result<Vec<CuePoint>> {
        self.db.query_rows(
            "SELECT slot, position_frames, colour_argb, label FROM cues WHERE track_id=?1 ORDER BY slot;",
            params![track_id],
            |row| {
                Ok(CuePoint {
                    track_id,
                    slot: row.get(0)?,
                    position_frames: row.get(1)?,
                    colour_argb: row.get::<_, i64>(2)? as u32,
                    label: row.get(3)?,
                })
            },
        )
    }

    /// Inserts or replaces the loop in (track_id, slot). Fails for an unknown track or an inverted
    /// range.
    pub fn set_loop(&self, l: &SavedLoop) -> Result<()> {
        self.db.connection().execute(
            "INSERT OR REPLACE INTO loops (track_id, slot, start_frames, end_frames, colour_argb, label) \
             VALUES (?1,?2,?3,?4,?5,?6);",
            params![
                l.track_id,
                l.slot,
                l.start_frames,
                l.end_frames,
                i64::from(l.colour_argb),
                l.label,
            ],
        )?;
        Ok(())
    }

    pub fn clear_loop(&self, track_id: i64, slot: i32) -> Result<()> {
        self.db.connection().execute(
            "DELETE FROM loops WHERE track_id=?1 AND slot=?2;",
            params![track_id, slot],
        )?;
        Ok(())
    }

    /// The track's saved loops, ordered by slot.
    pub fn loops(&self, track_id: i64) -> Result<Vec<SavedLoop>> {
        self.db.query_rows(
            "SELECT slot, start_frames, end_frames, colour_argb, label FROM loops \
             WHERE track_id=?1 ORDER BY slot;",
            params![track_id],
            |row| {
                Ok(SavedLoop {
                    track_id,
                    slot: row.get(0)?,
                    start_frames: row.get(1)?,
                    end_frames: row.get(2)?,
                    colour_argb: row.get::<_, i64>(3)? as u32,
                    label: row.get(4)?,
                })
            },
        )
    }
}
