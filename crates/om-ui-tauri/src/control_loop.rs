//! The control thread (`om-ui-control`): builds the demo library when there is one, then at 30 Hz
//! pumps the library and the track loader and reports the decks that changed. It owns the library
//! services; nothing here runs on the audio thread or waits on a plugin.

use crate::decks::{DeckChanged, Decks, LibrarySetup};
use crate::pacer::Pacer;
use om_engine::DjEngine;
use om_ui::library::LibraryServices;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

/// Name of the control thread.
pub const CONTROL_THREAD: &str = "om-ui-control";
/// How often the loader is pumped.
const CONTROL_HZ: f64 = 30.0;

/// What the control thread works on.
pub(crate) struct ControlLoop {
    pub(crate) engine: Arc<DjEngine>,
    pub(crate) decks: Arc<Decks>,
    pub(crate) setup: Option<LibrarySetup>,
    pub(crate) stop: Arc<AtomicBool>,
}

impl ControlLoop {
    /// Runs the loop on a thread named [`CONTROL_THREAD`]; `emit` is called with no lock held.
    pub(crate) fn spawn(
        self,
        emit: impl Fn(DeckChanged) + Send + 'static,
    ) -> std::io::Result<JoinHandle<()>> {
        std::thread::Builder::new()
            .name(CONTROL_THREAD.into())
            .spawn(move || self.run(emit))
    }

    fn run(mut self, emit: impl Fn(DeckChanged)) {
        let mut services = self.attach_library(&emit);
        let mut pacer = Pacer::new(CONTROL_HZ);
        while !self.stop.load(Ordering::Acquire) {
            if let Some(services) = services.as_mut() {
                services.pump();
            }
            let changes = self.decks.tick(&self.engine, services.as_ref());
            changes.into_iter().for_each(&emit);
            pacer.wait();
        }
    }

    // The services are built here because they cannot move between threads; the decks the loader
    // already holds are reported at once.
    fn attach_library(&mut self, emit: &impl Fn(DeckChanged)) -> Option<LibraryServices> {
        let library = self.setup.take().and_then(|setup| setup())?;
        let changes = self.decks.install(&self.engine, Box::new(library.loader));
        changes.into_iter().for_each(emit);
        Some(library.services)
    }
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
            setup,
            stop: Arc::new(AtomicBool::new(false)),
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
        let handle = control.spawn(emit).unwrap();
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
        let handle = control.spawn(|_| {}).unwrap();
        assert!(wait_until(
            Duration::from_secs(5),
            Duration::from_millis(2),
            || ran.load(Ordering::Acquire)
        ));
        stop.store(true, Ordering::Release);
        handle.join().unwrap();
    }
}
