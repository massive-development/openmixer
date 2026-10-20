//! The control clock. A thread calls [`DjEngine::update_control`] about every 33 ms (30 Hz), so
//! sync phase and rate, FX tempo and slip keep running when nothing draws: a minimised or occluded
//! window, or a headless run. The UI only reads the results. The same thread installs finished sample
//! decodes (see [`SampleBank::pump`]) once a bank is attached, and steps automix (see
//! [`AutomixRunner`]; nothing runs while it is off and idle). Stops and joins when dropped.

use crate::automix_service::{AutomixRunner, AutomixShared};
use om_core::CommandSink;
use om_engine::DjEngine;
use om_ui::sample_bank::SampleBank;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Time between control steps: 33 ms, about 30 Hz (the engine's control rate is 20-50 Hz).
pub const CONTROL_PERIOD: Duration = Duration::from_millis(33);

pub struct ControlTimer {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl ControlTimer {
    /// Starts the control thread for `engine` at [`CONTROL_PERIOD`].
    pub fn start(
        engine: Arc<DjEngine>,
        samples: Arc<OnceLock<Arc<SampleBank>>>,
        automix: Arc<AutomixShared>,
        sink: Arc<dyn CommandSink>,
    ) -> io::Result<Self> {
        Self::start_with_period(engine, samples, automix, sink, CONTROL_PERIOD)
    }

    fn start_with_period(
        engine: Arc<DjEngine>,
        samples: Arc<OnceLock<Arc<SampleBank>>>,
        automix: Arc<AutomixShared>,
        sink: Arc<dyn CommandSink>,
        period: Duration,
    ) -> io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = std::thread::Builder::new()
            .name("openmixer-control".to_owned())
            .spawn(move || {
                let started = Instant::now();
                let mut last = started;
                let mut automix = AutomixRunner::new(automix);
                while !flag.load(Ordering::Acquire) {
                    std::thread::sleep(period);
                    let now = Instant::now();
                    engine.update_control(now.duration_since(last).as_secs_f64());
                    if let Some(bank) = samples.get() {
                        bank.pump();
                    }
                    automix.step(
                        &engine,
                        sink.as_ref(),
                        now.duration_since(started).as_secs_f64(),
                    );
                    last = now;
                }
            })?;
        Ok(Self {
            stop,
            handle: Some(handle),
        })
    }
}

impl Drop for ControlTimer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
