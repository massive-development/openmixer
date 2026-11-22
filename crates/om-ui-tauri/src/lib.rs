#![forbid(unsafe_code)]

//! The Tauri v2 window of the web front end trial (ADR 0009). It owns the window, the Tauri
//! commands, the snapshot stream thread and the control thread that pumps the track loader; the
//! composition (engine, router, theme, demo library) comes from the `openmixer` binary.
//! [`run`] runs on the main thread; see each module for the thread its code runs on.

mod bench;
mod commands;
mod control_loop;
mod controls;
mod deck_info;
mod decks;
mod devices;
mod drive;
mod header;
mod instruments;
mod library;
mod library_commands;
mod pacer;
mod plugin_window;
mod settings;
mod smoke;
mod stems_settings;
mod stream;
mod stream_slot;
mod strip_ink;

pub use bench::{parse_report, write_report, BenchError, BenchReport, Summary};
pub use commands::RunConfig;
pub use control_loop::CONTROL_THREAD;
pub use controls::{control_dtos, ControlDto};
pub use deck_info::{DeckInfo, PeakLevelDto, PEAKS_HEADER_LEN};
pub use decks::{DeckChanged, DeckLibrary, LibrarySetup, LoadFailed};
pub use devices::{
    Choice, ClapSection, DevicesHost, MappingChange, MappingConflict, MappingFilter, MappingForm,
    MappingPage, MappingRow, MidiChange, MidiPage, Notice, PluginParam, PluginRow, PluginsChange,
    PluginsPage, View,
};
pub use drive::{
    drive_state, parse_layout, write_json_atomic, ControlRect, DriveError, DriveLayout, DriveOut,
    DriveState, LayoutMap, Viewport,
};
pub use header::{
    automix_dto, install_header, next_length, record_dto, AutomixAction, AutomixBarDto, HeaderHost,
    HeaderParts, HeaderState, RecordBarDto, TickSetup, Ticker,
};
pub use instruments::{PadDto, SamplerDto, StemDto, StemsDto, SAMPLER_EVENT, STEMS_EVENT};
pub use library::{
    AnalysisDto, ColumnDto, LibraryView, NamingDto, RowDto, SidebarDto, TrackAnalysisDto,
    MAX_VIEW_ROWS,
};
pub use settings::{
    AppearanceChange, AppearancePage, AppearanceResult, AudioChange, AudioPage, AutomixChange,
    AutomixPage, FormatChoice, LengthChoice, QueueSource, RecordChange, RecordPage, SettingsHost,
    StemsPrefsChange, StemsPrefsPage, ThemeChoice,
};
pub use smoke::{bench_timeout, SMOKE_TIMEOUT};
pub use stream::{run_stream, spawn_stream, PacketSink, StreamConfig, STREAM_THREAD};
pub use strip_ink::{strip_ink_dto, StripInkDto};

use bench::BenchRun;
use commands::ShellState;
use control_loop::ControlLoop;
use decks::Decks;
use instruments::{InstrumentEvent, Instruments};
use library::LibraryQueue;
use om_core::ParameterRouter;
use om_engine::DjEngine;
use om_ui::sample_bank::SampleBank;
use om_ui::sampler_binding::SamplerBinding;
use om_ui::stem_binding::StemBinding;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use stream_slot::StreamSlot;
use tauri::{App, Emitter};

/// The event the front end hears when a deck's track, waveform or hot cues change.
pub const DECK_CHANGED_EVENT: &str = "deck-changed";
/// Event the control thread emits when a load the front end asked for did not happen.
pub const LOAD_FAILED_EVENT: &str = "load-failed";

/// The lanes' frame cap when none is given (the plan's default).
pub const DEFAULT_FPS_CAP: u32 = 60;

