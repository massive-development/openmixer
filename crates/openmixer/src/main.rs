#![forbid(unsafe_code)]

//! `openmixer` binary. Headless flags run the engine and composition without a window; the Tauri window
//! (ADR 0009) is the only window and opens by default behind the `gui-tauri` feature (see `gui_tauri.rs`).
//! `--headless --smoke` is the start-up check that opens no window; without the feature
//! (`--no-default-features`) `--headless` is implied. `--ui=tauri` is still accepted and changes nothing.

#[cfg(feature = "gui-tauri")]
mod gui_tauri;

use om_audio::{AudioDevice, MockAudioDevice, OutputRouter};
use om_midi::MockBackend;
use openmixer::composition::{self, AppComposition};
use openmixer::AppError;
use std::process::ExitCode;

/// Headless smoke: sample rate, block size and number of blocks rendered into memory.
const SMOKE_RATE: f64 = 48_000.0;
const SMOKE_BLOCK: usize = 256;
const SMOKE_BLOCKS: usize = 120;

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let headless = args.iter().any(|a| a == "--headless");
    args.retain(|a| a != "--headless");
    // The window is the default build; the old opt-in flag is dropped so scripts that pass it still run.
    args.retain(|a| a != "--ui=tauri");
    if headless && args.iter().any(|a| is_window_only(a)) {
        eprintln!("openmixer: --ui-bench, --ui-fps-cap and --ui-drive-out need the window");
        return ExitCode::FAILURE;
    }
    if args.iter().any(|a| a == "--version") {
        println!("openmixer {}", om_core::VERSION);
        return ExitCode::SUCCESS;
    }
    #[cfg(feature = "gui-tauri")]
    if !headless {
        return gui_tauri::main(&args);
    }
    #[cfg(not(feature = "gui-tauri"))]
    if args
        .iter()
        .any(|a| a == "--mock-device" || a == "--debug-panel" || is_window_only(a))
    {
        eprintln!(
            "openmixer: the window flags need the window (build without --no-default-features)"
        );
        return ExitCode::FAILURE;
    }
    match args.first().map(String::as_str) {
        Some("--smoke") => match headless_smoke() {
            Ok(()) => {
                println!("openmixer {} smoke ok", om_core::VERSION);
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("openmixer: smoke failed: {message}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!(
                "usage: openmixer --headless --smoke | --version (the window is the default build)"
            );
            ExitCode::FAILURE
        }
    }
}

/// The window's frame and drive flags, which the headless run does not read.
fn is_window_only(arg: &str) -> bool {
    ["--ui-bench", "--ui-fps-cap", "--ui-drive-out"]
        .iter()
        .any(|flag| arg.starts_with(flag))
}

/// Builds the composition, starts the engine on the paced-free mock device, applies the saved
/// preferences and renders a fixed number of blocks into memory. Nothing reaches a sound card.
fn headless_smoke() -> Result<(), AppError> {
    let mut composition = AppComposition::with_defaults();
    let audio = composition
        .take_audio()
        .ok_or(AppError::Smoke("the audio side was already taken"))?;
    // MIDI over a mock device: the check exercises the wiring and never opens hardware.
    let midi = MockBackend::new();
    midi.add_device("Smoke Controller");
    composition.attach_midi(Box::new(midi));
    let mut device = MockAudioDevice::new(SMOKE_RATE, SMOKE_BLOCK, 0, 4);
    device.start(Box::new(audio))?;

    // The mock has no output router, so the pair selection goes to a detached one.
    let (_router, pairs) = OutputRouter::new();
    composition.apply_preferences(&pairs, None, None);

    let rendered = device.render_blocks(SMOKE_BLOCKS);
    device.stop();
    if rendered != SMOKE_BLOCKS {
        return Err(AppError::SmokeRendered {
            rendered,
            expected: SMOKE_BLOCKS,
        });
    }
    if composition.engine.num_decks() != composition::DECKS {
        return Err(AppError::Smoke("the engine has the wrong number of decks"));
    }
    if composition.router.unknown_count() != 0 {
        return Err(AppError::Smoke("parameter registrations failed"));
    }
    if composition.services.midi.is_none() {
        return Err(AppError::Smoke("MIDI did not attach"));
    }
    let stats = device.latency_stats();
    if stats.xrun_count != 0 {
        return Err(AppError::SmokeXruns(stats.xrun_count));
    }
    let finite = device.last_output().iter().flatten().all(|s| s.is_finite());
    if !finite {
        return Err(AppError::Smoke("non-finite output sample"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_only_flags_are_recognised_with_or_without_a_value() {
        for arg in [
            "--ui-bench=5",
            "--ui-fps-cap=30",
            "--ui-drive-out=/tmp/d.json",
            "--ui-drive-out",
        ] {
            assert!(is_window_only(arg), "{arg}");
        }
        for arg in ["--ui=tauri", "--mock-device", "--demo-library", "--smoke"] {
            assert!(!is_window_only(arg), "{arg}");
        }
    }
}
