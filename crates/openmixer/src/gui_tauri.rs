//! `--features gui-tauri`, `--ui=tauri` (trial, ADR 0009): the Tauri window over the composition the
//! GPUI window uses (`gui.rs`). It runs only on the silent mock device: the trial never opens a sound
//! card. `--demo-library` loads two generated tracks into decks A and B from a library in the
//! scratch folder (`--demo-scene=NAME` also stages the scene); without a demo flag the decks are
//! empty and no library is opened. `--smoke` exits 0 once the front end has received a snapshot
//! packet, painted and called `ready`; with a demo library the front end first starts deck A through
//! `control`, and the run fails unless the engine then reports deck A playing. `--ui-bench=SECONDS`
//! (1..=600) records the front end's frames for that long after a 3 s warm-up, writes the report to
//! `OPENMIXER_BENCH_OUT` (or `ui-bench.json` in the scratch folder) and exits 0; `--ui-fps-cap=N`
//! (0..=1000, 0 uncapped, default 60) caps the waveform lanes' frame rate. `--ui-drive-out=<path>`
//! (for `tools/ui-drive.py`) has the front end write its controls' rectangles to `<path>` and the
//! control thread rewrite the engine's state to `<path>.state` at 10 Hz; nothing is written without it.

use om_engine::DjEngine;
use om_ui::labels::{notice_text, Notice};
use om_ui::library::{LibraryServices, TrackLoader};
use om_ui::main_layout::resolve_density;
use om_ui::preferences::LibraryPrefs;
use om_ui_tauri::{
    DeckLibrary, DriveOut, LibrarySetup, RunOptions, ShellError, ShellTheme, DEFAULT_FPS_CAP,
};
use openmixer::audio_settings::SharedOutput;
use openmixer::composition::{library_options, AppComposition};
use openmixer::flags::DemoScene;
use openmixer::startup::{
    demo_user_track, ingest_demo_tracks, load_demo_deck_b, mock_output, start_output,
};
use openmixer::{demo_scene, library_smoke, AppError};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

/// The window's opening size (`tauri.conf.json`); it picks the density an `Automatic` choice gets.
const WINDOW_SIZE: (i32, i32) = (1600, 780);

/// The environment variable naming the bench report's file.
const BENCH_OUT_ENV: &str = "OPENMIXER_BENCH_OUT";

/// `--ui-bench=SECONDS` and `--ui-fps-cap=N` as the run takes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Frames {
    bench_seconds: Option<u32>,
    fps_cap: u32,
}

/// The flag naming the drive run's layout map; its state file sits beside it.
const DRIVE_FLAG: &str = "--ui-drive-out";

/// What the top bar shows beside the wordmark.
struct Shown {
    banner: Option<String>,
    hud: bool,
}

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
    let smoke = args.iter().any(|a| a == "--smoke");
    let parsed = demo_of(args)
        .and_then(|demo| Ok((demo, frames_of(args, smoke)?)))
        .and_then(|(demo, frames)| Ok((demo, frames, drive_of(args)?)));
    let (demo, frames, drive) = match parsed {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("openmixer: {message}");
            return ExitCode::FAILURE;
        }
    };
    let out = bench_out(
        std::env::var_os(BENCH_OUT_ENV),
        &library_smoke::scratch_dir(),
    );
    let scratch = demo.as_ref().map(|_| library_smoke::scratch_dir());
    let shown = Shown {
        banner: banner_of(args),
        hud: hud_of(args, frames),
    };
    let result = run_window(smoke, frames, &out, demo, scratch.as_deref(), shown, drive);
    // The library closed with the control thread; the scratch folder goes with the run, unless
    // it holds the bench report.
    let keep =
        frames.bench_seconds.is_some() && scratch.as_deref().is_some_and(|r| out.starts_with(r));
    if let Some(root) = scratch.filter(|_| !keep) {
        let _ = std::fs::remove_dir_all(root);
    }
    result
}

/// The mock-device notice, by GPUI's rule (`gui.rs`): every run here is on the mock device, and a
/// demo run (`flags::demo_run`) shows none, since it is staged for screenshots.
fn banner_of(args: &[String]) -> Option<String> {
    let demo = openmixer::flags::demo_run(args);
    (!demo).then(|| notice_text(Notice::Demo).to_owned())
}

/// The frame counter shows for whoever tunes the window: a bench run, or `--debug-panel` (the flag
/// that shows GPUI's device and latency panel).
fn hud_of(args: &[String], frames: Frames) -> bool {
    frames.bench_seconds.is_some() || args.iter().any(|a| a == "--debug-panel")
}

