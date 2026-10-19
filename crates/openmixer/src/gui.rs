//! `--features gui`: opens the GPUI window over a DjEngine. The device is output only. Without a
//! device (or with `--mock-device`) the engine runs on the silent mock device and a banner says so.
//! `--smoke` renders a fixed number of frames and exits 0. `--debug-panel` shows the device, latency
//! and raw parameter panel. The settings overlay (Settings button) edits the audio model, the
//! themes and the preferences; every change is saved and applied here.

use om_audio::LatencyStats;
use om_ui::preferences::Preferences;
use om_ui::settings::AudioSettingsModel;
use om_ui::theme::default_user_directory;
use om_ui_gpui::app::{run, DeviceStatusFn, RunOptions};
use om_ui_gpui::debug_panel::DeviceStatus;
use om_ui_gpui::settings_view::SettingsOptions;
use openmixer::app_targets::AudioDeviceControl;
use openmixer::audio_settings::{
    CpalBackend, MockBackend, OutputBackend, OutputSettings, SharedOutput,
};
use openmixer::composition::{load_themes, AppComposition};
use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;

/// Frames rendered by `--smoke` before it exits.
const SMOKE_FRAMES: u64 = 120;

pub fn main(args: &[String]) -> ExitCode {
    let mock = args.iter().any(|a| a == "--mock-device");
    let smoke = args.iter().any(|a| a == "--smoke");
    let debug = args.iter().any(|a| a == "--debug-panel");

    let mut composition = AppComposition::with_defaults();
    if let Err(error) = composition.attach_library(None) {
        eprintln!("openmixer: library unavailable: {error}");
    }
    let engine = Arc::clone(&composition.engine);
    let router = Arc::clone(&composition.router);
    let Some(audio) = composition.take_audio() else {
        eprintln!("openmixer: the audio side was already taken");
        return ExitCode::FAILURE;
    };
    let prefs_file = om_ui::preferences::default_file();
    let user_themes = default_user_directory(std::env::var_os("HOME").as_deref());

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

    let composition = Rc::new(RefCell::new(composition));
    let status_output = output.clone();
    let status: DeviceStatusFn = Arc::new(move || device_status(&status_output.lock()));
    let settings = SettingsOptions {
        prefs: composition.borrow().prefs.clone(),
        themes: load_themes(user_themes.as_deref()),
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
    };
    let options = RunOptions {
        smoke_frames: smoke.then_some(SMOKE_FRAMES),
        banner,
        debug: debug.then_some(status),
        settings: Some(settings),
    };
    let frames = run(Arc::clone(&engine), router, options);
    output.lock().stop();

    if smoke {
        println!("openmixer gui smoke ok: {frames} frames");
        if frames < SMOKE_FRAMES {
            eprintln!("openmixer: window closed after {frames} of {SMOKE_FRAMES} smoke frames");
            return ExitCode::FAILURE;
        }
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
    if let Err(error) = prefs.save(prefs_file) {
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
