//! Callback timing and device latency statistics. One writer (the audio thread), any readers.

use om_core::atomic::AtomicF64;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LatencyStats {
    pub sample_rate: f64,
    pub buffer_size: usize,
    pub input_latency_samples: usize,
    pub output_latency_samples: usize,
    pub input_latency_ms: f64,
    pub output_latency_ms: f64,
    pub round_trip_latency_ms: f64,
    pub callback_count: u64,
    pub last_callback_us: f64,
    pub avg_callback_us: f64,
    pub max_callback_us: f64,
    pub budget_us: f64,
    /// Average callback time over the budget.
    pub cpu_load: f64,
    /// Callbacks that ran over budget.
    pub xrun_count: u64,
}

#[derive(Debug, Default)]
pub struct LatencyMonitor {
    sample_rate: AtomicF64,
    last_us: AtomicF64,
    total_us: AtomicF64,
    max_us: AtomicF64,
    budget_us: AtomicF64,
    buffer_size: AtomicUsize,
    in_latency: AtomicUsize,
    out_latency: AtomicUsize,
    count: AtomicU64,
    xruns: AtomicU64,
}

impl LatencyMonitor {
    pub fn configure(&self, sample_rate: f64, buffer_size: usize) {
        self.sample_rate.store(sample_rate);
        self.buffer_size.store(buffer_size, Ordering::Relaxed);
        let budget = if sample_rate > 0.0 {
            1.0e6 * buffer_size as f64 / sample_rate
        } else {
            0.0
        };
        self.budget_us.store(budget);
    }

    pub fn set_device_latency(&self, input_samples: usize, output_samples: usize) {
        self.in_latency.store(input_samples, Ordering::Relaxed);
        self.out_latency.store(output_samples, Ordering::Relaxed);
    }

    pub fn reset(&self) {
        self.last_us.store(0.0);
        self.total_us.store(0.0);
        self.max_us.store(0.0);
        self.count.store(0, Ordering::Relaxed);
        self.xruns.store(0, Ordering::Relaxed);
    }

    /// Audio thread. Called once per callback with its duration and frame count.
    pub fn record_callback(&self, duration_us: f64, num_samples: usize) {
        self.last_us.store(duration_us);
        self.total_us.store(self.total_us.load() + duration_us);
        if duration_us > self.max_us.load() {
            self.max_us.store(duration_us);
        }
        let sample_rate = self.sample_rate.load();
        let budget = if sample_rate > 0.0 {
            1.0e6 * num_samples as f64 / sample_rate
        } else {
            self.budget_us.load()
        };
        if budget > 0.0 && duration_us > budget {
            self.xruns.fetch_add(1, Ordering::Relaxed);
        }
        self.count.fetch_add(1, Ordering::Release);
    }

    pub fn stats(&self) -> LatencyStats {
        let callback_count = self.count.load(Ordering::Acquire);
        let sample_rate = self.sample_rate.load();
        let buffer_size = self.buffer_size.load(Ordering::Relaxed);
        let input_latency_samples = self.in_latency.load(Ordering::Relaxed);
        let output_latency_samples = self.out_latency.load(Ordering::Relaxed);
        let (input_latency_ms, output_latency_ms, round_trip_latency_ms) = if sample_rate > 0.0 {
            (
                1000.0 * input_latency_samples as f64 / sample_rate,
                1000.0 * output_latency_samples as f64 / sample_rate,
                1000.0 * (buffer_size + input_latency_samples + output_latency_samples) as f64
                    / sample_rate,
            )
        } else {
            (0.0, 0.0, 0.0)
        };
        let total_us = self.total_us.load();
        let budget_us = self.budget_us.load();
        let avg_callback_us = if callback_count > 0 {
            total_us / callback_count as f64
        } else {
            0.0
        };
        LatencyStats {
            sample_rate,
            buffer_size,
            input_latency_samples,
            output_latency_samples,
            input_latency_ms,
            output_latency_ms,
            round_trip_latency_ms,
            callback_count,
            last_callback_us: self.last_us.load(),
            avg_callback_us,
            max_callback_us: self.max_us.load(),
            budget_us,
            cpu_load: if budget_us > 0.0 {
                avg_callback_us / budget_us
            } else {
                0.0
            },
            xrun_count: self.xruns.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_latency_and_xruns_follow_the_callback_timing() {
        let monitor = LatencyMonitor::default();
        monitor.configure(48_000.0, 480);
        monitor.set_device_latency(48, 96);
        assert_eq!(monitor.stats().budget_us, 10_000.0);
        monitor.record_callback(4_000.0, 480);
        monitor.record_callback(12_000.0, 480);
        let s = monitor.stats();
        assert_eq!(s.callback_count, 2);
        assert_eq!(s.xrun_count, 1);
        assert_eq!(s.max_callback_us, 12_000.0);
        assert_eq!(s.avg_callback_us, 8_000.0);
        assert!((s.cpu_load - 0.8).abs() < 1e-12);
        assert!(
            (s.round_trip_latency_ms - 1000.0 * (480 + 48 + 96) as f64 / 48_000.0).abs() < 1e-9
        );
        monitor.reset();
        assert_eq!(monitor.stats().callback_count, 0);
    }
}
