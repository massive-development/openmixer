//! Port of tests/unit/AudioDeviceTest.cpp: mock device lifecycle, capture, latency instrumentation
//! and the paced realtime thread. Nothing here opens a real device.

mod common;

use om_audio::{AudioCallback, AudioDevice, AudioError, LatencyMonitor, MockAudioDevice};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::sync::Arc;
use std::time::Duration;

#[derive(Default)]
struct Probe {
    prepared: AtomicUsize,
    released: AtomicUsize,
    calls: AtomicUsize,
    sample_rate_bits: AtomicU64,
    block: AtomicUsize,
}

/// Copies input to output with gain, counting calls and lifecycle events.
struct GainCallback {
    probe: Arc<Probe>,
    gain: f32,
    sleep: Duration,
}

impl GainCallback {
    fn new(probe: &Arc<Probe>) -> Self {
        Self {
            probe: Arc::clone(probe),
            gain: 0.5,
            sleep: Duration::ZERO,
        }
    }
}

impl AudioCallback for GainCallback {
    fn prepare(&mut self, sample_rate: f64, block: usize) {
        self.probe
            .sample_rate_bits
            .store(sample_rate.to_bits(), Relaxed);
        self.probe.block.store(block, Relaxed);
        self.probe.prepared.fetch_add(1, Relaxed);
    }

    fn release(&mut self) {
        self.probe.released.fetch_add(1, Relaxed);
    }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if !self.sleep.is_zero() {
            std::thread::sleep(self.sleep);
        }
        for (c, out) in outputs.iter_mut().enumerate() {
            for (i, sample) in out.iter_mut().enumerate() {
                *sample = match inputs.get(c) {
                    Some(input) => input[i] * self.gain,
                    None => 0.0,
                };
            }
        }
        self.probe.calls.fetch_add(1, Relaxed);
    }
}

fn wait_for_calls(probe: &Probe, count: usize, timeout: Duration) -> bool {
    common::wait_until(|| probe.calls.load(Relaxed) >= count, timeout)
}

#[test]
fn mock_device_lifecycle_and_block_rendering() {
    let mut dev = MockAudioDevice::new(44_100.0, 128, 2, 2);
    let probe = Arc::new(Probe::default());
    assert!(!dev.is_running());
    assert!(!dev.render_block(), "cannot render before start");
    dev.start(Box::new(GainCallback::new(&probe)))
        .expect("start");
    assert_eq!(
        dev.start(Box::new(GainCallback::new(&probe))),
        Err(AudioError::AlreadyRunning),
        "double start rejected"
    );
    assert_eq!(probe.prepared.load(Relaxed), 1);
    assert_eq!(
        f64::from_bits(probe.sample_rate_bits.load(Relaxed)),
        44_100.0
    );
    assert_eq!(probe.block.load(Relaxed), 128);

    for i in 0..128 {
        dev.set_input_sample(0, i, 1.0);
        dev.set_input_sample(1, i, -1.0);
    }

    assert_eq!(dev.render_blocks(4), 4);
    assert_eq!(probe.calls.load(Relaxed), 4);
    let out = dev.last_output();
    assert!((out[0][10] - 0.5).abs() < 1e-6);
    assert!((out[1][10] + 0.5).abs() < 1e-6);

    let returned = dev.stop();
    assert!(returned.is_some(), "stop hands the callback back");
    assert_eq!(probe.released.load(Relaxed), 0, "stop does not release it");
    assert!(!dev.is_running());
    assert!(!dev.render_block());
}

#[test]
fn capture_records_output_across_blocks() {
    let mut dev = MockAudioDevice::new(48_000.0, 64, 1, 1);
    let probe = Arc::new(Probe::default());
    let mut cb = GainCallback::new(&probe);
    cb.gain = 1.0;
    dev.start(Box::new(cb)).expect("start");
    dev.start_capture(200);
    for i in 0..64 {
        dev.set_input_sample(0, i, 0.25);
    }
    dev.render_blocks(10); // 640 frames offered, capture holds 200
    assert_eq!(dev.captured_frames(), 200);
    assert!((dev.capture()[0][199] - 0.25).abs() < 1e-6);
}

#[test]
fn latency_instrumentation_reports_device_latency_and_timing() {
    let mut dev = MockAudioDevice::new(48_000.0, 480, 2, 2);
    dev.set_simulated_latency(96, 240);
    let probe = Arc::new(Probe::default());
    dev.start(Box::new(GainCallback::new(&probe)))
        .expect("start");
    dev.render_blocks(20);
    let s = dev.latency_stats();
    assert_eq!(s.callback_count, 20);
    assert_eq!(s.input_latency_samples, 96);
    assert_eq!(s.output_latency_samples, 240);
    assert!((s.input_latency_ms - 2.0).abs() < 1e-9);
    assert!((s.output_latency_ms - 5.0).abs() < 1e-9);
    assert!((s.round_trip_latency_ms - 1000.0 * (480 + 96 + 240) as f64 / 48_000.0).abs() < 1e-9);
    assert!((s.budget_us - 10_000.0).abs() < 1e-6);
    assert!(s.max_callback_us >= s.avg_callback_us);
    assert!(s.avg_callback_us >= 0.0);
    assert_eq!(s.xrun_count, 0);
}

#[test]
fn slow_callbacks_are_counted_as_xruns() {
    let mut dev = MockAudioDevice::new(48_000.0, 48, 2, 2); // 1 ms budget
    let probe = Arc::new(Probe::default());
    let mut cb = GainCallback::new(&probe);
    cb.sleep = Duration::from_millis(5);
    dev.start(Box::new(cb)).expect("start");
    dev.render_blocks(3);
    let s = dev.latency_stats();
    assert_eq!(s.xrun_count, 3);
    assert!(
        s.max_callback_us >= 4000.0,
        "max callback time reflects the sleep"
    );
    assert!(s.cpu_load > 1.0, "load above 100% of budget");
}

#[test]
fn latency_monitor_records_synthetic_timings() {
    let m = LatencyMonitor::default();
    m.configure(48_000.0, 480);
    m.record_callback(2000.0, 480);
    m.record_callback(4000.0, 480);
    m.record_callback(12_000.0, 480); // exceeds 10 ms budget
    let s = m.stats();
    assert_eq!(s.callback_count, 3);
    assert_eq!(s.xrun_count, 1);
    assert!((s.avg_callback_us - 6000.0).abs() < 1e-6);
    assert!((s.max_callback_us - 12_000.0).abs() < 1e-6);
    assert!((s.last_callback_us - 12_000.0).abs() < 1e-6);
    m.reset();
    assert_eq!(m.stats().callback_count, 0);
}

#[test]
fn paced_realtime_thread_drives_the_callback() {
    let mut dev = MockAudioDevice::new(48_000.0, 480, 2, 2); // 10 ms blocks
    let probe = Arc::new(Probe::default());
    dev.start(Box::new(GainCallback::new(&probe)))
        .expect("start");
    dev.start_realtime();
    // No upper bound: slow CI overshoots and the pacer catches up.
    assert!(
        wait_for_calls(&probe, 3, Duration::from_secs(5)),
        "paced thread should run blocks"
    );
    dev.stop();
    let calls = probe.calls.load(Relaxed);
    assert!(calls >= 3);
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(probe.calls.load(Relaxed), calls, "no callbacks after stop");
}
