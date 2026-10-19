//! Loads library tracks into decks. The file goes to the engine deck; the track metadata, the cached
//! waveform and the cached beat grid follow. A track that is not analysed yet is queued in the prep
//! queue with priority, and when the queue reports it done the deck is refreshed from the library.
//!
//! The waveform and track metadata are kept per deck for the deck widgets to read ([`Self::waveform`],
//! [`Self::track_info`]); the beat grid is set on the engine, which owns the clock. The loader holds
//! a subscription to the services and must be pumped ([`Self::pump`]) after the services are.
//!
//! A load also restores the track's saved hotcues into the deck, and each pump writes the hotcue
//! edits back ([`CueSaver`]) and counts playing time for the play history ([`PlayTracker`]).

use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use om_analysis::WaveformData;
use om_core::param_id::HOTCUES_PER_DECK;
use om_engine::grid::BeatGrid;
use om_engine::DjEngine;
use om_library::{Library, LibraryError, PrepEntry, PrepStatus, TrackRecord};

use super::cue_saver::{CueSaver, Cues};
use super::play_tracker::PlayTracker;
use super::services::{LibraryEvent, LibraryServices, TrackAnalysis};
use crate::widgets::deck::TrackInfo;

/// Why a track could not be loaded into a deck. The display text is shown in the status line.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// Nothing is wired to load tracks (no deck handler).
    #[error("loading is not available")]
    Unavailable,
    #[error("unknown track {0}")]
    UnknownTrack(i64),
    #[error("no such deck")]
    NoSuchDeck,
    /// A playing deck refuses a load unless forced.
    #[error("deck is playing")]
    DeckPlaying,
    #[error("file is missing")]
    FileMissing,
    #[error("cannot decode {file}")]
    Decode { file: String },
    #[error(transparent)]
    Library(#[from] LibraryError),
}

#[derive(Default)]
struct Slot {
    track: Option<TrackRecord>,
    /// Queued for analysis; refreshed when the queue reports it done.
    awaiting_analysis: bool,
    waveform: Option<Arc<WaveformData>>,
    /// The grid the deck was given for `track`; put back if a later load fails.
    grid: Option<Arc<dyn BeatGrid>>,
}

pub struct TrackLoader {
    slots: Vec<Slot>,
    events: Receiver<LibraryEvent>,
    plays: Vec<PlayTracker>,
    cues: CueSaver,
    /// When the previous pump ran; the playing time credited to the decks is measured from here.
    last_pump: Option<Instant>,
}

impl TrackLoader {
    /// A loader for `num_decks` decks. Subscribes to `services` for analysis completions.
    pub fn new(services: &mut LibraryServices, num_decks: usize) -> Self {
        Self {
            slots: (0..num_decks).map(|_| Slot::default()).collect(),
            events: services.subscribe(),
            plays: vec![PlayTracker::default(); num_decks],
            cues: CueSaver::new(num_decks),
            last_pump: None,
        }
    }

    /// Loads a library track by id into `deck`. See [`Self::load_track`] for `force`.
    pub fn load(
        &mut self,
        engine: &DjEngine,
        services: &mut LibraryServices,
        deck: usize,
        track_id: i64,
        force: bool,
    ) -> Result<(), LoadError> {
        let track = services
            .library()
            .tracks()
            .find_by_id(track_id)?
            .ok_or(LoadError::UnknownTrack(track_id))?;
        self.load_track(engine, services, deck, &track, force)
    }

    /// Loads a track into `deck`. A missing file or an undecodable one fails without touching the
    /// deck. A track without cached analysis is queued with priority; the deck is loaded and
    /// playable either way.
    ///
    /// A playing deck refuses the load ("deck is playing") unless `force` is set, since loading
    /// stops it. The deck's grid is cleared before the file changes, so a synced deck never runs
    /// the old grid over the new audio; if the file then fails to decode, the old grid comes back.
    pub fn load_track(
        &mut self,
        engine: &DjEngine,
        services: &mut LibraryServices,
        deck: usize,
        track: &TrackRecord,
        force: bool,
    ) -> Result<(), LoadError> {
        if deck >= self.slots.len() {
            return Err(LoadError::NoSuchDeck);
        }
        if !force && engine.deck(deck).is_playing() {
            return Err(LoadError::DeckPlaying);
        }
        let path = Path::new(&track.path);
        if !path.is_file() {
            return Err(LoadError::FileMissing);
        }
        // Hotcue edits since the last pump belong to the outgoing track. Write them before the
        // deck's bank is cleared by the load.
        self.save_cue_edits(engine, services.library(), deck);
        let previous_grid = self.slots[deck].grid.take();
        engine.set_grid(deck, None);
        if engine.deck(deck).load(path).is_err() {
            engine.set_grid(deck, previous_grid.clone());
            self.slots[deck].grid = previous_grid;
            return Err(LoadError::Decode {
                file: file_name(path),
            });
        }
        let saved = restore_cues(engine, services.library(), deck, track.id);
        self.cues.bind(deck, track.id, saved);
        self.plays[deck].reset(Some(track.id));
        let found = services.analysis_for(track);
        let slot = &mut self.slots[deck];
        slot.track = Some(track.clone());
        slot.awaiting_analysis = !found.analysed;
        apply(engine, slot, deck, track, &found);
        if !found.analysed {
            services.prep_queue_mut().enqueue(track.id, true)?;
        }
        Ok(())
    }