/// How the window runs.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Exit with code 0 once the front end has received a packet, painted and called `ready`. With
    /// a library, the front end first starts deck A (loaded by then) through `control`.
    pub smoke: bool,
    /// The notice the top bar shows (the mock device); `None` for none.
    pub banner: Option<String>,
    /// Show the frame counter in the top bar (a bench run or `--debug-panel`).
    pub hud: bool,
    /// Frame benchmark length in seconds (`--ui-bench`): the front end records its frames, the
    /// report goes to `bench_out` and the window exits 0.
    pub bench_seconds: Option<u32>,
    /// Where a bench run writes its report.
    pub bench_out: PathBuf,
    /// The waveform lanes' frame cap; 0 is uncapped.
    pub fps_cap: u32,
    /// `--ui-drive-out`: where the front end's layout map and the engine's state go; `None` writes
    /// neither.
    pub drive_out: Option<DriveOut>,
    /// Open the sampler drawer at start (`--demo-scene=sampler`).
    pub sampler_open: bool,
    /// Start deck A on its stems tab (`--demo-scene=stems`).
    pub stems_tab: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            smoke: false,
            banner: None,
            hud: false,
            bench_seconds: None,
            bench_out: std::env::temp_dir().join("openmixer-ui-bench.json"),
            fps_cap: DEFAULT_FPS_CAP,
            drive_out: None,
            sampler_open: false,
            stems_tab: false,
        }
    }
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
    #[error("bench: {0}")]
    Bench(String),
    #[error("bench: the front end never sent its frame statistics")]
    NoBenchResult,
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
    samples: Option<Arc<SampleBank>>,
    settings: Option<Box<dyn SettingsHost>>,
    options: RunOptions,
) -> Result<(), ShellError> {
    let shared = Shared::new(&engine, &router, samples);
    // A smoke run with a library proves a control round trip on the deck it loads.
    let smoke_play = options.smoke && library.is_some();
    let (bench, bench_wait) = bench_run(&options);
    let (state, ready_wait) = shared.shell_state(theme, &options, smoke_play, bench.clone());
    settings::install(settings);
    let header = header::take_installed();
    let (host, tick) = match header {
        Some(parts) => (Some(parts.host), parts.tick),
        None => (None, None),
    };
    let app = build_app(Arc::clone(&engine), router, state, host)?;
    if options.smoke {
        smoke::spawn_watchdog(app.handle().clone(), ready_wait, SMOKE_TIMEOUT)?;
    }
    if let (Some(seconds), Some(wait)) = (options.bench_seconds, bench_wait) {
        smoke::spawn_watchdog(app.handle().clone(), wait, smoke::bench_timeout(seconds))?;
    }
    let drive_state = options.drive_out.as_ref().map(|d| d.state.clone());
    let control = shared.spawn_control(&app, engine, library, tick, drive_state)?;
    let code = app.run_return(|_, _| {});
    settings::uninstall();
    shared.shut_down(control);
    bench_outcome(bench.as_deref())?;
    smoke::outcome(options.smoke && !shared.ready.load(Ordering::Acquire), code)
}

/// The bench run's shared state and the watchdog's end of its signal, for a `--ui-bench` run.
fn bench_run(options: &RunOptions) -> (Option<Arc<BenchRun>>, Option<Receiver<()>>) {
    let Some(seconds) = options.bench_seconds else {
        return (None, None);
    };
    let (done, wait) = mpsc::channel();
    let run = BenchRun {
        seconds,
        out: options.bench_out.clone(),
        outcome: Mutex::new(None),
        done,
    };
    (Some(Arc::new(run)), Some(wait))
}

/// A bench run fails when its report was refused or never came; its error says why.
fn bench_outcome(bench: Option<&BenchRun>) -> Result<(), ShellError> {
    let Some(bench) = bench else { return Ok(()) };
    match lock(&bench.outcome).take() {
        Some(Ok(())) => Ok(()),
        Some(Err(message)) => Err(ShellError::Bench(message)),
        None => Err(ShellError::NoBenchResult),
    }
}

/// State the window's threads share with `run`, which stops them after the window closes.
struct Shared {
    ready: Arc<AtomicBool>,
    streams: Arc<StreamSlot>,
    decks: Arc<Decks>,
    instruments: Arc<Instruments>,
    library: Arc<LibraryQueue>,
    stop: Arc<AtomicBool>,
}

