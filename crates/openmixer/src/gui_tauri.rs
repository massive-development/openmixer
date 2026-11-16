//! `--features gui-tauri`, `--ui=tauri` (trial, ADR 0009): the Tauri window over the composition the
//! GPUI window uses (`gui.rs`). It runs only on the silent mock device: the trial never opens a sound
//! card. `--smoke` exits 0 once the front end has painted and called `ready`.

use om_ui::labels::{notice_text, Notice};
use om_ui::main_layout::resolve_density;
use om_ui_tauri::{RunOptions, ShellError};
use openmixer::audio_settings::SharedOutput;
use openmixer::composition::AppComposition;
use openmixer::startup::{mock_output, start_output};
use std::process::ExitCode;
use std::sync::Arc;

/// The window's opening size (`tauri.conf.json`); it picks the density an `Automatic` choice gets.
const WINDOW_SIZE: (i32, i32) = (1600, 780);

pub fn main(args: &[String]) -> ExitCode {
    if !args.iter().any(|a| a == "--mock-device") {
        eprintln!(
            "openmixer: --ui=tauri needs --mock-device (the Tauri trial never opens a sound card)"
        );
        return ExitCode::FAILURE;
    }
    let smoke = args.iter().any(|a| a == "--smoke");
    let mut composition = AppComposition::with_defaults();
    let Some(output) = start_mock_audio(&mut composition) else {
        return ExitCode::FAILURE;
    };
    let options = RunOptions {
        smoke,
        banner: Some(notice_text(Notice::Demo).to_owned()),
        bench_seconds: None,
    };
    let engine = Arc::clone(&composition.engine);
    let router = Arc::clone(&composition.router);
    let result = om_ui_tauri::run(engine, router, theme_css(&composition), options);
    output.lock().stop();
    report(result, smoke)
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

/// The active theme (the saved one, or `OPENMIXER_THEME`) as CSS, at the opening density.
fn theme_css(composition: &AppComposition) -> String {
    let tokens = composition.themes.current();
    let density = resolve_density(composition.prefs.density, tokens, WINDOW_SIZE);
    om_ui_bridge::theme_css(tokens, density)
}

fn report(result: Result<(), ShellError>, smoke: bool) -> ExitCode {
    match result {
        Ok(()) if smoke => {
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