    /// Loads the audio file at `path` into `deck`. A file the library already holds (matched by
    /// its path) loads as that track, with its analysis, cues and play history. Any other file
    /// loads plainly: the deck plays it, with no metadata, grid, waveform or cues, and the old
    /// track's hotcue edits are still saved first. `force` and the failure cases are as in
    /// [`Self::load_track`].
    pub fn load_path(
        &mut self,
        engine: &DjEngine,
        services: &mut LibraryServices,
        deck: usize,
        path: &Path,
        force: bool,
    ) -> Result<(), LoadError> {
        if deck >= self.slots.len() {
            return Err(LoadError::NoSuchDeck);
        }
        if !force && engine.deck(deck).is_playing() {
            return Err(LoadError::DeckPlaying);
        }
        if !path.is_file() {
            return Err(LoadError::FileMissing);
        }
        let known = services
            .library()
            .tracks()
            .find_by_path(&path.to_string_lossy())?;
        match known {
            Some(track) => self.load_track(engine, services, deck, &track, force),
            None => self.load_plain(engine, services, deck, path),
        }
    }

    /// Loads a file that is not in the library: the deck gets the audio and nothing else.
    fn load_plain(
        &mut self,
        engine: &DjEngine,
        services: &LibraryServices,
        deck: usize,
        path: &Path,
    ) -> Result<(), LoadError> {
        self.save_cue_edits(engine, services.library(), deck);
        let previous_grid = self.slots[deck].grid.take();
        engine.set_grid(deck, None);
        if engine.deck(deck).load(path).is_err() {
            engine.set_grid(deck, previous_grid.clone());
            self.slots[deck].grid = previous_grid;
            return Err(LoadError::Decode {
                file: file_name(path),
            });
        }
        self.cues.unbind(deck);
        self.plays[deck].reset(None);
        self.slots[deck] = Slot::default();
        Ok(())
    }

