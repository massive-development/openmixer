#![forbid(unsafe_code)]

//! Audio device layer: the [`AudioDevice`] contract, a headless [`MockAudioDevice`] for tests, and
//! the cpal-backed device. Devices drive an [`AudioCallback`] (re-exported from `om-engine`); the
//! callback runs on the device's audio thread and must not allocate, lock or block.

mod cpal_device;
mod device;
mod mock;

pub use cpal_device::{output_devices, CpalAudioDevice, OutputDeviceInfo, OutputRequest};
pub use device::{AudioDevice, AudioError};
pub use mock::MockAudioDevice;
pub use om_engine::{
    AudioCallback, DjEngine, DjEngineAudio, LatencyMonitor, LatencyStats, OutputPairs, OutputRouter,
};
