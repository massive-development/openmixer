//! The automix host over the app's library: the queue picks, the track loader loads, the analysis
//! cache says where the beats, the key and the structure of each deck are. [`AppAutomixHost`] is
//! the [`om_automix::AutomixHost`] the window steps; it is generic over [`AutomixLibrary`] so the
//! tests run it over an in-memory library with no window.
//!
//! Threads: the window thread. The library and the track loader are not `Send` (the services hold
//! `Rc` state), so the host lives beside them, and [`AutomixRunner`](crate::automix_service::AutomixRunner)
//! is stepped from the window's frame tick. Loads are synchronous (the loader decodes on the
//! caller), so [`AutomixHost::poll_load`] is always ready once `start_load` succeeded.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use om_analysis::{Section as AnalysisSection, StructureResult};
use om_automix::queue::{AutomixQueue, QueueTrack};
use om_automix::{
    AutomixHost, Camelot, DeckInfo, GridView, LoadPoll, NextTrack, Section, SectionKind,
};
use om_engine::grid::BeatGrid;
use om_engine::DjEngine;
use om_library::TrackRecord;
use om_ui::library::{LibraryServices, TrackLoader};

use crate::automix_queue::LibraryQueue;

/// What the host needs from the window's library: the services and the loader, borrowed together.
pub trait AutomixLibrary {
    fn parts(&self) -> (&LibraryServices, &TrackLoader);
    fn parts_mut(&mut self) -> (&mut LibraryServices, &mut TrackLoader);
}

/// The engine's grid as the planner's view (same four methods).
struct EngineGrid(Arc<dyn BeatGrid>);

impl GridView for EngineGrid {
    fn bpm(&self) -> f64 {
        self.0.bpm()
    }
    fn beats_per_bar(&self) -> i32 {
        self.0.beats_per_bar()
    }
    fn beat_at_frame(&self, frame: i64) -> f64 {
        self.0.beat_at_frame(frame)
    }
    fn frame_at_beat(&self, beat: f64) -> i64 {
        self.0.frame_at_beat(beat)
    }
}

/// `DeckInfo` for the track on a deck, from the library's cache and the engine's deck state.
/// Returns `None` when the deck is empty.
fn deck_info_for<L: AutomixLibrary>(library: &L, engine: &DjEngine, deck: u8) -> Option<DeckInfo> {
    let (services, loader) = library.parts();
    let track = loader.loaded_track(usize::from(deck))?;
    let snap = engine.deck_snapshot(usize::from(deck))?;
    let rate = if snap.file_sample_rate > 0.0 {
        snap.file_sample_rate
    } else {
        44_100.0
    };
    let grid = services.analysis_for(track).grid;
    let structure = services
        .cached_analysis(track, "structure")
        .and_then(|json| StructureResult::from_json(&json));
    let to_beat =
        |seconds: f64, grid: &dyn BeatGrid| grid.beat_at_frame((seconds * rate).round() as i64);
    let (bpm, length_beats, sections, mix_in, mix_out) = match &grid {
        Some(g) => {
            let sections = structure
                .as_ref()
                .map(|s| {
                    s.sections
                        .iter()
                        .map(|sec: &AnalysisSection| Section {
                            start_beat: to_beat(sec.start_seconds, g.as_ref()),
                            end_beat: to_beat(sec.end_seconds, g.as_ref()),
                            kind: SectionKind::from_label(&sec.label),
                            energy: sec.energy,
                        })
                        .collect()
                })
                .unwrap_or_default();
            let (mix_in, mix_out) = structure.as_ref().map_or((None, None), |s| {
                (
                    Some(to_beat(s.mix_in_seconds, g.as_ref())),
                    Some(to_beat(s.mix_out_seconds, g.as_ref())),
                )
            });
            (
                g.bpm(),
                g.beat_at_frame(snap.length_frames),
                sections,
                mix_in,
                mix_out,
            )
        }
        None => (track.bpm.unwrap_or(0.0), 0.0, Vec::new(), None, None),
    };
    Some(DeckInfo {
        deck,
        grid: grid.map(|g| Arc::new(EngineGrid(g)) as Arc<dyn GridView>),
        // Unparsable codes are unknown to the planner; keep the record's text only when it parses.
        camelot: if Camelot::parse(&track.camelot).is_some() {
            track.camelot.trim().to_owned()
        } else {
            String::new()
        },
        sections,
        mix_out,
        mix_in,
        has_stems: engine.deck(usize::from(deck)).has_stems(),
        length_beats,
        bpm,
    })
}

