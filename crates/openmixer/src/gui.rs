//! `--features gui`: opens the GPUI window over a DjEngine. The device is output only. Without a
//! device (or with `--mock-device`) the engine runs on the silent mock device and a banner says so.
//! `--smoke` renders a fixed number of frames and exits 0. `--debug-panel` shows the device, latency
//! and raw parameter panel.

use om_audio::{AudioDevice, AudioError, CpalAudioDevice, LatencyMonitor, LatencyStats, MockAudioDevice, OutputRequest};
use om_core::ParameterRouter;
use om_engine::{AudioCallback, DjEngine};
use om_ui_gpui::app::{run, DeviceStatusFn, RunOptions};
use om_ui_gpui::debug_panel::DeviceStatus;
use std::process::ExitCode;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Frames rendered by `--smoke` before it exits.
const SMOKE_FRAMES: u64 = 120;
const DECKS: usize = 2;
const MOCK_RATE: f64 = 48_000.0;
const MOCK_BLOCK: usize = 256;

/// The audio device the window drives: cpal output, or the paced silent mock. Delegates the
/// [`AudioDevice`] contract so the debug panel can read it from the UI thread.
enum Device {
    Cpal(CpalAudioDevice),
    Mock(MockAudioDevice),
}

impl Device {
    fn inner(&self) -> &dyn AudioDevice {
        match self {
            Device::Cpal(d) => d,
            Device::Mock(d) => d,
        }
    }

    fn inner_mut(&mut self) -> &mut dyn AudioDevice {
        match self {
            Device::Cpal(d) => d,
            Device::Mock(d) => d,
        }
    }
}

impl AudioDevice for Device {
    fn start(&mut self, callback: Box<dyn AudioCallback>) -> Result<(), AudioError> {
        self.inner_mut().start(callback)?;
        if let Device::Mock(mock) = self {
            // Paced in real time, into memory only: nothing reaches a sound card.
            mock.start_realtime();
        }
        Ok(())
    }

    fn stop(&mut self) -> Option<Box<dyn AudioCallback>> {
        if let Device::Mock(mock) = self {
            mock.stop_realtime();
        }
        self.inner_mut().stop()
    }

    fn is_running(&self) -> bool {
        self.inner().is_running()
    }

    fn name(&self) -> String {
        self.inner().name()
    }

    fn sample_rate(&self) -> f64 {
        self.inner().sample_rate()
    }

    fn buffer_size(&self) -> usize {
        self.inner().buffer_size()
    }

    fn latency_monitor(&self) -> &LatencyMonitor {
        self.inner().latency_monitor()
    }
}

/// Locks the shared device; a poisoned lock still yields the device for reading.
fn lock(device: &Mutex<Device>) -> MutexGuard<'_, Device> {
    device.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn main(args: &[String]) -> ExitCode {
    let mock = args.iter().any(|a| a == "--mock-device");
    let smoke = args.iter().any(|a| a == "--smoke");
    let debug = args.iter().any(|a| a == "--debug-panel");

    let (engine, audio) = DjEngine::new(DECKS);
    let engine = Arc::new(engine);
    let router = Arc::new(ParameterRouter::new());
    engine.register(&router);

    let (device, banner) = if mock {
        (mock_device(), Some("Mock device: silent, no audio output (--mock-device).".to_owned()))
    } else {
        match CpalAudioDevice::open(&OutputRequest {
            device: None,
            sample_rate: None,
            buffer_size: None,
            channels: None,
        }) {
            Ok(device) => (Device::Cpal(device), None),
            Err(error) => {
                eprintln!("openmixer: no audio device ({error}); running silent on the mock device");
                (
                    mock_device(),
                    Some("No audio device: running silent on the mock device.".to_owned()),
                )
            }
        }
    };
    let device = Arc::new(Mutex::new(device));
    if let Err(error) = lock(&device).start(Box::new(audio)) {
        eprintln!("openmixer: could not start the audio device: {error}");
        return ExitCode::FAILURE;
    }

    let status_device = Arc::clone(&device);
    let status: DeviceStatusFn = Arc::new(move || device_status(&lock(&status_device)));
    let options = RunOptions {
        smoke_frames: smoke.then_some(SMOKE_FRAMES),
        banner,
        debug: debug.then_some(status),
    };
    let frames = run(Arc::clone(&engine), router, options);
    lock(&device).stop();

    if smoke {
        println!("openmixer gui smoke ok: {frames} frames");
        if frames < SMOKE_FRAMES {
            eprintln!("openmixer: window closed after {frames} of {SMOKE_FRAMES} smoke frames");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn mock_device() -> Device {
    Device::Mock(MockAudioDevice::new(MOCK_RATE, MOCK_BLOCK, 0, 4))
}

/// The debug panel's view of the device, from its latency monitor.
fn device_status(device: &Device) -> DeviceStatus {
    let s: LatencyStats = device.latency_stats();
    DeviceStatus {
        name: device.name(),
        sample_rate: device.sample_rate(),
        buffer_size: device.buffer_size(),
        round_trip_ms: s.round_trip_latency_ms,
        callback_avg_us: s.avg_callback_us,
        callback_max_us: s.max_callback_us,
        budget_us: s.budget_us,
        cpu_load: s.cpu_load,
        xruns: s.xrun_count,
        callbacks: s.callback_count,
    }
}
