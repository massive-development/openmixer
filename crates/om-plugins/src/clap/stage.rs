//! The engine stage that runs one CLAP effect instance (stereo in, stereo out).
//!
//! Threads: built on the control thread (`new`, `prepare`, drop); `process` runs on the audio
//! thread and only fills pre-allocated buffers and calls the plugin. A plugin that panics or fails
//! in `process` is marked faulted and the stage passes the dry signal through from then on.
//! `catch_unwind` is not a sandbox: a native crash or an exception in the plugin still takes the
//! process down.

use super::buffers::BlockBuffers;
use super::host::{self, Host};
use super::probe;
use super::state::{Fault, StageState};
use super::worker::Worker;
use clack_host::prelude::*;
use om_engine::ProcessStage;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Instant;

pub struct ClapStage {
    worker: Option<Worker>,
    state: Arc<StageState>,
    processor: Option<PluginAudioProcessor<Host>>,
    buffers: Option<BlockBuffers>,
    config: Option<(f64, usize)>,
    steady: u64,
}

impl ClapStage {
    pub(super) fn new(worker: Option<Worker>, state: Arc<StageState>) -> Self {
        Self {
            worker,
            state,
            processor: None,
            buffers: None,
            config: None,
            steady: 0,
        }
    }

    /// Control thread: (re)activates the plugin for this rate and block size.
    fn activate(&mut self, sample_rate: f64, max_block: usize) {
        self.release_processor();
        let Some(worker) = &self.worker else { return };
        let config = PluginAudioConfiguration {
            sample_rate,
            min_frames_count: 1,
            max_frames_count: u32::try_from(max_block).unwrap_or(u32::MAX).max(1),
        };
        match worker.call(move |ctx| ctx.activate(config)) {
            Some(Ok(processor)) => {
                self.processor = Some(processor);
                self.buffers = Some(BlockBuffers::new(max_block.max(1)));
                self.config = Some((sample_rate, max_block));
                self.steady = 0;
                self.state.clear_fault();
                if let Some(frames) = worker.call(probe::read_latency) {
                    self.state.set_latency(frames);
                }
            }
            Some(Err(error)) => {
                tracing::warn!(%error, "CLAP plugin did not activate; stage bypassed");
                self.state.set_fault(Fault::ActivationFailed);
            }
            None => self.state.set_fault(Fault::ControlThreadLost),
        }
    }

    /// Stops processing and deactivates the instance on its control thread.
    fn release_processor(&mut self) {
        let (Some(worker), Some(processor)) = (&self.worker, self.processor.take()) else {
            return;
        };
        self.buffers = None;
        self.config = None;
        if worker.call(move |ctx| ctx.deactivate(processor)).is_none() {
            self.state.set_fault(Fault::ControlThreadLost);
        }
    }

    fn fault(&self, fault: Fault) {
        self.state.set_fault(fault);
    }

    /// Runs one block of at most `max_block` frames. Audio thread.
    fn run_block(&mut self, left: &mut [f32], right: &mut [f32]) -> Result<(), Fault> {
        let (Some(processor), Some(buffers)) = (&mut self.processor, &mut self.buffers) else {
            return Err(Fault::NotActive);
        };
        let frames = left.len();
        buffers.load(left, right);
        let started = processor
            .ensure_processing_started()
            .map_err(|_| Fault::ProcessFailed)?;
        let status = buffers.run(started, self.steady);
        self.steady = self.steady.wrapping_add(frames as u64);
        status.map_err(|_| Fault::ProcessFailed)?;
        buffers.store(left, right);
        Ok(())
    }
}

impl ProcessStage for ClapStage {
    fn prepare(&mut self, sample_rate: f64, max_block_size: usize) {
        if self.config == Some((sample_rate, max_block_size)) && self.processor.is_some() {
            return;
        }
        self.activate(sample_rate, max_block_size);
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        if self.processor.is_none() || self.state.fault().is_some() {
            return;
        }
        host::mark_audio_thread();
        let max_block = self.config.map_or(1, |(_, m)| m.max(1));
        let started = Instant::now();
        for (l, r) in left.chunks_mut(max_block).zip(right.chunks_mut(max_block)) {
            let outcome = catch_unwind(AssertUnwindSafe(|| self.run_block(l, r)));
            match outcome {
                Ok(Ok(())) => {}
                Ok(Err(fault)) => return self.fault(fault),
                Err(_) => return self.fault(Fault::Panicked),
            }
        }
        self.state.record_cost(started.elapsed());
    }
}

impl Drop for ClapStage {
    fn drop(&mut self) {
        self.release_processor();
    }
}
