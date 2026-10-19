#![forbid(unsafe_code)]

//! `openmixer` binary. Headless flags run the engine and composition without a window; the GPUI window
//! is behind the `gui` feature (see `gui.rs`). `--smoke` is the headless start-up check.

#[cfg(feature = "gui")]
mod gui;

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
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        println!("openmixer {}", om_core::VERSION);
        return ExitCode::SUCCESS;
    }
    #[cfg(feature = "gui")]
    if args.is_empty() || args.iter().any(|a| a.starts_with("--")) {
        return gui::main(&args);
    }
    #[cfg(not(feature = "gui"))]
    if args
        .iter()
        .any(|a| a == "--mock-device" || a == "--debug-panel")
    {
        eprintln!("openmixer: --mock-device and --debug-panel need the window (build with --features gui)");
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
            eprintln!("usage: openmixer --smoke | --version (window: build with --features gui)");
            ExitCode::FAILURE
        }
    }
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
