//! Writes an [`ImportedLibrary`] into the database in one transaction, merging by path (see
//! `import_merge`). Any database failure rolls everything back, so a half-imported library never
//! exists. Playlists merge by name under the same parent, so re-importing adds no duplicates.
//! Runs on the thread that owns the `Library`.

use crate::error::Result;
use crate::import_cues::import_cues;
use crate::import_merge::merge_track_record;
use crate::import_model::{ImportReport, ImportedLibrary, ImportedPlaylist, ImportedTrack};
use crate::library::Library;
use crate::track_record::TrackRecord;
use std::collections::{BTreeMap, BTreeSet};

pub struct ImportWriter<'a> {
    library: &'a Library,
    report: &'a mut ImportReport,
    /// Source track key -> local track id, for resolving playlist entries.
    ids_by_key: BTreeMap<String, i64>,
}

impl<'a> ImportWriter<'a> {
    pub fn new(library: &'a Library, report: &'a mut ImportReport) -> Self {
        Self {
            library,
            report,
            ids_by_key: BTreeMap::new(),
        }
    }

    /// On failure the library is unchanged; counts from this write are dropped and one error is
    /// added to the report.
    pub fn write(&mut self, data: &ImportedLibrary) -> Result<()> {
        let before = self.report.clone();
        match self.write_all(data) {
            Ok(()) => Ok(()),
            Err(e) => {
                *self.report = before;
                self.report.errors += 1;
                self.report.note(e.to_string());
                Err(e)
            }
        }
    }

    fn write_all(&mut self, data: &ImportedLibrary) -> Result<()> {
        // Dropped (rolled back) on any early return below.
        let tx = crate::database::Transaction::begin(self.library.database())?;
        for track in &data.tracks {
            self.write_track(track)?;
        }
        self.write_playlists(&data.playlists, None)?;
        tx.commit()
    }

    fn write_track(&mut self, track: &ImportedTrack) -> Result<()> {
        let tracks = self.library.tracks();
        // A failed lookup propagates; it must not be read as "new".
        let id = match tracks.find_by_path(&track.record.path)? {
            Some(mut existing) => {
                self.update_existing(&mut existing, &track.record)?;
                self.report.tracks_merged += 1;
                existing.id
            }
            None => {
                let id = tracks.insert(&track.record)?;
                self.report.tracks_added += 1;
                id
            }
        };
        self.ids_by_key.insert(track.key.clone(), id);
        import_cues(&self.library.cues(), id, track, self.report)
    }

    fn update_existing(&self, local: &mut TrackRecord, imported: &TrackRecord) -> Result<()> {
        if !merge_track_record(local, imported) {
            return Ok(());
        }
        let dao = self.library.tracks();
        // update() leaves user data alone, so the rating and play stats are written separately.
        dao.update(local)?;
        dao.set_rating(local.id, local.rating)?;
        dao.set_play_stats(local.id, local.play_count, local.last_played_ms)
    }

    fn write_playlists(&mut self, nodes: &[ImportedPlaylist], parent: Option<i64>) -> Result<()> {
        for node in nodes {
            self.write_playlist(node, parent)?;
        }
        Ok(())
    }

    fn write_playlist(&mut self, node: &ImportedPlaylist, parent: Option<i64>) -> Result<()> {
        let playlists = self.library.playlists();
        // Last match wins.
        let existing = playlists
            .children(parent)?
            .into_iter()
            .filter(|s| s.name == node.name && s.is_folder == node.is_folder)
            .next_back();
        let id = match existing {
            Some(sibling) => sibling.id,
            None => {
                let id = playlists.create(&node.name, parent, node.is_folder)?;
                self.report.playlists_created += 1;
                id
            }
        };
        if node.is_folder {
            self.write_playlists(&node.children, Some(id))
        } else {
            self.fill_playlist(id, node)
        }
    }

    fn fill_playlist(&mut self, playlist_id: i64, node: &ImportedPlaylist) -> Result<()> {
        let playlists = self.library.playlists();
        let mut present: BTreeSet<i64> = playlists
            .entries(playlist_id)?
            .iter()
            .map(|e| e.track_id)
            .collect();
        for key in &node.track_keys {
            let Some(&track_id) = self.ids_by_key.get(key) else {
                self.report.entries_skipped += 1;
                continue;
            };
            // Already there from an earlier import.
            if !present.insert(track_id) {
                continue;
            }
            playlists.add_track(playlist_id, track_id, -1)?;
            self.report.entries_added += 1;
        }
        Ok(())
    }
}
