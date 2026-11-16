//! `--features gui-tauri`, `--ui=tauri` (trial, ADR 0009): the Tauri window over the composition the
//! GPUI window uses (`gui.rs`). It runs only on the silent mock device: the trial never opens a sound
//! card. `--demo-library` loads two generated tracks into decks A and B from a library in the
//! scratch folder (`--demo-scene=NAME` also stages the scene); without a demo flag the decks are
//! empty and no library is opened. `--smoke` exits 0 once the front end has received a snapshot
//! packet, painted and called `ready`; with a demo library the front end first starts deck A through
//! `control`, and the run fails unless the engine then reports deck A playing.

use om_engine::DjEngine;
use om_ui::labels::{notice_text, Notice};
use om_ui::library::{LibraryServices, TrackLoader};
use om_ui::main_layout::resolve_density;
use om_ui::preferences::LibraryPrefs;
use om_ui_tauri::{DeckLibrary, LibrarySetup, RunOptions, ShellError, ShellTheme};
use openmixer::audio_settings::SharedOutput;
use openmixer::composition::{library_options, AppComposition};
use openmixer::flags::DemoScene;
use openmixer::startup::{
    demo_user_track, ingest_demo_tracks, load_demo_deck_b, mock_output, start_output,
};
use openmixer::{demo_scene, library_smoke, AppError};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

/// The window's opening size (`tauri.conf.json`); it picks the density an `Automatic` choice gets.
const WINDOW_SIZE: (i32, i32) = (1600, 780);

/// What a demo run stages: the scene, and the user's file for `--demo-scene=track`.
struct Demo {
    scene: Option<DemoScene>,
    user_track: Option<PathBuf>,
}

pub fn main(args: &[String]) -> ExitCode {
    // Before any thread exists: the environment is not safe to change once threads read it.
    om_ui_tauri::prepare_environment();
    if !args.iter().any(|a| a == "--mock-device") {
        eprintln!(
            "openmixer: --ui=tauri needs --mock-device (the Tauri trial never opens a sound card)"
        );
        return ExitCode::FAILURE;
    }
    let demo = match demo_of(args) {
        Ok(demo) => demo,
        Err(message) => {
            eprintln!("openmixer: {message}");
            return ExitCode::FAILURE;
        }
    };
    let smoke = args.iter().any(|a| a == "--smoke");
    let scratch = demo.as_ref().map(|_| library_smoke::scratch_dir());
    let result = run_window(smoke, demo, scratch.as_deref());
    if let Some(root) = scratch {
        // The library closed with the control thread; the scratch folder goes with the run.
        let _ = std::fs::remove_dir_all(root);
    }
    result
}

/// `Some` for `--demo-library` or `--demo-scene=NAME`; `Err` names a bad scene or a missing file.
fn demo_of(args: &[String]) -> Result<Option<Demo>, String> {
    let scene = openmixer::flags::demo_scene(args)?;
    if scene.is_none() && !args.iter().any(|a| a == "--demo-library") {
        return Ok(None);
    }
    let user_track = demo_user_track(scene)?;
    Ok(Some(Demo { scene, user_track }))
}

fn run_window(smoke: bool, demo: Option<Demo>, scratch: Option<&Path>) -> ExitCode {
    let mut composition = AppComposition::with_defaults();
    let Some(output) = start_mock_audio(&mut composition) else {
        return ExitCode::FAILURE;
    };
    let engine = Arc::clone(&composition.engine);
    let library = demo.zip(scratch).map(|(demo, root)| {
        demo_library(root, &composition.prefs.library, Arc::clone(&engine), demo)
    });
    let round_trip = smoke && library.is_some();
    let options = RunOptions {
        smoke,
        banner: Some(notice_text(Notice::Demo).to_owned()),
        bench_seconds: None,
    };
    let router = Arc::clone(&composition.router);
    let theme = shell_theme(&composition, engine.num_decks());
    let result = om_ui_tauri::run(Arc::clone(&engine), router, theme, library, options);
    let played = round_trip.then(|| deck_a_playing(&engine));
    output.lock().stop();
    report(result, smoke, played)
}

/// The control round trip's verdict: the front end's `deck0.play` reached the engine.
fn deck_a_playing(engine: &DjEngine) -> bool {
    engine.snapshot().decks.first().is_some_and(|d| d.playing)
}

/// The engine on the silent mock device with the preferences applied; `None` once it said why.
fn start_mock_audio(composition: &mut AppComposition) -> Option<SharedOutput> {
    let Some(audio) = composition.take_audio() else {
        eprintln!("openmixer: the audio side was already taken");
        return None;
    };
    let output = mock_output();
    match start_output(composition, &output, audio, None) {
        Ok(()) => Some(output),
        Err(error) => {
            eprintln!("openmixer: could not start the audio device: {error}");
            None
        }
    }
}

/// Opens the scratch library and loads the demo decks, on the window's control thread (the
/// library services cannot move between threads). Never the user's library.
fn demo_library(
    root: &Path,
    prefs: &LibraryPrefs,
    engine: Arc<DjEngine>,
    demo: Demo,
) -> LibrarySetup {
    let root = root.to_path_buf();
    let options_prefs = prefs.clone();
    Box::new(move || {
        let options = library_options(&options_prefs, Some(root.join("library.db")));
        let mut services = LibraryServices::create(options)
            .map_err(|error| eprintln!("openmixer: library unavailable: {error}"))
            .ok()?;
        let mut loader = TrackLoader::new(&mut services, engine.num_decks());
        if let Err(error) = load_demo_decks(&mut services, &mut loader, &engine, &root, &demo) {
            eprintln!("openmixer: demo decks: {error}");
        }
        Some(DeckLibrary { services, loader })
    })
}

/// As the GPUI demo: ingest, deck A, deck B (parked), then the scene on deck A.
fn load_demo_decks(
    services: &mut LibraryServices,
    loader: &mut TrackLoader,
    engine: &DjEngine,
    root: &Path,
    demo: &Demo,
) -> Result<(), AppError> {
    ingest_demo_tracks(services, root, demo.scene, demo.user_track.as_deref())?;
    library_smoke::load_first_into_deck0(services, loader, engine)?;
    load_demo_deck_b(services, loader, engine, demo.scene);
    if let Some(scene) = demo.scene {
        demo_scene::apply(scene, engine);
    }
    Ok(())
}

/// The active theme (the saved one, or `OPENMIXER_THEME`) as CSS at the opening density, and the
/// waveform lane ink of each of the engine's `decks`.
fn shell_theme(composition: &AppComposition, decks: usize) -> ShellTheme {
    let tokens = composition.themes.current();
    let density = resolve_density(composition.prefs.density, tokens, WINDOW_SIZE);
    ShellTheme {
        css: om_ui_bridge::theme_css(tokens, density),
        strip_inks: (0..decks)
            .map(|deck| om_ui_tauri::strip_ink_dto(tokens, deck))
            .collect(),
    }
}

/// `played` is the round trip's verdict, when the smoke run made one.
fn report(result: Result<(), ShellError>, smoke: bool, played: Option<bool>) -> ExitCode {
    match result {
        Ok(()) if played == Some(false) => {
            eprintln!("openmixer: tauri smoke: deck A is not playing after the front end's play");
            ExitCode::FAILURE
        }
        Ok(()) if smoke => {
            if played == Some(true) {
                println!("openmixer tauri control round trip ok");
            }
            println!("openmixer tauri smoke ok");
            ExitCode::SUCCESS
        }
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("openmixer: tauri window: {error}");
            ExitCode::FAILURE
        }
    }
}
