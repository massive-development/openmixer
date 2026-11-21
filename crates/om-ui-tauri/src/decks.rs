//! What the front end knows of each deck's track: the track loader and two generations per deck,
//! one counting every change (track, waveform, hot cues) and one counting waveform changes only, so
//! the front end fetches and uploads peaks once per waveform. The control thread ticks it; the deck commands read
//! it from Tauri's IPC threads. The library hub (services, loader, browser) stays on the control
//! thread (it is not `Send`), which publishes what each deck shows ([`PublishedDeck`]) after every
//! tick; only those copies sit behind the lock. A load the front end asks for waits in a queue until
//! the control thread, which holds the loader, runs it.

use crate::lock;
use om_analysis::WaveformData;
use om_engine::DjEngine;
use om_ui::library::{Cues, LibraryServices, TrackLoader};
use om_ui::widgets::deck::TrackInfo;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The demo library the control thread runs: its services and the loader over them.
pub struct DeckLibrary {
    pub services: LibraryServices,
    pub loader: TrackLoader,
}

/// Builds the [`DeckLibrary`] on the control thread, where it then lives.
pub type LibrarySetup = Box<dyn FnOnce() -> Option<DeckLibrary> + Send>;

/// Loads the file at the path into the deck; `Err` is the reason, as the front end shows it.
pub(crate) type LoadFile<'a> = dyn FnMut(usize, &Path) -> Result<(), String> + 'a;

/// What one deck shows of its loaded track, copied off the loader by the control thread.
#[derive(Debug, Clone, Default)]
pub struct PublishedDeck {
    pub track_id: Option<i64>,
    pub waveform: Option<Arc<WaveformData>>,
    pub track: TrackInfo,
}

impl PublishedDeck {
    /// What `loader` holds for `deck` right now.
    pub fn of(loader: &TrackLoader, deck: usize) -> Self {
        Self {
            track_id: loader.loaded_track(deck).map(|t| t.id),
            waveform: loader.waveform(deck).cloned(),
            track: loader.track_info(deck),
        }
    }
}

/// Payload of the `load-failed` event: the deck and why its load did not happen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoadFailed {
    pub deck: usize,
    pub file: String,
    pub reason: String,
}

/// A load the front end asked for, waiting for the control thread.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LoadRequest {
    deck: usize,
    path: PathBuf,
}

/// Payload of the `deck-changed` event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DeckChanged {
    pub deck: usize,
    pub generation: u64,
}

/// One deck as the deck commands read it.
#[derive(Debug, Clone, Default)]
pub(crate) struct DeckView {
    pub(crate) generation: u64,
    /// Changes only with the waveform, the peaks `deck_peaks` returns.
    pub(crate) peaks_generation: u64,
    pub(crate) track: TrackInfo,
    pub(crate) waveform: Option<Arc<WaveformData>>,
}

/// What a change is measured against: the track, the waveform's address, the hot cues.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Seen {
    track: Option<i64>,
    waveform: usize,
    hotcues: Cues,
}

#[derive(Debug, Default)]
struct Watch {
    seen: Seen,
    generation: u64,
    peaks_generation: u64,
}

impl Watch {
    /// Takes in what the deck is now; true when anything changed. Only a different waveform
    /// (including none, or a first one) moves the peaks generation.
    fn advance(&mut self, now: Seen) -> bool {
        if self.seen == now {
            return false;
        }
        if self.seen.waveform != now.waveform {
            self.peaks_generation += 1;
        }
        self.seen = now;
        self.generation += 1;
        true
    }
}

struct DeckState {
    /// One per deck once a library publishes; empty before (and without one).
    published: Vec<PublishedDeck>,
    watches: Vec<Watch>,
}

/// The shared deck state; see the module doc.
pub(crate) struct Decks {
    state: Mutex<DeckState>,
    loads: Mutex<Vec<LoadRequest>>,
}

impl Decks {
    /// Generation 0 for every deck, measured from the engine as it is now.
    pub(crate) fn new(engine: &DjEngine) -> Self {
        let mut state = DeckState {
            published: Vec::new(),
            watches: Vec::new(),
        };
        state.watches = (0..engine.num_decks())
            .map(|deck| Watch {
                seen: seen(&state, engine, deck),
                ..Watch::default()
            })
            .collect();
        Self {
            state: Mutex::new(state),
            loads: Mutex::new(Vec::new()),
        }
    }

