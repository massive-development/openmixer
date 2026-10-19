//! Adds imported cues and loops to a track without disturbing local ones (merge policy in
//! `import_merge`). Hotcue `num` n goes to slot n; memory cues and loops (`num` -1) go to the
//! first free slot from 8 up. Times are seconds, converted to engine frames with the track's
//! sample rate (44100 when unknown).

use crate::cue_dao::{CueDao, CuePoint, SavedLoop};
use crate::error::Result;
use crate::import_model::{CueKind, ImportReport, ImportedCue, ImportedTrack};
use std::collections::BTreeSet;

pub const FIRST_MEMORY_SLOT: i32 = 8;
pub const ASSUMED_SAMPLE_RATE: f64 = 44_100.0;

/// Tracks which slots and positions are taken so imports never overwrite or duplicate.
#[derive(Default)]
struct SlotPlan {
    slots: BTreeSet<i32>,
    positions: BTreeSet<i64>,
}

impl SlotPlan {
    fn mark_used(&mut self, slot: i32, position: i64) {
        self.slots.insert(slot);
        self.positions.insert(position);
    }

    /// The slot to write, or `None` when local data already covers this cue.
    fn claim(&mut self, num: i32, position: i64) -> Option<i32> {
        if self.positions.contains(&position) {
            return None;
        }
        let mut slot = if num >= 0 { num } else { FIRST_MEMORY_SLOT };
        while num < 0 && self.slots.contains(&slot) {
            slot += 1;
        }
        if self.slots.contains(&slot) {
            return None;
        }
        self.mark_used(slot, position);
        Some(slot)
    }
}

fn to_frames(seconds: f64, rate: f64) -> i64 {
    (seconds * rate).round() as i64
}

fn is_valid(c: &ImportedCue, start: i64, end: i64) -> bool {
    let loop_ok = c.kind != CueKind::Loop || end > start;
    start >= 0 && loop_ok && c.num >= -1
}

/// Counts added cues in `report.cues_imported` and malformed ones in `report.errors`.
pub fn import_cues(
    dao: &CueDao<'_>,
    track_id: i64,
    track: &ImportedTrack,
    report: &mut ImportReport,
) -> Result<()> {
    let mut cue_plan = SlotPlan::default();
    let mut loop_plan = SlotPlan::default();
    for c in dao.cues(track_id)? {
        cue_plan.mark_used(c.slot, c.position_frames);
    }
    for l in dao.loops(track_id)? {
        loop_plan.mark_used(l.slot, l.start_frames);
    }
    let rate = if track.sample_rate > 0.0 {
        track.sample_rate
    } else {
        ASSUMED_SAMPLE_RATE
    };
    // Hotcues first: a memory cue at the same position must not shadow the hotcue.
    let (hot, memory): (Vec<&ImportedCue>, Vec<&ImportedCue>) =
        track.cues.iter().partition(|c| c.num >= 0);
    for c in hot.into_iter().chain(memory) {
        let start = to_frames(c.start_sec, rate);
        let end = to_frames(c.end_sec, rate);
        if !is_valid(c, start, end) {
            report.errors += 1;
            report.note(format!("invalid cue in {}", track.record.path));
            continue;
        }
        let plan = if c.kind == CueKind::Loop {
            &mut loop_plan
        } else {
            &mut cue_plan
        };
        let Some(slot) = plan.claim(c.num, start) else {
            continue;
        };
        store(dao, track_id, c, slot, start, end)?;
        report.cues_imported += 1;
    }
    Ok(())
}

fn store(
    dao: &CueDao<'_>,
    track_id: i64,
    c: &ImportedCue,
    slot: i32,
    start: i64,
    end: i64,
) -> Result<()> {
    if c.kind == CueKind::Loop {
        dao.set_loop(&SavedLoop {
            track_id,
            slot,
            start_frames: start,
            end_frames: end,
            colour_argb: c.colour_argb,
            label: c.label.clone(),
        })
    } else {
        dao.set_cue(&CuePoint {
            track_id,
            slot,
            position_frames: start,
            colour_argb: c.colour_argb,
            label: c.label.clone(),
        })
    }
}
