//! Headless device for tests. Driven by [`MockAudioDevice::render_block`] on the caller's thread, or
//! by a paced thread after [`MockAudioDevice::start_realtime`]. It renders into memory only and
//! never opens a device, so nothing is audible.
//!
//! The render state sits behind a mutex. That is fine for a test double; the real device path
//! (`CpalAudioDevice`) takes no lock on its audio thread.

use crate::device::{AudioDevice, AudioError};
use om_core::sync::LockExt;
use om_engine::{AudioCallback, LatencyMonitor};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Render {
    callback: Option<Box<dyn AudioCallback>>,
    inputs: Vec<Vec<f32>>,
    outputs: Vec<Vec<f32>>,
    capture: Vec<Vec<f32>>,
    capture_limit: usize,
    captured: usize,
}

struct Shared {
    render: Mutex<Render>,
    monitor: LatencyMonitor,
    buffer_size: usize,
    sample_rate: f64,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Render> {
        self.render.lock_unpoisoned()
    }

    /// Runs one callback block. False when no callback is attached.
    fn render_block(&self) -> bool {
        let mut st = self.lock();
        let st = &mut *st;
        let Some(callback) = st.callback.as_mut() else {
            return false;
        };
        let inputs: Vec<&[f32]> = st.inputs.iter().map(Vec::as_slice).collect();
        let mut outputs: Vec<&mut [f32]> = st.outputs.iter_mut().map(Vec::as_mut_slice).collect();
        let started = Instant::now();
        callback.process(&inputs, &mut outputs);
        let elapsed_us = started.elapsed().as_secs_f64() * 1.0e6;
        self.monitor.record_callback(elapsed_us, self.buffer_size);

        if st.capture_limit > 0 {
            let n = self.buffer_size.min(st.capture_limit - st.captured);
            for (dst, src) in st.capture.iter_mut().zip(st.outputs.iter()) {
                dst[st.captured..st.captured + n].copy_from_slice(&src[..n]);
            }
            st.captured += n;
        }
        true
    }
}

struct Realtime {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

pub struct MockAudioDevice {
    shared: Arc<Shared>,
    running: bool,
    realtime: Option<Realtime>,
}

impl MockAudioDevice {
    pub fn new(
        sample_rate: f64,
        buffer_size: usize,
        num_inputs: usize,
        num_outputs: usize,
    ) -> Self {
        let monitor = LatencyMonitor::default();
        monitor.configure(sample_rate, buffer_size);
        let shared = Shared {
            render: Mutex::new(Render {
                callback: None,
                inputs: vec![vec![0.0; buffer_size]; num_inputs],
                outputs: vec![vec![0.0; buffer_size]; num_outputs],
                capture: Vec::new(),
                capture_limit: 0,
                captured: 0,
            }),
            monitor,
            buffer_size,
            sample_rate,
        };
        Self {
            shared: Arc::new(shared),
            running: false,
            realtime: None,
        }
    }

    pub fn set_simulated_latency(&self, input_samples: usize, output_samples: usize) {
        self.shared
            .monitor
            .set_device_latency(input_samples, output_samples);
    }

    /// Sets one input sample. Out-of-range indices are ignored.
    pub fn set_input_sample(&self, channel: usize, frame: usize, value: f32) {
        let mut st = self.shared.lock();
        if let Some(sample) = st
            .inputs
            .get_mut(channel)
            .and_then(|buf| buf.get_mut(frame))
        {
            *sample = value;
        }
    }

    /// Copy of the most recent output block, one vector per channel.
    pub fn last_output(&self) -> Vec<Vec<f32>> {
        self.shared.lock().outputs.clone()
    }

    /// Clears the capture and records up to `max_frames` frames of output.
    pub fn start_capture(&self, max_frames: usize) {
        let mut st = self.shared.lock();
        let channels = st.outputs.len();
        st.capture = vec![vec![0.0; max_frames]; channels];
        st.capture_limit = max_frames;
        st.captured = 0;
    }

    pub fn captured_frames(&self) -> usize {
        self.shared.lock().captured
    }

    /// Copy of the capture buffer, one vector per channel (only the first `captured_frames` are
    /// meaningful).
    pub fn capture(&self) -> Vec<Vec<f32>> {
        self.shared.lock().capture.clone()
    }

    /// Renders one block. False before `start` and after `stop`.
    pub fn render_block(&self) -> bool {
        self.shared.render_block()
    }

    /// Renders up to `blocks` blocks; returns how many ran.
    pub fn render_blocks(&self, blocks: usize) -> usize {
        (0..blocks).take_while(|_| self.render_block()).count()
    }

    /// Starts a thread that renders blocks at the device's pace until [`stop_realtime`] or `stop`.
    /// Does nothing unless the device is running and no paced thread exists yet.
    ///
    /// [`stop_realtime`]: MockAudioDevice::stop_realtime
    pub fn start_realtime(&mut self) {
        if self.realtime.is_some() || !self.running {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::clone(&self.shared);
        let stop_flag = Arc::clone(&stop);
        let handle = std::thread::Builder::new()
            .name("MockAudioDevice".into())
            .spawn(move || {
                let block = Duration::from_secs_f64(shared.buffer_size as f64 / shared.sample_rate);
                let mut next = Instant::now();
                while !stop_flag.load(Ordering::Acquire) {
                    shared.render_block();
                    next += block;
                    let now = Instant::now();
                    if next > now {
                        std::thread::sleep(next - now);
                    }
                }
            })
            .expect("spawn paced mock thread");
        self.realtime = Some(Realtime { stop, handle });
    }

    pub fn stop_realtime(&mut self) {
        if let Some(realtime) = self.realtime.take() {
            realtime.stop.store(true, Ordering::Release);
            let _ = realtime.handle.join();
        }
    }
}

impl AudioDevice for MockAudioDevice {
    fn start(&mut self, mut callback: Box<dyn AudioCallback>) -> Result<(), AudioError> {
        if self.running {
            return Err(AudioError::AlreadyRunning);
        }
        self.shared.monitor.reset();
        callback.prepare(self.shared.sample_rate, self.shared.buffer_size);
        self.shared.lock().callback = Some(callback);
        self.running = true;
        Ok(())
    }

    fn stop(&mut self) -> Option<Box<dyn AudioCallback>> {
        self.stop_realtime();
        if !std::mem::take(&mut self.running) {
            return None;
        }
        self.shared.lock().callback.take()
    }

    fn is_running(&self) -> bool {
        self.running
    }

    fn name(&self) -> String {
        "Mock Audio Device".to_owned()
    }

    fn sample_rate(&self) -> f64 {
        self.shared.sample_rate
    }

    fn buffer_size(&self) -> usize {
        self.shared.buffer_size
    }

    fn latency_monitor(&self) -> &LatencyMonitor {
        &self.shared.monitor
    }
}

impl Drop for MockAudioDevice {
    fn drop(&mut self) {
        // The device is going away, so nothing will start the callback again: release it.
        if let Some(mut callback) = self.stop() {
            callback.release();
        }
    }
}
