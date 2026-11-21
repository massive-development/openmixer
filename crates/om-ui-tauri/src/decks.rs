//! What the front end knows of each deck's track: the track loader and two generations per deck,
//! one counting every change (track, waveform, hot cues) and one counting waveform changes only, so
//! the front end fetches and uploads peaks once per waveform. The control thread ticks it; the deck commands read
//! it from Tauri's IPC threads. The library services stay on the control thread (they are not
//! `Send`), so only the loader sits behind the lock. A load the front end asks for waits in a queue
//! until the control thread, which holds the services, runs it.

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
    /// Separates tracks into stems (the model, the cache); `None` when the app has none.
    pub stems: Option<Box<dyn om_ui::stems::StemHost>>,
}

/// Builds the [`DeckLibrary`] on the control thread, where it then lives.
pub type LibrarySetup = Box<dyn FnOnce() -> Option<DeckLibrary> + Send>;

/// What the deck commands read off the track loader; a trait so a test can stand in for one.
pub(crate) trait DeckSource: Send {
    /// Applies finished analysis (the control thread's tick).
    fn pump(&mut self, engine: &DjEngine, services: &LibraryServices);
    fn track_id(&self, deck: usize) -> Option<i64>;
    fn waveform(&self, deck: usize) -> Option<&Arc<WaveformData>>;
    fn track_info(&self, deck: usize) -> TrackInfo;
    /// Loads the audio file at `path` into `deck` (adding it to the library first); a playing
    /// deck refuses. `Err` is the reason, as the front end shows it.
    fn load_file(
        &mut self,
        engine: &DjEngine,
        services: &mut LibraryServices,
        deck: usize,
        path: &Path,
    ) -> Result<(), String>;
    /// The 'Load stems' / 'Stems off' action; true when the deck now plays from stems.
    fn toggle_stems(
        &mut self,
        _engine: &DjEngine,
        _services: &LibraryServices,
        _deck: usize,
    ) -> Result<bool, String> {
        Err("stems are not available".to_owned())
    }
    /// Why the stems of `deck` did not load with its last track.
    fn stem_note(&self, _deck: usize) -> Option<String> {
        None
    }
    /// Whether the deck's track has a complete cached stem set.
    fn stems_cached(&self, _services: &LibraryServices, _deck: usize) -> bool {
        false
    }
}

impl DeckSource for TrackLoader {
    fn pump(&mut self, engine: &DjEngine, services: &LibraryServices) {
        TrackLoader::pump(self, engine, services);
    }

    fn track_id(&self, deck: usize) -> Option<i64> {
        self.loaded_track(deck).map(|t| t.id)
    }

    fn waveform(&self, deck: usize) -> Option<&Arc<WaveformData>> {
        TrackLoader::waveform(self, deck)
    }

    fn track_info(&self, deck: usize) -> TrackInfo {
        TrackLoader::track_info(self, deck)
    }

    fn load_file(
        &mut self,
        engine: &DjEngine,
        services: &mut LibraryServices,
        deck: usize,
        path: &Path,
    ) -> Result<(), String> {
        TrackLoader::load_file(self, engine, services, deck, path, false).map_err(|e| e.to_string())
    }

    fn toggle_stems(
        &mut self,
        engine: &DjEngine,
        services: &LibraryServices,
        deck: usize,
    ) -> Result<bool, String> {
        TrackLoader::toggle_stems(self, engine, services, deck).map_err(|e| e.to_string())
    }

    fn stem_note(&self, deck: usize) -> Option<String> {
        TrackLoader::stem_note(self, deck).map(str::to_owned)
    }