/// The queue track of a loaded record, for the queue's key and tempo preference.
fn current_track(track: &TrackRecord) -> QueueTrack {
    QueueTrack {
        id: track.id,
        title: track.title.clone(),
        artist: track.artist.clone(),
        path: track.path.clone(),
        bpm: track.bpm,
        camelot: track.camelot.clone(),
        has_grid: true,
    }
}

/// The automix host over the window's library.
pub struct AppAutomixHost<L: AutomixLibrary + 'static> {
    engine: Arc<DjEngine>,
    library: Rc<RefCell<L>>,
    queue: LibraryQueue,
    /// Why the last load failed, for the controller's `last_error` (the controller also says it).
    last_load_error: Option<String>,
}

impl<L: AutomixLibrary + 'static> AppAutomixHost<L> {
    /// A host that picks from `queue` and loads through `library`. The queue's beat-grid check is
    /// the analysis cache's (a BPM alone does not count): this sets it.
    pub fn new(engine: Arc<DjEngine>, library: Rc<RefCell<L>>, mut queue: LibraryQueue) -> Self {
        let check_library = Rc::clone(&library);
        queue = queue
            .with_grid_check(move |track| check_library.borrow().parts().0.has_beat_grid(track));
        Self {
            engine,
            library,
            queue,
            last_load_error: None,
        }
    }

    /// The queue, for changing its source or reading its skips.
    pub fn queue_mut(&mut self) -> &mut LibraryQueue {
        &mut self.queue
    }

    /// The reason the last `start_load` failed, if any.
    pub fn last_load_error(&self) -> Option<&str> {
        self.last_load_error.as_deref()
    }
}

impl<L: AutomixLibrary + 'static> AutomixHost for AppAutomixHost<L> {
    fn next_track(&mut self, playing_deck: u8) -> Option<NextTrack> {
        let current = {
            let library = self.library.borrow();
            library
                .parts()
                .1
                .loaded_track(usize::from(playing_deck))
                .map(current_track)
        };
        let pick = self.queue.next_track(current.as_ref())?;
        Some(NextTrack {
            id: pick.id,
            title: pick.title,
        })
    }

    fn start_load(&mut self, deck: u8, track: &NextTrack) -> Result<(), String> {
        let mut library = self.library.borrow_mut();
        let (services, loader) = library.parts_mut();
        let result = loader
            .load(&self.engine, services, usize::from(deck), track.id, false)
            .map_err(|error| error.to_string());
        self.last_load_error = result.as_ref().err().cloned();
        result
    }

    fn poll_load(&mut self, _deck: u8) -> LoadPoll {
        // Loads are synchronous: `start_load` returned, so the deck holds the track.
        LoadPoll::Ready
    }

    fn deck_info(&mut self, deck: u8) -> Option<DeckInfo> {
        let library = self.library.borrow();
        deck_info_for(&*library, &self.engine, deck)
    }

    fn deck_title(&mut self, deck: u8) -> String {
        let library = self.library.borrow();
        library
            .parts()
            .1
            .loaded_track(usize::from(deck))
            .map(|t| t.title.clone())
            .unwrap_or_default()
    }

    fn underruns(&self) -> u64 {
        (0..self.engine.num_decks())
            .map(|deck| u64::from(self.engine.deck(deck).underrun_count()))
            .sum()
    }
}

#[cfg(feature = "gui")]
impl AutomixLibrary for om_ui_gpui::library::LibraryHub {
    fn parts(&self) -> (&LibraryServices, &TrackLoader) {
        (&self.services, &self.loader)
    }

    fn parts_mut(&mut self) -> (&mut LibraryServices, &mut TrackLoader) {
        (&mut self.services, &mut self.loader)
    }
}
