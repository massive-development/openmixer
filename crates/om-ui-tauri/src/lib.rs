#![forbid(unsafe_code)]

//! The Tauri v2 window of the web front end trial (ADR 0009). It owns the window, the Tauri
//! commands, the snapshot stream thread and the control thread that pumps the track loader; the
//! composition (engine, router, theme, demo library) comes from the `openmixer` binary.
//! [`run`] runs on the main thread; see each module for the thread its code runs on.

mod commands;
mod control_loop;
mod deck_info;
mod decks;
mod pacer;
mod smoke;
mod stream;
mod stream_slot;
mod strip_ink;

pub use control_loop::CONTROL_THREAD;
pub use deck_info::{DeckInfo, PeakLevelDto};
pub use decks::{DeckChanged, DeckLibrary, LibrarySetup};
pub use smoke::SMOKE_TIMEOUT;
pub use stream::{run_stream, spawn_stream, PacketSink, StreamConfig, STREAM_THREAD};
pub use strip_ink::{strip_ink_dto, StripInkDto};

use commands::ShellState;
use control_loop::ControlLoop;
use decks::Decks;
use om_core::ParameterRouter;
use om_engine::DjEngine;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use stream_slot::StreamSlot;
use tauri::{App, Emitter};

/// The event the front end hears when a deck's track, waveform or hot cues change.
pub const DECK_CHANGED_EVENT: &str = "deck-changed";

/// How the window runs.
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// Exit with code 0 once the front end has received a packet, painted and called `ready`.
    pub smoke: bool,
    /// The notice the window shows (the mock device); the empty shell does not show it yet.
    pub banner: Option<String>,
    /// Frame benchmark length (the trial's measurement task); unused until then.
    pub bench_seconds: Option<u32>,
}

/// The active theme as the front end takes it: the `--om-*` variables and each deck's lane ink.
#[derive(Debug, Clone, Default)]
pub struct ShellTheme {
    pub css: String,
    /// One per engine deck, in deck order.
    pub strip_inks: Vec<StripInkDto>,
}

/// Why the window could not run, or a smoke run failed.
#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
    #[error("could not start the smoke watchdog: {0}")]
    Watchdog(std::io::Error),
    #[error("could not start the control thread: {0}")]
    Control(std::io::Error),
    #[error("the front end never called ready (is web/dist built?)")]
    NotReady,
    #[error("the window exited with code {0}")]
    Exit(i32),
}

/// Sets the process environment the webview reads. Call it first in `main`, before any thread
/// exists, because changing the environment races with threads that read it.
pub fn prepare_environment() {
    disable_dmabuf_renderer();
}

/// Opens the window over `engine` and `router`, styled by `theme`, and returns when it closes.
/// `library`, when given, is built on the control thread and loads the decks the front end shows.
pub fn run(
    engine: Arc<DjEngine>,
    router: Arc<ParameterRouter>,
    theme: ShellTheme,
    library: Option<LibrarySetup>,
    options: RunOptions,
) -> Result<(), ShellError> {
    let shared = Shared::new(&engine);
    let (state, ready_wait) = shared.shell_state(theme, options.smoke);
    let app = build_app(Arc::clone(&engine), router, state)?;
    if options.smoke {
        smoke::spawn_watchdog(app.handle().clone(), ready_wait)?;
    }
    let control = shared.spawn_control(&app, engine, library)?;
    let code = app.run_return(|_, _| {});
    shared.shut_down(control);
    smoke::outcome(options.smoke && !shared.ready.load(Ordering::Acquire), code)
}

/// State the window's threads share with `run`, which stops them after the window closes.
struct Shared {
    ready: Arc<AtomicBool>,
    streams: Arc<StreamSlot>,
    decks: Arc<Decks>,
    stop: Arc<AtomicBool>,
}

impl Shared {
    fn new(engine: &DjEngine) -> Self {
        Self {
            ready: Arc::new(AtomicBool::new(false)),
            streams: Arc::new(StreamSlot::default()),
            decks: Arc::new(Decks::new(engine)),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    fn shell_state(&self, theme: ShellTheme, smoke: bool) -> (ShellState, Receiver<()>) {
        let (ready_signal, ready_wait) = mpsc::channel();
        let state = ShellState {
            theme_css: theme.css,
            strip_inks: theme.strip_inks,
            smoke,
            ready: Arc::clone(&self.ready),
            ready_signal,
            streams: Arc::clone(&self.streams),
            decks: Arc::clone(&self.decks),
        };
        (state, ready_wait)
    }

    fn spawn_control(
        &self,
        app: &App,
        engine: Arc<DjEngine>,
        setup: Option<LibrarySetup>,
    ) -> Result<JoinHandle<()>, ShellError> {
        let control = ControlLoop {
            engine,
            decks: Arc::clone(&self.decks),
            setup,
            stop: Arc::clone(&self.stop),
        };
        let handle = app.handle().clone();
        let emit = move |change: DeckChanged| {
            if let Err(error) = handle.emit(DECK_CHANGED_EVENT, change) {
                tracing::debug!(%error, "deck-changed not delivered");
            }
        };
        control.spawn(emit).map_err(ShellError::Control)
    }

    fn shut_down(&self, control: JoinHandle<()>) {
        self.streams.close();
        self.stop.store(true, Ordering::Release);
        if control.join().is_err() {
            tracing::error!("the control thread panicked");
        }
    }
}

fn build_app(
    engine: Arc<DjEngine>,
    router: Arc<ParameterRouter>,
    state: ShellState,
) -> Result<App, ShellError> {
    let app = tauri::Builder::default()
        .manage(engine)
        .manage(router)
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::theme_css,
            commands::smoke,
            commands::ready,
            commands::waveform_style,
            commands::strip_ink,
            commands::control,
            commands::control_ids,
            commands::start_stream,
            commands::deck_peaks,
            commands::deck_info,
        ])
        .build(tauri::generate_context!())?;
    Ok(app)
}

/// Locks `mutex`, taking the value over from a thread that panicked while holding it.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// ADR 0009: under WSL software rendering this cut WebKitGTK from 213% to 56% CPU.
#[cfg(target_os = "linux")]
fn disable_dmabuf_renderer() {
    const VAR: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
    if std::env::var_os(VAR).is_none() {
        std::env::set_var(VAR, "1");
    }
}

#[cfg(not(target_os = "linux"))]
fn disable_dmabuf_renderer() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_poisoned_lock_is_taken_over() {
        let mutex = Arc::new(Mutex::new(1));
        let held = Arc::clone(&mutex);
        let _ = std::thread::spawn(move || {
            let _guard = held.lock().unwrap();
            panic!("poison it");
        })
        .join();
        assert!(mutex.is_poisoned());
        *lock(&mutex) += 1;
        assert_eq!(*lock(&mutex), 2);
    }
}
