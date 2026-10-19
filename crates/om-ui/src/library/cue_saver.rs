//! Keeps the hotcues of the track on each deck in the library. Any hotcue change (a pad, MIDI, a
//! plugin, a test) reaches the deck as a state change, so the saver compares each deck's hotcue
//! bank with what it last wrote or applied, and writes only the slots that differ. Control thread
//! only.

use om_core::param_id::HOTCUES_PER_DECK;
use om_library::{CueDao, CuePoint, Library, LibraryError};

/// One deck's hotcue bank, as the snapshot reports it.
pub type Cues = [Option<i64>; HOTCUES_PER_DECK as usize];

/// What a deck is bound to: the track, and the bank as last written or applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Binding {
    track_id: i64,
    last: Cues,
}

/// Per-deck write-back of hotcue edits to the loaded track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueSaver {
    bound: Vec<Option<Binding>>,
}

impl CueSaver {
    pub fn new(num_decks: usize) -> Self {
        Self {
            bound: vec![None; num_decks],
        }
    }

    /// Binds `deck` to `track_id`. `cues` is the bank the deck holds now, after the saved cues were
    /// applied, so loading them writes nothing back. Out-of-range decks are ignored.
    pub fn bind(&mut self, deck: usize, track_id: i64, cues: Cues) {
        if let Some(slot) = self.bound.get_mut(deck) {
            *slot = Some(Binding {
                track_id,
                last: cues,
            });
        }
    }

    /// Stops writing for `deck`, for example when the deck is unloaded.
    pub fn unbind(&mut self, deck: usize) {
        if let Some(slot) = self.bound.get_mut(deck) {
            *slot = None;
        }
    }

    /// Writes every hotcue slot of `deck` that differs from the bound bank, then remembers `cues`
    /// as the new baseline. A slot whose write fails keeps its old baseline, so the next call
    /// retries it. Returns how many slots were written; a deck with no bound track writes nothing.
    pub fn save_changes(
        &mut self,
        deck: usize,
        cues: Cues,
        library: &Library,
    ) -> Result<usize, LibraryError> {
        let Some(binding) = self.bound.get_mut(deck).and_then(Option::as_mut) else {
            return Ok(0);
        };
        let dao = CueDao::new(library.database());
        let mut written = 0;
        for (index, now) in cues.iter().copied().enumerate() {
            if now == binding.last[index] {
                continue;
            }
            let slot = i32::try_from(index).unwrap_or(i32::MAX);
            match now {
                Some(position_frames) => {
                    // Keep what the row already has (colour and label come from the library, not
                    // the deck), and write only the position.
                    let (colour_argb, label) = dao
                        .cues(binding.track_id)?
                        .into_iter()
                        .find(|c| c.slot == slot)
                        .map(|c| (c.colour_argb, c.label))
                        .unwrap_or_default();
                    dao.set_cue(&CuePoint {
                        track_id: binding.track_id,
                        slot,
                        position_frames,
                        colour_argb,
                        label,
                    })?;
                }
                None => dao.clear_cue(binding.track_id, slot)?,
            }
            binding.last[index] = now;
            written += 1;
        }
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_library::{core_migrations, OpenMode, TrackRecord};

    fn library_with_track() -> (Library, i64) {
        let lib = Library::open(":memory:", OpenMode::ReadWrite, core_migrations()).expect("open");
        let id = lib
            .tracks()
            .insert(&TrackRecord {
                path: "/m/a.wav".into(),
                ..TrackRecord::default()
            })
            .expect("insert");
        (lib, id)
    }

    fn saved(lib: &Library, id: i64) -> Vec<(i32, i64)> {
        lib.cues()
            .cues(id)
            .expect("cues")
            .into_iter()
            .map(|c| (c.slot, c.position_frames))
            .collect()
    }

    #[test]
    fn set_writes_a_row_and_clear_deletes_it() {
        let (lib, id) = library_with_track();
        let mut saver = CueSaver::new(2);
        saver.bind(0, id, [None; 8]);

        let mut bank = [None; 8];
        bank[3] = Some(44_100);
        assert_eq!(saver.save_changes(0, bank, &lib).unwrap(), 1);
        assert_eq!(saved(&lib, id), vec![(3, 44_100)]);

        bank[3] = None;
        assert_eq!(saver.save_changes(0, bank, &lib).unwrap(), 1);
        assert!(saved(&lib, id).is_empty(), "row deleted");
    }

    #[test]
    fn unchanged_bank_writes_nothing_and_moving_a_cue_replaces_it() {
        let (lib, id) = library_with_track();
        let mut saver = CueSaver::new(1);
        saver.bind(0, id, [None; 8]);
        let mut bank = [None; 8];
        bank[0] = Some(10);
        saver.save_changes(0, bank, &lib).unwrap();
        assert_eq!(saver.save_changes(0, bank, &lib).unwrap(), 0);

        bank[0] = Some(20);
        assert_eq!(saver.save_changes(0, bank, &lib).unwrap(), 1);
        assert_eq!(saved(&lib, id), vec![(0, 20)]);
    }

    #[test]
    fn existing_label_and_colour_survive_a_moved_cue() {
        let (lib, id) = library_with_track();
        lib.cues()
            .set_cue(&CuePoint {
                track_id: id,
                slot: 1,
                position_frames: 5,
                colour_argb: 0xFF00_FF00,
                label: "drop".into(),
            })
            .unwrap();
        let mut saver = CueSaver::new(1);
        let mut bank = [None; 8];
        bank[1] = Some(5);
        saver.bind(0, id, bank);

        bank[1] = Some(99);
        saver.save_changes(0, bank, &lib).unwrap();
        let cue = lib.cues().cues(id).unwrap().remove(0);
        assert_eq!((cue.position_frames, cue.colour_argb), (99, 0xFF00_FF00));
        assert_eq!(cue.label, "drop");
    }

    #[test]
    fn applied_cues_are_not_written_back_and_unbound_decks_write_nothing() {
        let (lib, id) = library_with_track();
        lib.cues()
            .set_cue(&CuePoint {
                track_id: id,
                slot: 4,
                position_frames: 7,
                ..CuePoint::default()
            })
            .unwrap();
        let mut saver = CueSaver::new(2);
        let mut applied = [None; 8];
        applied[4] = Some(7);
        saver.bind(0, id, applied);
        assert_eq!(saver.save_changes(0, applied, &lib).unwrap(), 0);
        assert_eq!(saver.save_changes(1, [Some(1); 8], &lib).unwrap(), 0);

        saver.unbind(0);
        assert_eq!(saver.save_changes(0, [None; 8], &lib).unwrap(), 0);
        assert_eq!(saved(&lib, id), vec![(4, 7)], "untouched");
    }
}