    /// Takes in what the decks show now and returns the decks that changed since the last call.
    /// The measuring happens under the same lock, so a read never pairs a waveform with the
    /// generation from before it.
    pub(crate) fn publish(
        &self,
        engine: &DjEngine,
        published: Vec<PublishedDeck>,
    ) -> Vec<DeckChanged> {
        let mut state = lock(&self.state);
        state.published = published;
        observe_all(&mut state, engine)
    }

    /// The decks that changed since the last tick, with nothing new published (no library).
    pub(crate) fn tick(&self, engine: &DjEngine) -> Vec<DeckChanged> {
        observe_all(&mut lock(&self.state), engine)
    }

    /// Queues a load of `path` into `deck` for the control thread; `Err` for a deck that does not
    /// exist.
    pub(crate) fn request_load(&self, deck: usize, path: PathBuf) -> Result<(), String> {
        if deck >= lock(&self.state).watches.len() {
            return Err(format!("no deck {deck}"));
        }
        lock(&self.loads).push(LoadRequest { deck, path });
        Ok(())
    }

    /// Runs the queued loads through `load` on the control thread, which owns the loader; returns
    /// the ones that failed. Without a library (`None`) every load fails, saying so.
    pub(crate) fn run_loads(&self, mut load: Option<&mut LoadFile<'_>>) -> Vec<LoadFailed> {
        let requests = std::mem::take(&mut *lock(&self.loads));
        requests
            .into_iter()
            .filter_map(|request| {
                let result = match load.as_deref_mut() {
                    Some(load) => load(request.deck, &request.path),
                    None => Err("no library is open".to_owned()),
                };
                result.err().map(|reason| failed(&request, reason))
            })
            .collect()
    }

    /// Deck `deck` as the commands report it; `None` when there is no such deck.
    pub(crate) fn read(&self, deck: usize) -> Option<DeckView> {
        let state = lock(&self.state);
        let watch = state.watches.get(deck)?;
        let shown = state.published.get(deck);
        Some(DeckView {
            generation: watch.generation,
            peaks_generation: watch.peaks_generation,
            track: shown.map(|d| d.track.clone()).unwrap_or_default(),
            waveform: shown.and_then(|d| d.waveform.clone()),
        })
    }
}

fn failed(request: &LoadRequest, reason: String) -> LoadFailed {
    let file = request
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| request.path.display().to_string());
    LoadFailed {
        deck: request.deck,
        file,
        reason,
    }
}

fn observe_all(state: &mut DeckState, engine: &DjEngine) -> Vec<DeckChanged> {
    (0..state.watches.len())
        .filter_map(|deck| observe(state, engine, deck))
        .collect()
}

fn observe(state: &mut DeckState, engine: &DjEngine, deck: usize) -> Option<DeckChanged> {
    let now = seen(state, engine, deck);
    let watch = &mut state.watches[deck];
    if !watch.advance(now) {
        return None;
    }
    Some(DeckChanged {
        deck,
        generation: watch.generation,
    })
}

