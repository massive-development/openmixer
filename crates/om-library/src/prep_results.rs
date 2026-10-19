//! Turns finished analysis payloads into track metadata. Runs on the caller's thread with the
//! caller's `TrackDao`. The schema has no manual or locked flag for BPM or key, so analysis
//! overwrites. Port of `src/library/PrepResults.*`.

use crate::error::{LibraryError, Result};
use crate::track_dao::TrackDao;
use om_analysis::{BpmResult, KeyResult, MusicalKey};
use std::collections::BTreeMap;

fn parse_bpm(results: &BTreeMap<String, String>) -> Option<f64> {
    let parsed = BpmResult::from_json(results.get("bpm")?)?;
    (parsed.bpm > 0.0).then_some(parsed.bpm)
}

fn parse_key(results: &BTreeMap<String, String>) -> Option<MusicalKey> {
    let parsed = KeyResult::from_json(results.get("key")?)?;
    parsed.tonal.then_some(parsed.key)
}

/// Writes the BPM and key found in `results` (analyser id -> JSON). Missing or unusable entries
/// leave the stored value alone. A track that no longer exists is an error.
pub fn apply_prep_results(
    tracks: &TrackDao<'_>,
    track_id: i64,
    results: &BTreeMap<String, String>,
) -> Result<()> {
    if tracks.find_by_id(track_id)?.is_none() {
        return Err(LibraryError::NotFound("track no longer exists"));
    }
    let bpm = parse_bpm(results);
    let key = parse_key(results);
    if bpm.is_none() && key.is_none() {
        return Ok(());
    }
    // Narrow write: a whole-row update would race the scan connection.
    tracks.update_analysis(track_id, bpm, key.as_ref())
}