impl Shared {
    fn new(
        engine: &Arc<DjEngine>,
        router: &Arc<ParameterRouter>,
        samples: Option<Arc<SampleBank>>,
    ) -> Self {
        let sampler = samples.map(|bank| {
            SamplerBinding::new(
                Arc::clone(engine),
                Some(router.clone()),
                router.clone(),
                Some(bank),
            )
        });
        let stems = (0..engine.num_decks())
            .map(|deck| StemBinding::typed(Arc::clone(engine), Arc::clone(router), deck))
            .collect();
        Self {
            instruments: Arc::new(Instruments::new(sampler, stems)),
            ready: Arc::new(AtomicBool::new(false)),
            streams: Arc::new(StreamSlot::default()),
            decks: Arc::new(Decks::new(engine)),
            library: Arc::default(),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    fn shell_state(
        &self,
        theme: ShellTheme,
        options: &RunOptions,
        smoke_play: bool,
        bench: Option<Arc<BenchRun>>,
    ) -> (ShellState, Receiver<()>) {
        let (ready_signal, ready_wait) = mpsc::channel();
        let state = ShellState {
            theme: Mutex::new(theme),
            config: RunConfig {
                smoke: options.smoke,
                bench_seconds: options.bench_seconds,
                fps_cap: options.fps_cap,
                banner: options.banner.clone(),
                hud: options.hud,
                drive: options.drive_out.is_some(),
                sampler_open: options.sampler_open,
                stems_tab: options.stems_tab,
            },
            drive_layout: options.drive_out.as_ref().map(|d| d.layout.clone()),
            bench,
            smoke_play,
            ready: Arc::clone(&self.ready),
            ready_signal,
            streams: Arc::clone(&self.streams),
            decks: Arc::clone(&self.decks),
            instruments: Arc::clone(&self.instruments),
            library: Arc::clone(&self.library),
        };
        (state, ready_wait)
    }

    fn spawn_control(
        &self,
        app: &App,
        engine: Arc<DjEngine>,
        setup: Option<LibrarySetup>,
        tick: Option<TickSetup>,
        drive_state: Option<PathBuf>,
    ) -> Result<JoinHandle<()>, ShellError> {
        let control = ControlLoop {
            engine,
            decks: Arc::clone(&self.decks),
            library: Arc::clone(&self.library),
            setup,
            tick,
            stop: Arc::clone(&self.stop),
            drive_state,
            instruments: Arc::clone(&self.instruments),
        };
        let handle = app.handle().clone();
        let emit = move |change: DeckChanged| {
            if let Err(error) = handle.emit(DECK_CHANGED_EVENT, change) {
                tracing::debug!(%error, "deck-changed not delivered");
            }
        };
        let handle = app.handle().clone();
        let failed = move |failure: LoadFailed| {
            tracing::warn!(deck = failure.deck, file = %failure.file, reason = %failure.reason, "load failed");
            if let Err(error) = handle.emit(LOAD_FAILED_EVENT, failure) {
                tracing::debug!(%error, "load-failed not delivered");
            }
        };
        let handle = app.handle().clone();
        let instrument = move |event: InstrumentEvent| {
            let sent = match event {
                InstrumentEvent::Sampler(state) => handle.emit(SAMPLER_EVENT, state),
                InstrumentEvent::Stems(state) => handle.emit(STEMS_EVENT, state),
            };
            if let Err(error) = sent {
                tracing::debug!(%error, "instrument state not delivered");
            }
        };
        control
            .spawn(emit, failed, instrument)
            .map_err(ShellError::Control)
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
    header: Option<Arc<dyn HeaderHost>>,
) -> Result<App, ShellError> {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(engine)
        .manage(router)
        .manage(state)
        .manage(header::HeaderSlot(header))
        .invoke_handler(tauri::generate_handler![
            commands::theme_css,
            commands::run_config,
            commands::bench_done,
            commands::smoke_play,
            commands::ready,
            commands::waveform_style,
            commands::strip_ink,
            commands::control,
            commands::controls,
            commands::start_stream,
            commands::deck_peaks,
            commands::deck_info,
            commands::load_file,
            commands::ui_layout,
            commands::sampler_state,
            commands::sampler_press,
            commands::sampler_release,
            commands::sampler_select,
            commands::sampler_cycle_mode,
            commands::sampler_set_gain,
            commands::sampler_set_pitch,
            commands::sampler_load,
            commands::sampler_clear,
            commands::sampler_stop_all,
            commands::stems_state,
            commands::stem_volume,
            commands::stem_filter,
            commands::stem_mute,
            commands::stem_solo,
            commands::stem_all_on,
            commands::stem_toggle,
            commands::stem_prepare,
            stems_settings::stems_settings,
            stems_settings::stems_settings_change,
            stems_settings::stems_accept_download,
            stems_settings::stems_cancel_download,
            stems_settings::stems_clear_cache,
            commands::audio_settings,
            commands::audio_change,
            commands::midi_page,
            commands::midi_change,
            commands::mapping_page,
            commands::mapping_change,
            commands::plugins_page,
            commands::plugins_change,
            commands::open_plugin_window,
            commands::appearance_settings,
            commands::appearance_change,
            library_commands::library_view,
            library_commands::library_search,
            library_commands::library_clear_search,
            library_commands::library_select_sidebar,
            library_commands::library_sort,
            library_commands::library_select_row,
            library_commands::library_key,
            library_commands::library_activate,
            library_commands::library_load,
            library_commands::library_analyse,
            library_commands::library_stems,
            library_commands::library_name,
            library_commands::library_delete_playlist,
            library_commands::library_remove_from_playlist,
            library_commands::library_drop_on_playlist,
            library_commands::library_drop_playlist,
            library_commands::library_drop_on_row,
            library_commands::library_copy,
            header::header_state,
            header::automix_enable,
            header::automix_length,
            header::automix_press,
            header::record_toggle,
            commands::automix_settings,
            commands::automix_change,
            commands::record_settings,
            commands::record_change,
            header::automix_playlists,
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