/// The bench and frame-cap flags; a bench cannot share a run with `--smoke`.
fn frames_of(args: &[String], smoke: bool) -> Result<Frames, String> {
    let bench_seconds = int_flag(args, "--ui-bench=", 1, 600)?;
    if smoke && bench_seconds.is_some() {
        return Err("--ui-bench and --smoke cannot run together".to_owned());
    }
    let fps_cap = int_flag(args, "--ui-fps-cap=", 0, 1000)?.unwrap_or(DEFAULT_FPS_CAP);
    Ok(Frames {
        bench_seconds,
        fps_cap,
    })
}

/// The integer after `prefix`, in `min..=max`; `Ok(None)` when the flag is absent.
fn int_flag(args: &[String], prefix: &str, min: u32, max: u32) -> Result<Option<u32>, String> {
    let Some(text) = args.iter().find_map(|a| a.strip_prefix(prefix)) else {
        return Ok(None);
    };
    let flag = prefix.trim_end_matches('=');
    match text.parse::<u32>() {
        Ok(n) if (min..=max).contains(&n) => Ok(Some(n)),
        _ => Err(format!(
            "{flag} needs a whole number from {min} to {max}, not {text:?}"
        )),
    }
}

/// `--ui-drive-out=<path>`: the caller names the file (nothing is written without the flag); it
/// must be a file in a folder that exists, so a typo fails at start rather than silently.
fn drive_of(args: &[String]) -> Result<Option<DriveOut>, String> {
    let Some(arg) = args.iter().find(|a| a.starts_with(DRIVE_FLAG)) else {
        return Ok(None);
    };
    let text = arg
        .strip_prefix(DRIVE_FLAG)
        .and_then(|rest| rest.strip_prefix('='))
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("{DRIVE_FLAG} needs a file path: {DRIVE_FLAG}=<path>"))?;
    drive_path(Path::new(text)).map(|path| Some(DriveOut::new(path)))
}

fn drive_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_dir() || text_ends_with_separator(path) {
        return Err(format!(
            "{DRIVE_FLAG} names a folder, not a file: {}",
            path.display()
        ));
    }
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    if parent.is_some_and(|p| !p.is_dir()) {
        return Err(format!("{DRIVE_FLAG}: no folder for {}", path.display()));
    }
    Ok(path.to_path_buf())
}

fn text_ends_with_separator(path: &Path) -> bool {
    path.as_os_str()
        .to_string_lossy()
        .ends_with(std::path::MAIN_SEPARATOR)
}

/// The bench report's file: `OPENMIXER_BENCH_OUT` when set and not empty, else in `scratch`
/// (never the user's folders).
fn bench_out(env: Option<OsString>, scratch: &Path) -> PathBuf {
    env.filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| scratch.join("ui-bench.json"))
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

