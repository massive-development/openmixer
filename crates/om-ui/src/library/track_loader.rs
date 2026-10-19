//! Loads library tracks into decks. The file goes to the engine deck; the track metadata, the cached
//! waveform and the cached beat grid follow. A track that is not analysed yet is queued in the prep
//! queue with priority, and when the queue reports it done the deck is refreshed from the library.
//! Port of `src/ui/TrackLoader.{h,cpp}`.
//!
//! The waveform and track metadata are kept per deck for the deck widgets to read ([`Self::waveform`],
//! [`Self::track_info`]); the beat grid is set on the engine, which owns the clock. The loader holds
//! a subscription to the services and must be pumped ([`Self::pump`]) after the services are.

use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;

use om_analysis::WaveformData;
use om_engine::DjEngine;
use om_library::{PrepEntry, PrepStatus, TrackRecord};

use super::services::{LibraryEvent, LibraryServices, TrackAnalysis};
use crate::widgets::deck::TrackInfo;

#[derive(Default)]
struct Slot {
    track: Option<TrackRecord>,
    /// Queued for analysis; refreshed when the queue reports it done.
    awaiting_analysis: bool,
    waveform: Option<Arc<WaveformData>>,
}

pub struct TrackLoader {
    slots: Vec<Slot>,
    events: Receiver<LibraryEvent>,
}

impl TrackLoader {
    /// A loader for `num_decks` decks. Subscribes to `services` for analysis completions.
    pub fn new(services: &mut LibraryServices, num_decks: usize) -> Self {
        Self {
            slots: (0..num_decks).map(|_| Slot::default()).collect(),
            events: services.subscribe(),
        }
    }

    /// Loads a library track by id into `deck`.
    pub fn load(
        &mut self,
        engine: &DjEngine,
        services: &mut LibraryServices,
        deck: usize,
        track_id: i64,
    ) -> Result<(), String> {
        let track = services
            .library()
            .tracks()
            .find_by_id(track_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("unknown track {track_id}"))?;
        self.load_track(engine, services, deck, &track)
    }

    /// Loads a track into `deck`. A missing file or an undecodable one fails without touching the
    /// deck. A track without cached analysis is queued with priority; the deck is loaded and
    /// playable either way.
    pub fn load_track(
        &mut self,
        engine: &DjEngine,
        services: &mut LibraryServices,
        deck: usize,
        track: &TrackRecord,
    ) -> Result<(), String> {
        if deck >= self.slots.len() {
            return Err("no such deck".to_owned());
        }
        let path = Path::new(&track.path);
        if !path.is_file() {
            return Err("file is missing".to_owned());
        }
        if engine.deck(deck).load(path).is_err() {
            return Err(format!("cannot decode {}", file_name(path)));
        }
        let found = services.analysis_for(track);
        let slot = &mut self.slots[deck];
        slot.track = Some(track.clone());
        slot.awaiting_analysis = !found.analysed;
        apply(engine, slot, deck, track, &found);
        if !found.analysed {
            services
                .prep_queue_mut()
                .enqueue(track.id, true)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Applies analysis completions delivered since the last call.
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
    slot.waveform = found.waveform.clone();
    slot.track = Some(track.clone());
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}