    /// Applies analysis completions delivered since the last call, then does the control-side
    /// bookkeeping for playback: plays, and hotcue edits to write back.
    pub fn pump(&mut self, engine: &DjEngine, services: &LibraryServices) {
        loop {
            match self.events.try_recv() {
                Ok(LibraryEvent::PrepEntryChanged(entry)) => {
                    self.on_prep_entry(engine, services, &entry);
                }
                Ok(LibraryEvent::LibraryChanged) => {}
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        let now = Instant::now();
        let elapsed = self
            .last_pump
            .map_or(Duration::ZERO, |last| now.duration_since(last));
        self.last_pump = Some(now);
        self.observe_playback(engine, services.library(), elapsed);
    }

    /// One control tick of playback bookkeeping. `elapsed` is the wall time since the previous
    /// tick. Each deck that has played its loaded track for 30 s gets one play recorded, and each
    /// deck's hotcue edits are written to its track. [`Self::pump`] calls this; it is public so a
    /// host with its own clock, and the tests, can drive it.
    pub fn observe_playback(&mut self, engine: &DjEngine, library: &Library, elapsed: Duration) {
        for deck in 0..self.slots.len() {
            let Some(snapshot) = engine.deck_snapshot(deck) else {
                continue;
            };
            if let Some(track_id) = self.plays[deck].observe(&snapshot, elapsed) {
                let played_ms = now_ms();
                let deck_number = u8::try_from(deck).ok();
                if let Err(error) = library
                    .tracks()
                    .record_play(track_id, deck_number, played_ms)
                {
                    eprintln!("library: could not record a play of track {track_id}: {error}");
                }
            }
            if let Err(error) = self.cues.save_changes(deck, snapshot.hotcues, library) {
                eprintln!("library: could not save hotcues: {error}");
            }
        }
    }

    /// Writes the deck's hotcue edits to the track it holds (before the deck's bank changes).
    fn save_cue_edits(&mut self, engine: &DjEngine, library: &Library, deck: usize) {
        let cues = hotcues_of(engine, deck);
        if let Err(error) = self.cues.save_changes(deck, cues, library) {
            eprintln!("library: could not save hotcues: {error}");
        }
    }

    fn on_prep_entry(&mut self, engine: &DjEngine, services: &LibraryServices, entry: &PrepEntry) {
        if entry.status != PrepStatus::Done {
            return;
        }
        for deck in 0..self.slots.len() {
            let waiting = self.slots[deck].awaiting_analysis
                && self.slots[deck]
                    .track
                    .as_ref()
                    .is_some_and(|t| t.id == entry.track_id);
            if waiting {
                self.refresh_from_library(engine, services, deck);
            }
        }
    }

    /// Re-reads the row (analysis wrote bpm and key back) and the cache, then updates the deck.
    fn refresh_from_library(&mut self, engine: &DjEngine, services: &LibraryServices, deck: usize) {
        let Some(id) = self.slots[deck].track.as_ref().map(|t| t.id) else {
            return;
        };
        let Ok(Some(fresh)) = services.library().tracks().find_by_id(id) else {
            return;
        };
        let found = services.analysis_for(&fresh);
        let slot = &mut self.slots[deck];
        slot.awaiting_analysis = !found.analysed;
        apply(engine, slot, deck, &fresh, &found);
        slot.track = Some(fresh);
    }

    /// The track last loaded into `deck`.
    pub fn loaded_track(&self, deck: usize) -> Option<&TrackRecord> {
        self.slots.get(deck)?.track.as_ref()
    }

    /// The cached waveform of the track in `deck`, once it is known.
    pub fn waveform(&self, deck: usize) -> Option<&Arc<WaveformData>> {
        self.slots.get(deck)?.waveform.as_ref()
    }

    /// Header metadata for `deck`; empty when nothing is loaded.
    pub fn track_info(&self, deck: usize) -> TrackInfo {
        self.loaded_track(deck)
            .map(|t| TrackInfo {
                title: t.title.clone(),
                artist: t.artist.clone(),
                key: t.key.clone(),
                camelot: t.camelot.clone(),
                bpm: t.bpm.unwrap_or(0.0),
            })
            .unwrap_or_default()
    }

    /// Where a double-click should load: an empty idle deck, else any idle deck, else deck 0.
    pub fn suggested_deck(&self, engine: &DjEngine) -> usize {
        let mut idle = None;
        for (deck, slot) in self.slots.iter().enumerate() {
            if engine.deck(deck).is_playing() {
                continue;
            }
            if slot.track.is_none() {
                return deck;
            }
            idle.get_or_insert(deck);
        }
        idle.unwrap_or(0)
    }

    /// Camelot code of the track playing now (the first playing deck), else of the first loaded
    /// deck; empty when neither has a key.
    pub fn reference_camelot(&self, engine: &DjEngine) -> String {
        let mut first_loaded = String::new();
        for (deck, slot) in self.slots.iter().enumerate() {
            let Some(track) = slot.track.as_ref() else {
                continue;
            };
            if track.camelot.is_empty() {
                continue;
            }
            if engine.deck(deck).is_playing() {
                return track.camelot.clone();
            }
            if first_loaded.is_empty() {
                first_loaded = track.camelot.clone();
            }
        }
        first_loaded
    }
}

/// Sets the deck's grid and per-deck metadata from an analysis result. An absent grid or waveform
/// clears the old one, so a track with no analysis never shows the previous track's data.
fn apply(
    engine: &DjEngine,
    slot: &mut Slot,
    deck: usize,
    track: &TrackRecord,
    found: &TrackAnalysis,
) {
    engine.set_grid(deck, found.grid.clone());
    slot.grid = found.grid.clone();
    slot.waveform = found.waveform.clone();
    slot.track = Some(track.clone());
}

/// The deck's hotcue bank as the engine reports it; empty when the deck is unknown.
fn hotcues_of(engine: &DjEngine, deck: usize) -> Cues {
    engine
        .deck_snapshot(deck)
        .map_or([None; HOTCUES_PER_DECK as usize], |s| s.hotcues)
}

/// Puts the track's saved hotcues into the deck's bank and returns the bank that results. Slots
/// outside the bank are skipped. An unreadable cue table loads the track without cues.
fn restore_cues(engine: &DjEngine, library: &Library, deck: usize, track_id: i64) -> Cues {
    let saved = library.cues().cues(track_id).unwrap_or_else(|error| {
        eprintln!("library: could not read the saved hotcues: {error}");
        Vec::new()
    });
    for cue in saved {
        let Ok(index) = usize::try_from(cue.slot) else {
            continue;
        };
        if index < HOTCUES_PER_DECK as usize && cue.position_frames >= 0 {
            engine.deck(deck).set_hotcue(index, cue.position_frames);
        }
    }
    hotcues_of(engine, deck)
}

/// Wall-clock time as Unix milliseconds, for the play history.
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}