fn seen(state: &DeckState, engine: &DjEngine, deck: usize) -> Seen {
    let shown = state.published.get(deck);
    Seen {
        track: shown.and_then(|d| d.track_id),
        // The address tells one waveform from the next; it is never dereferenced.
        waveform: shown
            .and_then(|d| d.waveform.as_ref())
            .map_or(0, |w| Arc::as_ptr(w) as usize),
        hotcues: engine
            .deck_snapshot(deck)
            .map(|s| s.hotcues)
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> DjEngine {
        DjEngine::new(2).0
    }

    #[test]
    fn a_quiet_engine_reports_nothing() {
        let engine = engine();
        let decks = Decks::new(&engine);
        assert!(decks.tick(&engine).is_empty());
        assert_eq!(decks.read(0).map(|v| v.generation), Some(0));
    }

    #[test]
    fn a_hot_cue_change_bumps_that_decks_generation_once() {
        let engine = engine();
        let decks = Decks::new(&engine);
        engine.deck(1).set_hotcue(2, 44_100);
        let changes = decks.tick(&engine);
        assert_eq!(
            changes,
            vec![DeckChanged {
                deck: 1,
                generation: 1
            }]
        );
        assert!(decks.tick(&engine).is_empty(), "reported once");
        engine.deck(1).set_hotcue(2, 88_200);
        assert_eq!(decks.tick(&engine)[0].generation, 2);
        assert_eq!(decks.read(1).map(|v| v.generation), Some(2));
        assert_eq!(decks.read(0).map(|v| v.generation), Some(0));
    }

    fn seen_with(waveform: usize, cue: Option<i64>) -> Seen {
        let mut hotcues = Cues::default();
        hotcues[0] = cue;
        Seen {
            track: Some(1),
            waveform,
            hotcues,
        }
    }

    fn watching(seen: Seen) -> Watch {
        Watch {
            seen,
            ..Watch::default()
        }
    }

    #[test]
    fn a_hot_cue_change_leaves_the_peaks_generation() {
        let mut watch = watching(seen_with(0x10, None));
        assert!(watch.advance(seen_with(0x10, Some(44_100))));
        assert_eq!((watch.generation, watch.peaks_generation), (1, 0));
        assert!(!watch.advance(seen_with(0x10, Some(44_100))), "no change");
        assert_eq!((watch.generation, watch.peaks_generation), (1, 0));
    }

    #[test]
    fn a_new_waveform_bumps_both_generations() {
        let mut watch = watching(seen_with(0x10, None));
        assert!(watch.advance(seen_with(0x20, None)));
        assert_eq!((watch.generation, watch.peaks_generation), (1, 1));
    }

    #[test]
    fn unloading_or_a_first_waveform_bumps_both_generations() {
        let mut watch = watching(seen_with(0x10, None));
        let mut unloaded = seen_with(0, None);
        unloaded.track = None;
        assert!(watch.advance(unloaded));
        assert_eq!((watch.generation, watch.peaks_generation), (1, 1));
        assert!(watch.advance(seen_with(0x30, None)));
        assert_eq!((watch.generation, watch.peaks_generation), (2, 2));
    }

    #[test]
    fn a_title_only_change_leaves_the_peaks_generation() {
        let mut watch = watching(seen_with(0x10, None));
        let mut retitled = seen_with(0x10, None);
        retitled.track = Some(2);
        assert!(watch.advance(retitled));
        assert_eq!((watch.generation, watch.peaks_generation), (1, 0));
    }

    #[test]
    fn a_hot_cue_change_on_the_engine_reads_back_without_a_peaks_bump() {
        let engine = engine();
        let decks = Decks::new(&engine);
        engine.deck(0).set_hotcue(1, 1_000);
        decks.tick(&engine);
        let view = decks.read(0).unwrap();
        assert_eq!((view.generation, view.peaks_generation), (1, 0));
    }

    #[test]
    fn an_unknown_deck_reads_none() {
        let engine = engine();
        assert!(Decks::new(&engine).read(2).is_none());
    }

    #[test]
    fn without_a_loader_a_deck_is_empty() {
        let engine = engine();
        let view = Decks::new(&engine).read(0).unwrap();
        assert_eq!(view.track, TrackInfo::default());
        assert!(view.waveform.is_none());
    }

    #[test]
    fn a_load_for_a_deck_that_does_not_exist_is_refused() {
        let engine = engine();
        let decks = Decks::new(&engine);
        assert!(decks.request_load(2, PathBuf::from("/m/a.wav")).is_err());
        assert!(decks.request_load(1, PathBuf::from("/m/a.wav")).is_ok());
    }

    #[test]
    fn without_a_library_a_queued_load_fails_once_naming_the_file() {
        let engine = engine();
        let decks = Decks::new(&engine);
        decks
            .request_load(1, PathBuf::from("/music/set/intro.flac"))
            .unwrap();
        let failures = decks.run_loads(None);
        assert_eq!(
            failures,
            vec![LoadFailed {
                deck: 1,
                file: "intro.flac".to_owned(),
                reason: "no library is open".to_owned(),
            }]
        );
        assert!(decks.run_loads(None).is_empty(), "the queue drained");
    }
}
