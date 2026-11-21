//! The control thread (`om-ui-control`): builds the library hub when there is one (the services,
//! the track loader and the browser's [`LibraryPanel`]), then at 30 Hz runs the library calls and
//! the loads the front end queued, ticks the panel (which pumps the library and the track loader),
//! publishes what the decks show and reports the decks that changed and the loads that failed; in
//! a `--ui-drive-out` run it also rewrites the engine's state file every third tick (10 Hz). The
//! hub is not `Send`, so it lives on this thread alone; nothing here runs on the audio thread or
//! waits on a plugin.

use crate::decks::{DeckChanged, Decks, LibrarySetup, LoadFailed, PublishedDeck};
use crate::drive::{drive_state, write_json_atomic};
use crate::library::LibraryQueue;
use crate::pacer::Pacer;
use om_engine::DjEngine;
use om_ui::library::hub::{LibraryHub, LibraryPanel};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

/// Name of the control thread.
pub const CONTROL_THREAD: &str = "om-ui-control";
/// How often the loader is pumped.
const CONTROL_HZ: f64 = 30.0;
/// The drive state file is rewritten every this many ticks (10 Hz).
const STATE_EVERY: u64 = 3;

/// What the control thread works on.
pub(crate) struct ControlLoop {
    pub(crate) engine: Arc<DjEngine>,
    pub(crate) decks: Arc<Decks>,
    /// Calls the commands made on the library browser, run here.
    pub(crate) library: Arc<LibraryQueue>,
    pub(crate) setup: Option<LibrarySetup>,
    pub(crate) stop: Arc<AtomicBool>,
    /// The drive run's state file (`<path>.state`); `None` writes nothing.
    pub(crate) drive_state: Option<PathBuf>,
}

impl ControlLoop {
    /// Runs the loop on a thread named [`CONTROL_THREAD`]; `emit` and `failed` are called with no
    /// lock held.
    pub(crate) fn spawn(
        self,
        emit: impl Fn(DeckChanged) + Send + 'static,
        failed: impl Fn(LoadFailed) + Send + 'static,
    ) -> std::io::Result<JoinHandle<()>> {
        std::thread::Builder::new()
            .name(CONTROL_THREAD.into())
            .spawn(move || self.run(emit, failed))
    }

    fn run(mut self, emit: impl Fn(DeckChanged), failed: impl Fn(LoadFailed)) {
        let mut panel = self.attach_library(&emit);
        let mut pacer = Pacer::new(CONTROL_HZ);
        let mut ticks: u64 = 0;
        while !self.stop.load(Ordering::Acquire) {
            let (failures, changes) = self.step(panel.as_mut());
            failures.into_iter().for_each(&failed);
            changes.into_iter().for_each(&emit);
            if ticks.is_multiple_of(STATE_EVERY) {
                self.write_drive_state();
            }
            ticks = ticks.wrapping_add(1);
            pacer.wait();
        }
        // Whoever still waits on the library finds it closed.
        self.library.discard();
    }

    // One tick: the library calls, the queued loads, the panel's own tick, then what the decks show.
    fn step(&self, panel: Option<&mut LibraryPanel>) -> (Vec<LoadFailed>, Vec<DeckChanged>) {
        let Some(panel) = panel else {
            self.library.discard();
            return (self.decks.run_loads(None), self.decks.tick(&self.engine));
        };
        self.library.run(panel);
        let failures = self.run_loads(panel);
        panel.tick();
        let shown = published(panel, self.engine.num_decks());
        (failures, self.decks.publish(&self.engine, shown))
    }

    // A file the front end chose goes into the library first (so it is analysed like any track),
    // then onto its deck; the browser hears of the new track.
    fn run_loads(&self, panel: &LibraryPanel) -> Vec<LoadFailed> {
        let hub = panel.hub();
        let mut load = |deck: usize, path: &Path| {
            let mut hub = hub.borrow_mut();
            let LibraryHub {
                services, loader, ..
            } = &mut *hub;
            loader
                .load_file(&self.engine, services, deck, path, false)
                .map_err(|e| e.to_string())?;
            services.notify_library_changed();
            Ok(())
        };
        self.decks.run_loads(Some(&mut load))
    }

    // A drive run's state file; a failed write is retried on the next round, so it only logs.
    fn write_drive_state(&self) {
        let Some(path) = &self.drive_state else {
            return;
        };
        let bpms: Vec<Option<f64>> = (0..self.engine.num_decks())
            .map(|d| self.engine.deck_control(d).grid().map(|g| g.bpm()))
            .collect();
        let state = drive_state(&self.engine.snapshot(), &bpms);
        if let Err(error) = write_json_atomic(path, &state) {
            tracing::debug!(%error, "drive state not written");
        }
    }

