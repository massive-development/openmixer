//! Wraps the engine's audio callback and counts bad output samples (NaN, Inf, overflow) on the
//! audio thread. Port of `tools/soak/OutputChecker.h`.

use om_audio::AudioCallback;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// |sample| above this counts as overflow: far beyond any legal master level (C++ `kOverflowLimit`).
pub const OVERFLOW_LIMIT: f32 = 4.0;

/// Totals shared between the audio thread (writer) and the control side (reader).
#[derive(Default)]
pub struct OutputStats {
    non_finite: AtomicU64,
    overflow: AtomicU64,
    peak_bits: AtomicU32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutputReport {
    pub non_finite: u64,
    pub overflow: u64,
    pub peak: f32,
}

impl OutputStats {
    pub fn report(&self) -> OutputReport {
        OutputReport {
            non_finite: self.non_finite.load(Ordering::Relaxed),
            overflow: self.overflow.load(Ordering::Relaxed),
            peak: f32::from_bits(self.peak_bits.load(Ordering::Relaxed)),
        }
    }

    fn scan(&self, samples: &[f32]) {
        let mut bad = 0_u64;
        let mut over = 0_u64;
        let mut block_peak = 0.0_f32;
        for &s in samples {
            if !s.is_finite() {
                bad += 1;
                continue;
            }
            let a = s.abs();
            over += u64::from(a > OVERFLOW_LIMIT);
            block_peak = block_peak.max(a);
        }
        self.non_finite.fetch_add(bad, Ordering::Relaxed);
        self.overflow.fetch_add(over, Ordering::Relaxed);
        if block_peak > f32::from_bits(self.peak_bits.load(Ordering::Relaxed)) {
            self.peak_bits
                .store(block_peak.to_bits(), Ordering::Relaxed);
        }
    }
}

/// Audio-thread wrapper: runs `inner`, then scans every output channel it wrote.
pub struct OutputChecker<C: AudioCallback> {
    inner: C,
    stats: Arc<OutputStats>,
}

impl<C: AudioCallback> OutputChecker<C> {
    pub fn new(inner: C) -> (Self, Arc<OutputStats>) {
        let stats = Arc::new(OutputStats::default());
        let checker = Self {
            inner,
            stats: Arc::clone(&stats),
        };
        (checker, stats)
    }
}

impl<C: AudioCallback> AudioCallback for OutputChecker<C> {
    fn prepare(&mut self, sample_rate: f64, max_block_size: usize) {
        self.inner.prepare(sample_rate, max_block_size);
    }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        self.inner.process(inputs, outputs);
        for out in outputs.iter() {
            self.stats.scan(out);
        }
    }

    fn release(&mut self) {
        self.inner.release();
    }
}
