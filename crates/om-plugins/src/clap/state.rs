//! What the control side can read about a running stage: whether it faulted, its latency and
//! the cost of its `process` calls. Atomics only, written from the audio thread.

use super::gui::PluginGui;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

/// Why a stage is bypassed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// The plugin panicked inside `process` (a Rust panic only; native crashes are not caught).
    Panicked,
    /// `process` returned an error status.
    ProcessFailed,
    /// The plugin refused to activate.
    ActivationFailed,
    /// The plugin's control thread ended (it panicked there).
    ControlThreadLost,
    /// `process` ran before the stage was activated.
    NotActive,
}

impl Fault {
    fn code(self) -> u8 {
        match self {
            Fault::Panicked => 1,
            Fault::ProcessFailed => 2,
            Fault::ActivationFailed => 3,
            Fault::ControlThreadLost => 4,
            Fault::NotActive => 5,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            1 => Fault::Panicked,
            2 => Fault::ProcessFailed,
            3 => Fault::ActivationFailed,
            4 => Fault::ControlThreadLost,
            5 => Fault::NotActive,
            _ => return None,
        })
    }
}

/// Shared between a stage and the control side.
#[derive(Debug, Default)]
pub struct StageState {
    fault: AtomicU8,
    latency: AtomicU32,
    last_ns: AtomicU64,
    max_ns: AtomicU64,
    gui: OnceLock<PluginGui>,
}

impl StageState {
    /// The fault that bypassed the stage, if any.
    pub fn fault(&self) -> Option<Fault> {
        Fault::from_code(self.fault.load(Ordering::Acquire))
    }

    /// Plugin latency in frames, read at activation.
    pub fn latency_frames(&self) -> u32 {
        self.latency.load(Ordering::Relaxed)
    }

    /// Wall time of the last `process` call (all chunks of one engine block).
    pub fn last_process_time(&self) -> Duration {
        Duration::from_nanos(self.last_ns.load(Ordering::Relaxed))
    }

    /// The longest `process` call since the stage was created.
    pub fn max_process_time(&self) -> Duration {
        Duration::from_nanos(self.max_ns.load(Ordering::Relaxed))
    }

    /// The handle to this instance's own window, when the instance was created.
    pub fn gui(&self) -> Option<PluginGui> {
        self.gui.get().cloned()
    }

    pub(super) fn set_gui(&self, gui: PluginGui) {
        let _ = self.gui.set(gui);
    }

    pub(super) fn set_fault(&self, fault: Fault) {
        self.fault.store(fault.code(), Ordering::Release);
    }

    pub(super) fn clear_fault(&self) {
        self.fault.store(0, Ordering::Release);
    }

    pub(super) fn set_latency(&self, frames: u32) {
        self.latency.store(frames, Ordering::Relaxed);
    }

    pub(super) fn record_cost(&self, elapsed: Duration) {
        let ns = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
        self.last_ns.store(ns, Ordering::Relaxed);
        self.max_ns.fetch_max(ns, Ordering::Relaxed);
    }
}