    // The hub is built here because it cannot move between threads; the decks the loader already
    // holds are reported at once.
    fn attach_library(&mut self, emit: &impl Fn(DeckChanged)) -> Option<LibraryPanel> {
        let library = self.setup.take().and_then(|setup| setup())?;
        let hub = LibraryHub::from_parts(library.services, library.loader);
        let panel = LibraryPanel::new(hub, Arc::clone(&self.engine));
        let changes = self
            .decks
            .publish(&self.engine, published(&panel, self.engine.num_decks()));
        changes.into_iter().for_each(emit);
        Some(panel)
    }
}

/// Each deck's track, waveform and header text, copied off the loader.
fn published(panel: &LibraryPanel, num_decks: usize) -> Vec<PublishedDeck> {
    let hub = panel.hub().borrow();
    (0..num_decks)
        .map(|deck| PublishedDeck::of(&hub.loader, deck))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_testing::wait::wait_until;
    use std::sync::Mutex;
    use std::time::Duration;

    fn control(engine: &Arc<DjEngine>, setup: Option<LibrarySetup>) -> ControlLoop {
        ControlLoop {
            engine: Arc::clone(engine),
            decks: Arc::new(Decks::new(engine)),
            library: Arc::default(),
            setup,
            stop: Arc::new(AtomicBool::new(false)),
            drive_state: None,
        }
    }

    #[test]
    fn a_hot_cue_change_is_emitted_from_the_named_thread() {
        let engine = Arc::new(DjEngine::new(2).0);
        let control = control(&engine, None);
        let stop = Arc::clone(&control.stop);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let emit = move |c| {
            let name = std::thread::current().name().map(str::to_owned);
            sink.lock().unwrap().push((c, name));
        };
        let handle = control.spawn(emit, |_| {}).unwrap();
        engine.deck(0).set_hotcue(0, 1_000);
        let emitted = || !seen.lock().unwrap().is_empty();
        assert!(wait_until(
            Duration::from_secs(5),
            Duration::from_millis(2),
            emitted
        ));
        stop.store(true, Ordering::Release);
        handle.join().unwrap();
        let (change, thread) = seen.lock().unwrap()[0].clone();
        assert_eq!(
            change,
            DeckChanged {
                deck: 0,
                generation: 1
            }
        );
        assert_eq!(thread.as_deref(), Some(CONTROL_THREAD));
    }

    #[test]
    fn a_drive_run_rewrites_the_state_file_and_no_other_run_writes_one() {
        let engine = Arc::new(DjEngine::new(2).0);
        let dir = std::env::temp_dir().join(format!("om-ui-control-drive-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("drive.json.state");
        let mut control = control(&engine, None);
        control.drive_state = Some(path.clone());
        let stop = Arc::clone(&control.stop);
        let handle = control.spawn(|_| {}, |_| {}).unwrap();
        let written = || {
            std::fs::read(&path)
                .is_ok_and(|b| serde_json::from_slice::<serde_json::Value>(&b).is_ok())
        };
        assert!(wait_until(
            Duration::from_secs(5),
            Duration::from_millis(2),
            written
        ));
        stop.store(true, Ordering::Release);
        handle.join().unwrap();
        let state: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(state["decks"].as_array().map(Vec::len), Some(2));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_setup_that_finds_no_library_still_runs_the_loop() {
        let engine = Arc::new(DjEngine::new(2).0);
        let ran = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&ran);
        let setup: LibrarySetup = Box::new(move || {
            flag.store(true, Ordering::Release);
            None
        });
        let control = control(&engine, Some(setup));
        let stop = Arc::clone(&control.stop);
        let handle = control.spawn(|_| {}, |_| {}).unwrap();
        assert!(wait_until(
            Duration::from_secs(5),
            Duration::from_millis(2),
            || ran.load(Ordering::Acquire)
        ));
        stop.store(true, Ordering::Release);
        handle.join().unwrap();
    }

    /// A library in memory with two tracks, built on the control thread like the real one.
    fn two_tracks() -> LibrarySetup {
        Box::new(|| {
            let mut options = om_ui::library::Options::for_path(om_ui::library::MEMORY_PATH);
            options.analysis_workers = 1;
            let mut services = om_ui::library::LibraryServices::create(options).ok()?;
            for (title, bpm) in [("Low", 100.0), ("High", 128.0)] {
                let track = om_library::TrackRecord {
                    path: format!("/music/{title}.wav"),
                    title: title.to_owned(),
                    artist: "Gen".to_owned(),
                    bpm: Some(bpm),
                    camelot: "8A".to_owned(),
                    duration_sec: 200.0,
                    ..Default::default()
                };
                services.library().tracks().insert(&track).ok()?;
            }
            let loader = om_ui::library::TrackLoader::new(&mut services, 2);
            Some(crate::decks::DeckLibrary { services, loader })
        })
    }

    #[test]
    fn library_calls_run_on_the_control_thread_and_answer_with_the_browser_view() {
        let engine = Arc::new(DjEngine::new(2).0);
        let control = control(&engine, Some(two_tracks()));
        let (stop, queue) = (Arc::clone(&control.stop), Arc::clone(&control.library));
        let handle = control.spawn(|_| {}, |_| {}).unwrap();
        let first = queue
            .call(|panel| {
                let name = std::thread::current().name().map(str::to_owned);
                (name, crate::library::view(panel, 0, 10))
            })
            .unwrap();
        assert_eq!(first.0.as_deref(), Some(CONTROL_THREAD));
        let view = first.1;
        assert_eq!(view.row_count, 2);
        assert_eq!(view.rows.len(), 2);
        assert!(view.sidebar.iter().any(|l| l.kind == "all" && l.selected));
        assert!(
            view.columns.iter().all(|c| c.title != "Stems"),
            "no stems host"
        );
        assert_eq!(view.rows[0].cells.len(), view.columns.len());
        // A search narrows the list, and the answer already shows it.
        let found = queue
            .call(|panel| {
                panel.set_query("High");
                panel.commit_query();
                crate::library::view(panel, 0, 10)
            })
            .unwrap();
        assert_eq!(found.row_count, 1);
        assert_eq!(found.rows[0].cells[0], "High");
        // Selecting a row shows its analysis, and a window past the end is empty, not an error.
        let picked = queue
            .call(|panel| {
                panel.select_row(0);
                crate::library::view(panel, 50, 10)
            })
            .unwrap();
        assert!(picked.rows.is_empty());
        assert_eq!(picked.analysis.track.map(|t| t.analysed), Some(true));
        stop.store(true, Ordering::Release);
        handle.join().unwrap();
        // With the control thread gone, a call fails at once instead of waiting.
        assert!(queue.call(|_| ()).is_err());
    }

    #[test]
    fn without_a_library_a_call_says_so() {
        let engine = Arc::new(DjEngine::new(2).0);
        let control = control(&engine, None);
        let (stop, queue) = (Arc::clone(&control.stop), Arc::clone(&control.library));
        let handle = control.spawn(|_| {}, |_| {}).unwrap();
        assert_eq!(queue.call(|_| ()), Err("no library is open".to_owned()));
        stop.store(true, Ordering::Release);
        handle.join().unwrap();
    }

    #[test]
    fn a_track_loaded_from_the_browser_reaches_the_decks_and_the_row_shows_its_deck() {
        let dir = std::env::temp_dir().join(format!("om-ui-library-load-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("tone.wav");
        let samples = om_testing::signals::stereo_sine(220.0, 0.3, 44_100.0, 44_100);
        om_testing::wav::write_float_wav(&wav, 2, 44_100, &samples);
        let path = wav.to_string_lossy().into_owned();
        let setup: LibrarySetup = Box::new(move || {
            let mut options = om_ui::library::Options::for_path(om_ui::library::MEMORY_PATH);
            options.analysis_workers = 1;
            let mut services = om_ui::library::LibraryServices::create(options).ok()?;
            let track = om_library::TrackRecord {
                path,
                title: "Tone".to_owned(),
                ..Default::default()
            };
            services.library().tracks().insert(&track).ok()?;
            let loader = om_ui::library::TrackLoader::new(&mut services, 2);
            Some(crate::decks::DeckLibrary { services, loader })
        });
        let engine = Arc::new(DjEngine::new(2).0);
        let control = control(&engine, Some(setup));
        let (stop, queue, decks) = (
            Arc::clone(&control.stop),
            Arc::clone(&control.library),
            Arc::clone(&control.decks),
        );
        let handle = control.spawn(|_| {}, |_| {}).unwrap();
        let id = queue
            .call(|panel| panel.row(0).map(|row| row.track_id))
            .unwrap()
            .unwrap();
        let loaded = queue.call(move |panel| panel.load_track_id(1, id).is_ok());
        assert_eq!(loaded, Ok(true));
        let shown = || decks.read(1).is_some_and(|d| d.track.title == "Tone");
        assert!(wait_until(
            Duration::from_secs(5),
            Duration::from_millis(2),
            shown
        ));
        let view = queue
            .call(|panel| crate::library::view(panel, 0, 5))
            .unwrap();
        assert_eq!(view.rows[0].on_deck, Some(1));
        stop.store(true, Ordering::Release);
        handle.join().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