fn run_window(
    smoke: bool,
    frames: Frames,
    out: &Path,
    demo: Option<Demo>,
    scratch: Option<&Path>,
    shown: Shown,
    drive_out: Option<DriveOut>,
) -> ExitCode {
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
        banner: shown.banner,
        hud: shown.hud,
        bench_seconds: frames.bench_seconds,
        bench_out: out.to_path_buf(),
        fps_cap: frames.fps_cap,
        drive_out,
    };
    let router = Arc::clone(&composition.router);
    let theme = shell_theme(&composition, engine.num_decks());
    let result = om_ui_tauri::run(Arc::clone(&engine), router, theme, library, options);
    let played = round_trip.then(|| deck_a_playing(&engine));
    output.lock().stop();
    if result.is_ok() && frames.bench_seconds.is_some() {
        println!("openmixer tauri bench written to {}", out.display());
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_mock_device_notice_shows_unless_a_demo_is_staged() {
        let notice = Some(notice_text(Notice::Demo).to_owned());
        assert_eq!(banner_of(&args(&["--mock-device"])), notice);
        assert_eq!(banner_of(&args(&["--mock-device", "--smoke"])), notice);
        for demo in [
            "--demo-library",
            "--demo-scene=cues",
            "--demo-map",
            "--demo-automix",
            "--demo-record",
        ] {
            assert_eq!(banner_of(&args(&["--mock-device", demo])), None, "{demo}");
        }
    }

    #[test]
    fn the_frame_counter_shows_for_a_bench_or_the_debug_panel_flag() {
        let hud = |list: &[&str]| {
            let a = args(list);
            frames_of(&a, false).map(|f| hud_of(&a, f))
        };
        assert_eq!(hud(&["--mock-device"]), Ok(false));
        assert_eq!(hud(&["--mock-device", "--ui-fps-cap=30"]), Ok(false));
        assert_eq!(hud(&["--mock-device", "--debug-panel"]), Ok(true));
        assert_eq!(hud(&["--mock-device", "--ui-bench=5"]), Ok(true));
        assert_eq!(hud(&["--debug-panel", "--demo-scene=cues"]), Ok(true));
    }

    #[test]
    fn frame_flags_default_to_no_bench_and_a_60_fps_cap() {
        let frames = frames_of(&args(&["--mock-device"]), false).unwrap();
        assert_eq!(
            frames,
            Frames {
                bench_seconds: None,
                fps_cap: 60
            }
        );
    }

    #[test]
    fn bench_seconds_parse_within_their_range() {
        let parse = |a: &str| frames_of(&args(&[a]), false).map(|f| f.bench_seconds);
        assert_eq!(parse("--ui-bench=1"), Ok(Some(1)));
        assert_eq!(parse("--ui-bench=600"), Ok(Some(600)));
        for bad in [
            "--ui-bench=0",
            "--ui-bench=601",
            "--ui-bench=",
            "--ui-bench=2.5",
            "--ui-bench=-3",
            "--ui-bench=ten",
        ] {
            let error = parse(bad).unwrap_err();
            assert!(
                error.contains("--ui-bench needs a whole number from 1 to 600"),
                "{bad}: {error}"
            );
        }
    }

    #[test]
    fn fps_cap_parses_zero_as_uncapped_and_refuses_out_of_range() {
        let parse = |a: &str| frames_of(&args(&[a]), false).map(|f| f.fps_cap);
        assert_eq!(parse("--ui-fps-cap=0"), Ok(0));
        assert_eq!(parse("--ui-fps-cap=1000"), Ok(1000));
        assert_eq!(parse("--ui-fps-cap=144"), Ok(144));
        assert!(parse("--ui-fps-cap=1001").is_err());
        assert!(parse("--ui-fps-cap=fast").is_err());
    }

    #[test]
    fn a_bench_refuses_the_smoke_run() {
        assert!(frames_of(&args(&["--ui-bench=5"]), true).is_err());
        assert!(frames_of(&args(&["--ui-fps-cap=30"]), true).is_ok());
    }

    #[test]
    fn no_drive_flag_writes_nothing() {
        assert_eq!(
            drive_of(&args(&["--mock-device", "--demo-library"])),
            Ok(None)
        );
    }

    #[test]
    fn the_drive_flag_names_the_layout_file_and_its_state_beside_it() {
        let dir = std::env::temp_dir();
        let file = dir.join("om-drive-test.json");
        let flag = format!("--ui-drive-out={}", file.display());
        let out = drive_of(&args(&["--mock-device", &flag])).unwrap().unwrap();
        assert_eq!(out.layout, file);
        assert_eq!(out.state, dir.join("om-drive-test.json.state"));
        let relative = drive_of(&args(&["--ui-drive-out=drive.json"]))
            .unwrap()
            .unwrap();
        assert_eq!(relative.layout, PathBuf::from("drive.json"));
    }

    #[test]
    fn the_drive_flag_refuses_no_path_a_folder_or_a_missing_folder() {
        let dir = std::env::temp_dir();
        let folder = format!("--ui-drive-out={}", dir.display());
        let trailing = format!("--ui-drive-out={}/", dir.join("x").display());
        let missing = format!(
            "--ui-drive-out={}",
            dir.join("om-no-such-dir-q").join("d.json").display()
        );
        for bad in [
            "--ui-drive-out",
            "--ui-drive-out=",
            "--ui-drive-outx",
            &folder,
            &trailing,
            &missing,
        ] {
            let error = drive_of(&args(&[bad])).unwrap_err();
            assert!(error.contains("--ui-drive-out"), "{bad}: {error}");
        }
    }

    #[test]
    fn bench_out_takes_the_environment_or_the_scratch_folder() {
        let scratch = Path::new("/tmp/openmixer-smoke-1");
        let env = Some(OsString::from("/x/bench.json"));
        assert_eq!(bench_out(env, scratch), PathBuf::from("/x/bench.json"));
        assert_eq!(
            bench_out(Some(OsString::new()), scratch),
            scratch.join("ui-bench.json")
        );
        assert_eq!(bench_out(None, scratch), scratch.join("ui-bench.json"));
    }
}
