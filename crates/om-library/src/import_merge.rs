//! Merge policy for tracks that already exist locally (matched by path). Local data wins:
//!  - text, year, duration, BPM, key, last-played: the imported value fills a field only when the
//!    local one is empty or unset;
//!  - rating and play count: the imported value is taken only when the local one is still 0;
//!  - id, path, content hash, analysis and date added are never touched.

use crate::track_key::normalise_key;
use crate::track_record::TrackRecord;

fn fill_text(local: &mut String, imported: &str) -> bool {
    if !local.is_empty() || imported.is_empty() {
        return false;
    }
    local.clear();
    local.push_str(imported);
    true
}

fn fill_default<T: PartialEq + Default + Clone>(local: &mut T, imported: &T) -> bool {
    if *local != T::default() || *imported == T::default() {
        return false;
    }
    *local = imported.clone();
    true
}

fn fill_optional<T: Clone>(local: &mut Option<T>, imported: &Option<T>) -> bool {
    if local.is_some() || imported.is_none() {
        return false;
    }
    *local = imported.clone();
    true
}

/// Re-derives key and Camelot from whichever is set; reports whether either changed.
fn complete_key(local: &mut TrackRecord) -> bool {
    let before = (local.key.clone(), local.camelot.clone());
    // Why: a filled key needs its Camelot code, or key: search misses it.
    normalise_key(local);
    before != (local.key.clone(), local.camelot.clone())
}

/// Applies the policy to `local`; true when anything changed.
pub fn merge_track_record(local: &mut TrackRecord, imported: &TrackRecord) -> bool {
    let mut changed = false;
    changed |= fill_text(&mut local.title, &imported.title);
    changed |= fill_text(&mut local.artist, &imported.artist);
    changed |= fill_text(&mut local.album, &imported.album);
    changed |= fill_text(&mut local.genre, &imported.genre);
    changed |= fill_text(&mut local.comment, &imported.comment);
    changed |= fill_text(&mut local.key, &imported.key);
    changed |= fill_default(&mut local.year, &imported.year);
    changed |= fill_default(&mut local.duration_sec, &imported.duration_sec);
    changed |= fill_default(&mut local.rating, &imported.rating);
    changed |= fill_default(&mut local.play_count, &imported.play_count);
    changed |= fill_optional(&mut local.bpm, &imported.bpm);
    changed |= complete_key(local);
    changed |= fill_optional(&mut local.last_played_ms, &imported.last_played_ms);
    changed
}
