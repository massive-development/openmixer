//! Rekordbox XML export (`DJ_PLAYLISTS`): `COLLECTION/TRACK` with `TEMPO` and `POSITION_MARK`
//! cues, and the `PLAYLISTS` node tree (folders and playlists). `POSITION_MARK` Type 0 = cue
//! (Num -1 memory cue, 0..7 hotcue), Type 4 = loop; other types (fade, load) are ignored.
//! Times are seconds.

use crate::file_url::file_url_to_path;
use crate::import_model::{
    CueKind, ImportReport, ImportedCue, ImportedLibrary, ImportedPlaylist, ImportedTrack,
};
use crate::lenient::iso8601_ms;
use crate::library_importer::LibraryImporter;
use crate::track_key::normalise_key;
use crate::track_record::TrackRecord;
use crate::xml::XmlElement;
use std::collections::BTreeMap;

const MARK_CUE: i64 = 0;
const MARK_LOOP: i64 = 4;
/// Rekordbox stores stars as 0, 51, 102, 153, 204, 255.
const RATING_STEP: i64 = 51;
const MAX_STARS: i64 = 5;

pub struct RekordboxImporter;

impl LibraryImporter for RekordboxImporter {
    fn name(&self) -> &str {
        "Rekordbox XML"
    }

    fn recognises(&self, root: &XmlElement) -> bool {
        root.has_tag_name("DJ_PLAYLISTS")
    }

    fn parse(
        &self,
        root: &XmlElement,
        out: &mut ImportedLibrary,
        report: &mut ImportReport,
    ) -> Result<(), String> {
        let mut by_path: BTreeMap<String, String> = BTreeMap::new();
        if let Some(collection) = root.child_by_name("COLLECTION") {
            for track in collection.children_named("TRACK") {
                read_track(track, out, report, &mut by_path);
            }
        }
        let top = root
            .child_by_name("PLAYLISTS")
            .and_then(|p| p.child_by_name("NODE"));
        // The single ROOT node is only a container.
        if let Some(top) = top {
            out.playlists = read_node(top, &by_path).children;
        }
        Ok(())
    }
}

fn colour_of(mark: &XmlElement) -> u32 {
    if !mark.has_attribute("Red") {
        return 0;
    }
    0xff00_0000
        | (((mark.int_attribute("Red", 0) & 0xff) as u32) << 16)
        | (((mark.int_attribute("Green", 0) & 0xff) as u32) << 8)
        | ((mark.int_attribute("Blue", 0) & 0xff) as u32)
}

/// False for marks that have no library home (fades, load points).
fn read_mark(mark: &XmlElement) -> Option<ImportedCue> {
    let mark_type = mark.int_attribute("Type", -1);
    if mark_type != MARK_CUE && mark_type != MARK_LOOP {
        return None;
    }
    Some(ImportedCue {
        kind: if mark_type == MARK_LOOP {
            CueKind::Loop
        } else {
            CueKind::Cue
        },
        num: mark.int_attribute("Num", -1) as i32,
        start_sec: mark.double_attribute("Start"),
        end_sec: mark.double_attribute("End"),
        label: mark.string_attribute("Name"),
        colour_argb: colour_of(mark),
    })
}

fn read_marks(track: &XmlElement, out: &mut ImportedTrack) {
    for mark in track.children_named("POSITION_MARK") {
        if let Some(cue) = read_mark(mark) {
            out.cues.push(cue);
        }
    }
}

/// `AverageBpm` when set, else the `TEMPO` element's `Bpm`, else 0.
fn bpm_of(track: &XmlElement) -> f64 {
    let avg = track.double_attribute("AverageBpm");
    if avg > 0.0 {
        return avg;
    }
    track
        .child_by_name("TEMPO")
        .map_or(0.0, |tempo| tempo.double_attribute("Bpm"))
}

fn fill_record(e: &XmlElement, r: &mut TrackRecord) {
    r.path = file_url_to_path(&e.string_attribute("Location")).unwrap_or_default();
    r.title = e.string_attribute("Name");
    r.artist = e.string_attribute("Artist");
    r.album = e.string_attribute("Album");
    r.genre = e.string_attribute("Genre");
    r.comment = e.string_attribute("Comments");
    r.key = e.string_attribute("Tonality");
    r.year = e.int_attribute("Year", 0) as i32;
    r.duration_sec = e.double_attribute("TotalTime");
    r.play_count = e.int_attribute("PlayCount", 0).max(0) as i32;
    let stars = (e.int_attribute("Rating", 0) + RATING_STEP / 2) / RATING_STEP;
    r.rating = stars.clamp(0, MAX_STARS) as i32;
    r.date_added_ms = if e.has_attribute("DateAdded") {
        iso8601_ms(&e.string_attribute("DateAdded"))
    } else {
        0
    };
    let bpm = bpm_of(e);
    if bpm > 0.0 {
        r.bpm = Some(bpm);
    }
    // Why: `key:` search matches the Camelot column, so both forms must be filled.
    normalise_key(r);
}

fn read_track(
    e: &XmlElement,
    out: &mut ImportedLibrary,
    report: &mut ImportReport,
    by_path: &mut BTreeMap<String, String>,
) {
    let mut track = ImportedTrack {
        key: e.string_attribute("TrackID"),
        sample_rate: e.double_attribute("SampleRate"),
        ..ImportedTrack::default()
    };
    fill_record(e, &mut track.record);
    if track.record.path.is_empty() {
        report.tracks_skipped += 1;
        report.note(format!(
            "skipped track {}: no local file location",
            track.key
        ));
        return;
    }
    read_marks(e, &mut track);
    by_path.insert(track.record.path.clone(), track.key.clone());
    out.tracks.push(track);
}

/// `KeyType` 0 refers to `TrackID`, 1 to the track's `Location`.
fn resolve_key(
    reference: &XmlElement,
    by_location: bool,
    by_path: &BTreeMap<String, String>,
) -> String {
    let key = reference.string_attribute("Key");
    if !by_location {
        return key;
    }
    let path = file_url_to_path(&key).unwrap_or_default();
    by_path.get(&path).cloned().unwrap_or_default()
}

fn read_node(node: &XmlElement, by_path: &BTreeMap<String, String>) -> ImportedPlaylist {
    let by_location = node.int_attribute("KeyType", 0) == 1;
    let mut p = ImportedPlaylist {
        name: node.string_attribute("Name"),
        is_folder: node.int_attribute("Type", 0) == 0,
        ..ImportedPlaylist::default()
    };
    for child in node.children() {
        if child.has_tag_name("NODE") {
            p.children.push(read_node(child, by_path));
        } else if child.has_tag_name("TRACK") {
            p.track_keys.push(resolve_key(child, by_location, by_path));
        }
    }
    p
}
