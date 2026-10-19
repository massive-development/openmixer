//! `--features gui`: opens the GPUI window over a DjEngine. The device is output only. Without a
//! device (or with `--mock-device`) the engine runs on the silent mock device and a banner says so.
//! `--smoke` renders a fixed number of frames and exits 0.

use om_audio::{AudioDevice, CpalAudioDevice, MockAudioDevice, OutputRequest};
use om_core::ParameterRouter;
use om_engine::DjEngine;
use om_ui_gpui::app::{run, RunOptions};
use std::process::ExitCode;
use std::sync::Arc;

/// Frames rendered by `--smoke` before it exits.
const SMOKE_FRAMES: u64 = 120;
const DECKS: usize = 2;

pub fn main(args: &[String]) -> ExitCode {
    let mock = args.iter().any(|a| a == "--mock-device");
    let smoke = args.iter().any(|a| a == "--smoke");

    let (engine, audio) = DjEngine::new(DECKS);
    let engine = Arc::new(engine);
    let router = Arc::new(ParameterRouter::new());
    engine.register(&router);

    let options = |banner: Option<String>| RunOptions {
        smoke_frames: smoke.then_some(SMOKE_FRAMES),
        banner,
    };
    let cpal = if mock {
        None
    } else {
        match CpalAudioDevice::open(&OutputRequest {
            device: None,
            sample_rate: None,
            buffer_size: None,
            channels: None,
        }) {
            Ok(device) => Some(device),
            Err(error) => {
                eprintln!("openmixer: no audio device ({error}); running silent on the mock device");
                None
            }
        }
    };

    let frames = match cpal {
        Some(mut device) => {
            if let Err(error) = device.start(Box::new(audio)) {
                eprintln!("openmixer: could not start the audio device: {error}");
                return ExitCode::FAILURE;
            }
            let frames = run(Arc::clone(&engine), router, options(None));
            device.stop();
            frames
        }
        None => {
            let banner = if mock {
                "Mock device: silent, no audio output (--mock-device)."
            } else {
                "No audio device: running silent on the mock device."
            };
            let mut device = MockAudioDevice::new(48_000.0, 256, 0, 4);
            if let Err(error) = device.start(Box::new(audio)) {
                eprintln!("openmixer: could not start the mock device: {error}");
                return ExitCode::FAILURE;
            }
            device.start_realtime();
            let frames = run(Arc::clone(&engine), router, options(Some(banner.to_owned())));
            device.stop_realtime();
            device.stop();
            frames
        }
    };

    if smoke {
        println!("openmixer gui smoke ok: {frames} frames");
        if frames < SMOKE_FRAMES {
            eprintln!("openmixer: window closed after {frames} of {SMOKE_FRAMES} smoke frames");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
