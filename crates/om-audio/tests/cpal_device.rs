//! Device tests for the cpal backend. Opening a real device is opt-in: set
//! `OPENMIXER_DEVICE_TESTS=1` to open the default output, which then runs a callback that writes
//! zeros, so nothing is audible. Without it, or on hosts with no usable device (WSL, CI), the test
//! prints SKIP and passes. Listing devices opens nothing and always runs.

use om_audio::{output_devices, AudioCallback, AudioDevice, CpalAudioDevice, OutputRequest};
use std::time::Duration;

/// Writes digital silence. The device test never renders program audio.
struct Silence;

impl AudioCallback for Silence {
    fn process(&mut self, _: &[&[f32]], outputs: &mut [&mut [f32]]) {
        for out in outputs.iter_mut() {
            out.fill(0.0);
        }
    }
}

#[test]
fn device_list_is_available_or_empty() {
    match output_devices() {
        Ok(devices) => println!("{} output device(s)", devices.len()),
        Err(e) => println!("SKIP: cannot enumerate output devices: {e}"),
    }
}

#[test]
fn device_opens_or_skips() {
    if std::env::var_os("OPENMIXER_DEVICE_TESTS").is_none() {
        println!("SKIP: set OPENMIXER_DEVICE_TESTS=1 to open the default output");
        return;
    }
    let mut device = match CpalAudioDevice::open(&OutputRequest::default()) {
        Ok(device) => device,
        Err(e) => {
            println!("SKIP: no usable default output device: {e}");
            return;
        }
    };
    device
        .start(Box::new(Silence))
        .expect("start on an open device");
    assert!(device.is_running());
    std::thread::sleep(Duration::from_millis(300));
    device.stop();
    assert!(!device.is_running());
    assert!(
        device.latency_stats().callback_count > 0,
        "the backend ran the callback"
    );
}
