//! `--features gui`: opens the GPUI window over a DjEngine. The device is output only. Without a
//! device (or with `--mock-device`) the engine runs on the silent mock device and a banner says so.
//! `--smoke` renders a fixed number of frames and exits 0; with the library it also ingests generated
//! tracks into a temporary library, loads one into deck A and checks the deck and the table after
//! the frames. `--debug-panel` shows the device, latency and raw parameter panel. The settings
//! overlay (Settings button) edits the audio model, the themes and the preferences; every change is
//! saved and applied here. The library browser opens the library from the preferences' folder (or
//! `OPENMIXER_LIBRARY_DB`); its analysis runs on background workers.

use om_audio::LatencyStats;
use om_engine::DjEngine;
use om_ui::library::StemSource;
use om_ui::preferences::Preferences;
use om_ui::settings::{learnable_targets, AudioSettingsModel, LibraryControl, MidiPage};
use om_ui::stems::{StemHost, StemJob, StemLabel};
use om_ui::theme::{default_user_directory, session_theme_override, ThemeRegistry};
use om_ui_gpui::app::{run, DeviceStatusFn, RunOptions};
use om_ui_gpui::debug_panel::DeviceStatus;
use om_ui_gpui::library::{HubLibrary, HubStems, LibraryHub};
use om_ui_gpui::settings_view::SettingsOptions;
use openmixer::app_targets::AudioDeviceControl;
use openmixer::audio_settings::{
    CpalBackend, MockBackend, OutputBackend, OutputSettings, SharedOutput,
};
use openmixer::composition::{load_themes, AppComposition, DECKS};
use openmixer::library_smoke::{self, TRACKS};
use openmixer::plugins::{sync_plugin_themes, PluginsPage};
use openmixer::AppError;
use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Frames rendered by `--smoke` before it exits.
const SMOKE_FRAMES: u64 = 120;

