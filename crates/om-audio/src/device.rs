//! The device contract: a real-time backend that drives one [`AudioCallback`]. Port of
//! `src/engine/AudioDevice.h`. The latency monitor is shared so the control side can read stats
//! while the device runs.

use om_engine::{AudioCallback, LatencyMonitor, LatencyStats};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AudioError {
    #[error("the device is already running")]
    AlreadyRunning,
    #[error("no audio output device is available")]
    NoDevice,
    #[error("unsupported device configuration: {0}")]
    Unsupported(String),
    #[error("audio stream error: {0}")]
    Stream(String),
}

pub trait AudioDevice: Send {
    /// Prepares `callback` for the device's rate and block size and starts driving it. Fails when
    /// the device is already running.
    fn start(&mut self, callback: Box<dyn AudioCallback>) -> Result<(), AudioError>;

    /// Returns once no callback is in flight, and hands the callback back to the caller so the
    /// same engine state can be started again. `None` when the device was not running. The device
    /// does not call `release` on the returned callback; the owner decides.
    fn stop(&mut self) -> Option<Box<dyn AudioCallback>>;

    fn is_running(&self) -> bool;

    fn name(&self) -> String;

    fn sample_rate(&self) -> f64;

    fn buffer_size(&self) -> usize;

    fn latency_monitor(&self) -> &LatencyMonitor;

    fn latency_stats(&self) -> LatencyStats {
        self.latency_monitor().stats()
    }
}
