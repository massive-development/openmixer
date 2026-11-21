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
            return (
                self.decks.run_loads(None),
                self.decks.tick(&self.engine),
            );
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
}
