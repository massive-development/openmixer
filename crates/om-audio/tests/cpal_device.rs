//! Device tests for the cpal backend. Both tests are `#[ignore]`d, so a plain `cargo test` never
//! touches audio hardware. Run them with `cargo test -p om-audio --test cpal_device -- --ignored`.
//! Opening the default output also needs `OPENMIXER_DEVICE_TESTS=1`; it then runs a callback that
//! writes zeros, so nothing is audible. Without the variable, or on hosts with no usable device
//! (WSL, CI), the open test prints SKIP and passes. Listing devices opens nothing.

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
#[ignore = "needs OPENMIXER_DEVICE_TESTS=1 and a real audio device"]
fn device_list_is_available_or_empty() {
    match output_devices() {
        Ok(devices) => {
            for device in &devices {
                assert!(
                    !device.name.trim().is_empty(),
                    "every output device has a non-empty name"
                );
            }
            println!("{} output device(s)", devices.len());
        }
        Err(e) => println!("SKIP: cannot enumerate output devices: {e}"),
    }
}

#[test]
#[ignore = "needs OPENMIXER_DEVICE_TESTS=1 and a real audio device"]
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