    fn stems_cached(&self, services: &LibraryServices, deck: usize) -> bool {
        let Some(track) = self.loaded_track(deck) else {
            return false;
        };
        self.cached_stems(services.library(), track.id).is_some()
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
    loader: Option<Box<dyn DeckSource>>,
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
            loader: None,
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

    /// Hands the loader over and measures the decks it loaded under the same lock, so a read never
    /// pairs its waveform with the generation from before it; returns the decks that changed.
    pub(crate) fn install(
        &self,
        engine: &DjEngine,
        loader: Box<dyn DeckSource>,
    ) -> Vec<DeckChanged> {
        let mut state = lock(&self.state);
        state.loader = Some(loader);
        observe_all(&mut state, engine)
    }

    /// Applies finished analysis, then returns the decks that changed since the last tick.
    pub(crate) fn tick(
        &self,
        engine: &DjEngine,
        services: Option<&LibraryServices>,
    ) -> Vec<DeckChanged> {
        let mut state = lock(&self.state);
        if let (Some(loader), Some(services)) = (state.loader.as_mut(), services) {
            loader.pump(engine, services);
        }
        observe_all(&mut state, engine)
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

    /// Runs the queued loads on the control thread, which owns `services`; returns the ones that
    /// failed. Without a library every load fails, saying so.
    pub(crate) fn run_loads(
        &self,
        engine: &DjEngine,
        mut services: Option<&mut LibraryServices>,
    ) -> Vec<LoadFailed> {
        let requests = std::mem::take(&mut *lock(&self.loads));
        let mut state = lock(&self.state);
        requests
            .into_iter()
            .filter_map(|request| {
                let result = match (state.loader.as_mut(), services.as_deref_mut()) {
                    (Some(loader), Some(services)) => {
                        loader.load_file(engine, services, request.deck, &request.path)
                    }
                    _ => Err("no library is open".to_owned()),
                };
                result.err().map(|reason| failed(&request, reason))
            })
            .collect()
    }

    /// The library id of the track in `deck`, when it holds one.
    pub(crate) fn track_id(&self, deck: usize) -> Option<i64> {
        lock(&self.state).loader.as_ref()?.track_id(deck)
    }

    /// The stems action of the control thread, which owns `services`.
    pub(crate) fn toggle_stems(
        &self,
        engine: &DjEngine,
        services: &LibraryServices,
        deck: usize,
    ) -> Result<bool, String> {
        let mut state = lock(&self.state);
        let loader = state.loader.as_mut().ok_or("no library is open")?;
        loader.toggle_stems(engine, services, deck)
    }

    pub(crate) fn stem_note(&self, deck: usize) -> Option<String> {
        lock(&self.state).loader.as_ref()?.stem_note(deck)
    }

    pub(crate) fn stems_cached(&self, deck: usize, services: &LibraryServices) -> bool {
        let state = lock(&self.state);
        state
            .loader
            .as_ref()
            .is_some_and(|l| l.stems_cached(services, deck))
    }

    /// Deck `deck` as the commands report it; `None` when there is no such deck.
    pub(crate) fn read(&self, deck: usize) -> Option<DeckView> {
        let state = lock(&self.state);
        let watch = state.watches.get(deck)?;
        let loader = state.loader.as_ref();
        Some(DeckView {
            generation: watch.generation,
            peaks_generation: watch.peaks_generation,
            track: loader.map(|l| l.track_info(deck)).unwrap_or_default(),
            waveform: loader.and_then(|l| l.waveform(deck)).cloned(),
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
    let loader = state.loader.as_ref();
    Seen {
        track: loader.and_then(|l| l.track_id(deck)),
        // The address tells one waveform from the next; it is never dereferenced.
        waveform: loader
            .and_then(|l| l.waveform(deck))
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
        assert!(decks.tick(&engine, None).is_empty());
        assert_eq!(decks.read(0).map(|v| v.generation), Some(0));
    }

    #[test]
    fn a_hot_cue_change_bumps_that_decks_generation_once() {
        let engine = engine();
        let decks = Decks::new(&engine);
        engine.deck(1).set_hotcue(2, 44_100);
        let changes = decks.tick(&engine, None);
        assert_eq!(
            changes,
            vec![DeckChanged {
                deck: 1,
                generation: 1
            }]
        );
        assert!(decks.tick(&engine, None).is_empty(), "reported once");
        engine.deck(1).set_hotcue(2, 88_200);
        assert_eq!(decks.tick(&engine, None)[0].generation, 2);
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
        decks.tick(&engine, None);
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
        let failures = decks.run_loads(&engine, None);
        assert_eq!(
            failures,
            vec![LoadFailed {
                deck: 1,
                file: "intro.flac".to_owned(),
                reason: "no library is open".to_owned(),
            }]
        );
        assert!(
            decks.run_loads(&engine, None).is_empty(),
            "the queue drained"
        );
    }
}
