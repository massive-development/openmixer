//! iTunes / Apple Music `Library.xml` (property list): tracks, ratings, play counts, playlists
//! and playlist folders. Cues do not exist in this format. Remote and file-less tracks are
//! skipped.

use crate::error::ImportError;
use crate::file_url::file_url_to_path;
use crate::import_model::{ImportReport, ImportedLibrary, ImportedPlaylist, ImportedTrack};
use crate::library_importer::LibraryImporter;
use crate::plist_reader::PlistDict;
use crate::track_record::TrackRecord;
use crate::xml::XmlElement;
use std::collections::BTreeSet;

const MAX_STARS: i64 = 5;

pub struct ItunesImporter;

impl LibraryImporter for ItunesImporter {
    fn name(&self) -> &str {
        "iTunes / Music Library.xml"
    }

    fn recognises(&self, root: &XmlElement) -> bool {
        root_dict(root).is_some_and(|dict| PlistDict::new(dict).has("Tracks"))
    }

    fn parse(
        &self,
        root: &XmlElement,
        out: &mut ImportedLibrary,
        report: &mut ImportReport,
    ) -> Result<(), ImportError> {
        let dict = root_dict(root).ok_or(ImportError::Plist)?;
        let top = PlistDict::new(dict);
        read_tracks(&top, out, report);
        let flat = read_playlists(&top);
        let known: BTreeSet<String> = flat.iter().map(|f| f.persistent_id.clone()).collect();
        out.playlists = assemble(&flat, "", &known);
        Ok(())
    }
}

fn root_dict(root: &XmlElement) -> Option<&XmlElement> {
    if root.has_tag_name("plist") {
        root.child_by_name("dict")
    } else {
        None
    }
}

/// iTunes stores 0..100 in steps of 20; computed ratings are derived from the album, not user
/// intent, so they count as unrated.
fn stars_from(d: &PlistDict<'_>) -> i32 {
    if d.boolean("Rating Computed") {
        return 0;
    }
    ((d.integer("Rating", 0) + 10) / 20).clamp(0, MAX_STARS) as i32
}

/// Fills `r` from one track dict. False when the track has no local file.
fn fill_record(d: &PlistDict<'_>, r: &mut TrackRecord) -> bool {
    if d.string("Track Type") == "URL" {
        return false;
    }
    r.path = file_url_to_path(&d.string("Location")).unwrap_or_default();
    r.title = d.string("Name");
    r.artist = d.string("Artist");
    r.album = d.string("Album");
    r.genre = d.string("Genre");
    r.comment = d.string("Comments");
    r.year = d.integer("Year", 0) as i32;
    r.duration_sec = d.integer("Total Time", 0) as f64 / 1000.0;
    r.rating = stars_from(d);
    r.play_count = d.integer("Play Count", 0).max(0) as i32;
    r.date_added_ms = d.date_ms("Date Added");
    let played = d.date_ms("Play Date UTC");
    if played > 0 {
        r.last_played_ms = Some(played);
    }
    let bpm = d.integer("BPM", 0);
    if bpm > 0 {
        r.bpm = Some(bpm as f64);
    }
    !r.path.is_empty()
}

fn read_tracks(root: &PlistDict<'_>, out: &mut ImportedLibrary, report: &mut ImportReport) {
    let Some(tracks) = root.find("Tracks") else {
        return;
    };
    let tracks_dict = PlistDict::new(tracks);
    for (id, value) in tracks_dict.entries() {
        let dict = PlistDict::new(value);
        let mut track = ImportedTrack {
            key: id.clone(),
            sample_rate: dict.integer("Sample Rate", 0) as f64,
            ..ImportedTrack::default()
        };
        if fill_record(&dict, &mut track.record) {
            out.tracks.push(track);
        } else {
            report.tracks_skipped += 1;
            report.note(format!("skipped track {id}: no local file location"));
        }
    }
}

struct FlatPlaylist {
    persistent_id: String,
    parent_id: String,
    node: ImportedPlaylist,
}

fn is_built_in(d: &PlistDict<'_>) -> bool {
    d.boolean("Master") || (d.has("Distinguished Kind") && !d.boolean("Folder"))
}

fn read_playlist(d: &PlistDict<'_>) -> FlatPlaylist {
    let mut node = ImportedPlaylist {
        name: d.string("Name"),
        is_folder: d.boolean("Folder"),
        ..ImportedPlaylist::default()
    };
    let is_folder = node.is_folder;
    if let Some(items) = d.find("Playlist Items").filter(|_| !is_folder) {
        for item in items.children() {
            let track_id = PlistDict::new(item).integer("Track ID", 0);
            node.track_keys.push(track_id.to_string());
        }
    }
    FlatPlaylist {
        persistent_id: d.string("Playlist Persistent ID"),
        parent_id: d.string("Parent Persistent ID"),
        node,
    }
}

fn read_playlists(root: &PlistDict<'_>) -> Vec<FlatPlaylist> {
    let Some(list) = root.find("Playlists") else {
        return Vec::new();
    };
    list.children()
        .iter()
        .map(PlistDict::new)
        .filter(|d| !is_built_in(d))
        .map(|d| read_playlist(&d))
        .collect()
}

/// Children of `parent_id` in file order. A playlist whose parent is unknown becomes top level.
fn assemble(
    flat: &[FlatPlaylist],
    parent_id: &str,
    known: &BTreeSet<String>,
) -> Vec<ImportedPlaylist> {
    let mut out = Vec::new();
    for f in flat {
        let orphan =
            parent_id.is_empty() && !f.parent_id.is_empty() && !known.contains(&f.parent_id);
        if f.parent_id != parent_id && !orphan {
            continue;
        }
        let mut node = f.node.clone();
        if node.is_folder && !f.persistent_id.is_empty() {
            node.children = assemble(flat, &f.persistent_id, known);
        }
        out.push(node);
    }
    out
}