pub fn main(args: &[String]) -> ExitCode {
    let mock = args.iter().any(|a| a == "--mock-device");
    let smoke = args.iter().any(|a| a == "--smoke");
    let debug = args.iter().any(|a| a == "--debug-panel");
    // `--demo-library`: like the smoke run's library (generated tracks, deck A loaded) but the
    // window stays open; used by tools/screenshots.sh.
    let demo = args.iter().any(|a| a == "--demo-library");
    let start_tab = match openmixer::flags::settings_tab(args) {
        Ok(tab) => tab,
        Err(message) => {
            eprintln!("openmixer: {message}");
            return ExitCode::FAILURE;
        }
    };

    // The smoke run gets its own library in a temporary folder, never the user's.
    let smoke_root = (smoke || demo).then(library_smoke::scratch_dir);
    let mut composition = AppComposition::with_defaults();
    let library_path = smoke_root.as_ref().map(|root| root.join("library.db"));
    if let Err(error) = composition.attach_library(library_path) {
        eprintln!("openmixer: library unavailable: {error}");
    }
    if let Err(error) = composition.attach_stems(openmixer::flags::mock_stems(args)) {
        eprintln!("openmixer: stems unavailable: {error}");
    }
    // MIDI: the system backend on a real run; a mock controller under --mock-device and --smoke, so
    // neither run opens hardware.
    if mock || smoke {
        let midi = om_midi::MockBackend::new();
        midi.add_input("Mock controller");
        composition.attach_midi(Box::new(midi));
    } else {
        match om_midi::system_backend() {
            Ok(backend) => composition.attach_midi(backend),
            Err(error) => eprintln!("openmixer: MIDI unavailable: {error}"),
        }
    }
    let engine = Arc::clone(&composition.engine);
    let router = Arc::clone(&composition.router);
    let Some(audio) = composition.take_audio() else {
        eprintln!("openmixer: the audio side was already taken");
        return ExitCode::FAILURE;
    };
    let prefs_file = om_ui::preferences::default_file();
    let home = dirs::home_dir().map(std::path::PathBuf::into_os_string);
    let user_themes = default_user_directory(home.as_deref());

    // The device: cpal output at the stored setup, else the default output. A failure, or
    // `--mock-device`, falls back to the silent mock with a banner.
    let stored = composition.prefs.audio.device_state.clone();
    let (output, banner) = if mock {
        (
            mock_output(),
            Some("Mock device: silent, no audio output (--mock-device).".to_owned()),
        )
    } else {
        let cpal = SharedOutput::new(OutputSettings::new(Box::new(CpalBackend)));
        let opened = cpal.lock().start_up(&stored);
        match opened {
            Ok(()) => (cpal, None),
            Err(error) => {
                eprintln!(
                    "openmixer: no audio device ({error}); running silent on the mock device"
                );
                (
                    mock_output(),
                    Some("No audio device: running silent on the mock device.".to_owned()),
                )
            }
        }
    };
    let started_with = (!mock && banner.is_none() && !stored.is_empty()).then_some(stored.as_str());

    // Preferences reach the output routing and the device of the output that was actually opened.
    {
        let mut settings = output.lock();
        let pairs = settings.pairs();
        composition.apply_preferences(&pairs, Some(&mut *settings), started_with);
    }
    if let Err(error) = output.lock().start(Box::new(audio)) {
        eprintln!("openmixer: could not start the audio device: {error}");
        return ExitCode::FAILURE;
    }

    // The library: ingest the smoke tracks and load the first one into deck A, or share the
    // services the composition opened. The window and the smoke check use the same hub.
    let mut smoke_error: Option<AppError> = None;
    let library = composition.services.library.take().map(|mut services| {
        if let Some(root) = &smoke_root {
            if let Err(error) = library_smoke::ingest_generated(&mut services, root) {
                smoke_error = Some(error);
            }
        }
        LibraryHub::shared(services, engine.num_decks())
    });
    // The stems host (model, cache, prepare jobs) is shared by the browser and the settings page.
    if let Some(hub) = &library {
        if let Some(host) = composition.take_stems_host() {
            // Cached stems load with their track when the preference says so; the sink is the
            // same one the MIDI and UI controls use.
            let source = host.cache_root().map(|root| {
                let (sep_id, version) = StemHost::separator(&host);
                StemSource {
                    root,
                    sep_id,
                    version,
                }
            });
            let mut hub = hub.borrow_mut();
            hub.loader.configure_stems(
                source,
                composition.prefs.stems.auto_load,
                Some(composition.stem_command_sink()),
            );
            hub.stem_host = Some(Box::new(host));
        }
    }
    if let (Some(hub), true, None) = (&library, smoke || demo, &smoke_error) {
        let mut hub = hub.borrow_mut();
        let hub = &mut *hub;
        if let Err(error) =
            library_smoke::load_first_into_deck0(&mut hub.services, &mut hub.loader, &engine)
        {
            smoke_error = Some(error);
        }
    }
    // `--demo-library --mock-stems`: separate deck A's track through the mock so its stem pads are
    // live in the screenshots. Failures only say so on stderr; the demo runs without stems.
    if demo && openmixer::flags::mock_stems(args) && smoke_error.is_none() {
        if let Some(hub) = &library {
            if let Err(error) = demo_stems(&mut hub.borrow_mut(), &engine) {
                eprintln!("openmixer: demo stems: {error}");
            }
        }
    }
    let rows_out = Arc::new(AtomicUsize::new(0));

    let midi_page: Option<Arc<dyn MidiPage>> = composition
        .services
        .midi
        .clone()
        .map(|service| service as Arc<dyn MidiPage>);
    let composition = Rc::new(RefCell::new(composition));
    let status_output = output.clone();
    let status: DeviceStatusFn = Arc::new(move || device_status(&status_output.lock()));
    // The window starts on OPENMIXER_THEME, else the saved theme, else the default.
    let mut themes = load_themes(user_themes.as_deref());
    let saved_theme = composition.borrow().prefs.theme.clone();
    let env_theme = std::env::var(openmixer::composition::THEME_ENV).ok();
    for id in [
        session_theme_override(env_theme.as_deref()),
        Some(saved_theme.as_str()),
    ]
    .into_iter()
    .flatten()
    {
        if themes.select(id) {
            break;
        }
    }
    let settings = SettingsOptions {
        prefs: composition.borrow().prefs.clone(),
        themes,
        audio: Some(Box::new(output.clone()) as Box<dyn AudioSettingsModel>),
        audio_stats: {
            let readout = output.clone();
            Some(Arc::new(move || readout.lock().stats()))
        },
        on_changed: {
            let composition = Rc::clone(&composition);
            let output = output.clone();
            let prefs_file = prefs_file.clone();
            Box::new(move |prefs: &Preferences| {
                apply_changes(&composition, &output, prefs, &prefs_file)
            })
        },
        midi: midi_page,
        learn_targets: learnable_targets(DECKS),
        library: library
            .clone()
            .map(|hub| Box::new(HubLibrary(hub)) as Box<dyn LibraryControl>),
        stems: library
            .clone()
            .map(|hub| Box::new(HubStems(hub)) as Box<dyn StemHost>),
        start_tab: start_tab.map(str::to_owned),
        plugins: Some(Box::new(PluginsPage(Rc::clone(&composition)))),
        sync_themes: {
            let composition = Rc::clone(&composition);
            Some(Box::new(move |themes: &mut ThemeRegistry| {
                sync_plugin_themes(themes, &composition.borrow().plugins)
            }))
        },
    };
    let options = RunOptions {
        smoke_frames: smoke.then_some(SMOKE_FRAMES),
        banner,
        debug: debug.then_some(status),
        settings: Some(settings),
        library: library.clone(),
        library_rows_out: smoke.then(|| Arc::clone(&rows_out)),
        // The demo's tracks are sine tones with no tempo: give deck A a fixed grid once its
        // analysis lands, so screenshots show beat ticks.
        demo_grid_bpm: demo.then_some(120.0),
    };
    let frames = run(Arc::clone(&engine), router, options);
    output.lock().stop();

    let mut failed = false;
    if smoke {
        let rows = rows_out.load(Ordering::Relaxed);
        let loaded = library
            .as_ref()
            .is_some_and(|hub| hub.borrow().loader.loaded_track(0).is_some())
            && engine.deck(0).is_loaded();
        if smoke_error.is_none() {
            if !loaded {
                smoke_error = Some(AppError::Smoke("deck A is not loaded"));
            } else if rows != TRACKS {
                smoke_error = Some(AppError::SmokeRows {
                    rows,
                    expected: TRACKS,
                });
            }
        }
        {
            let composition = composition.borrow();
            println!("openmixer gui MIDI: {}", composition.midi_status());
            println!("openmixer gui plugins: {}", composition.plugins_status());
        }
        match &smoke_error {
            None => println!(
                "openmixer gui smoke ok: {frames} frames, {rows} library rows, deck A loaded"
            ),
            Some(error) => eprintln!("openmixer: gui smoke failed: {error}"),
        }
        if frames < SMOKE_FRAMES {
            eprintln!("openmixer: window closed after {frames} of {SMOKE_FRAMES} smoke frames");
            failed = true;
        }
        failed |= smoke_error.is_some();
    }
    // Let the library close its files before the scratch folder goes.
    drop(library);
    if let Some(root) = smoke_root {
        let _ = std::fs::remove_dir_all(root);
    }
    if failed {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// A saved change from the settings overlay: keep the composition's copy, save the file, and push
/// the sections that changed into the subsystems.
fn apply_changes(
    composition: &RefCell<AppComposition>,
    output: &SharedOutput,
    prefs: &Preferences,
    prefs_file: &std::path::Path,
) {
    let mut composition = composition.borrow_mut();
    composition.prefs = prefs.clone();
    if !composition.prefs_replaceable {
        eprintln!(
            "openmixer: not saving preferences: {} could not be read, so it is left as it is",
            prefs_file.display()
        );
    } else if let Err(error) = prefs.save(prefs_file) {
        eprintln!("openmixer: could not save preferences: {error}");
    }
    let mut settings = output.lock();
    let pairs = settings.pairs();
    let device: &mut dyn AudioDeviceControl = &mut *settings;
    composition.apply_preferences(&pairs, Some(device), None);
}

/// The silent mock output, started by the window like the cpal one.
fn mock_output() -> SharedOutput {
    let backend: Box<dyn OutputBackend> = Box::new(MockBackend::default());
    let output = SharedOutput::new(OutputSettings::new(backend));
    if let Err(error) = output.lock().start_up("") {
        // The default mock cannot fail to open; keep the window up with no device if it does.
        eprintln!("openmixer: mock device: {error}");
    }
    output
}

/// The debug panel's view of the device, from its latency monitor.
fn device_status(settings: &OutputSettings) -> DeviceStatus {
    let name_and_stats = settings.device().map(|device| {
        let s: LatencyStats = device.latency_stats();
        (device.name(), device, s)
    });
    match name_and_stats {
        Some((name, device, s)) => DeviceStatus {
            name,
            sample_rate: device.sample_rate(),
            buffer_size: device.buffer_size(),
            round_trip_ms: s.round_trip_latency_ms,
            callback_avg_us: s.avg_callback_us,
            callback_max_us: s.max_callback_us,
            budget_us: s.budget_us,
            cpu_load: s.cpu_load,
            xruns: s.xrun_count,
            callbacks: s.callback_count,
        },
        None => DeviceStatus {
            name: "No device open".to_owned(),
            sample_rate: 0.0,
            buffer_size: 0,
            round_trip_ms: 0.0,
            callback_avg_us: 0.0,
            callback_max_us: 0.0,
            budget_us: 0.0,
            cpu_load: 0.0,
            xruns: 0,
            callbacks: 0,
        },
    }
}

/// Prepares the stems of deck A's track through the stem host (the mock separator under
/// `--mock-stems`), waits for the job for a bounded time, and then loads the stems into deck 0.
fn demo_stems(hub: &mut LibraryHub, engine: &DjEngine) -> Result<(), String> {
    let track = hub
        .loader
        .loaded_track(0)
        .cloned()
        .ok_or("deck A has no track")?;
    let job = StemJob {
        track_id: track.id,
        path: std::path::PathBuf::from(&track.path),
        content_hash: track.content_hash.clone(),
    };
    let host = hub.stem_host.as_deref_mut().ok_or("no stems host")?;
    let (sep_id, version) = host.separator();
    hub.stems
        .prepare(host, hub.services.library(), std::slice::from_ref(&job))
        .map_err(|error| error.to_string())?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        match hub
            .stems
            .label(hub.services.library(), track.id, (&sep_id, version))
        {
            StemLabel::Ready => break,
            StemLabel::Failed => return Err("separation failed".into()),
            _ if std::time::Instant::now() > deadline => return Err("separation timed out".into()),
            _ => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
    hub.loader
        .toggle_stems(engine, &hub.services, 0)
        .map(|_| ())
        .map_err(|error| error.to_string())
}
