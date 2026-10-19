//! Builds [`WaveformData`] from a clip: splits it into three bands with biquad crossovers, then
//! reduces blocks of `samples_per_point` samples to one point. Stateless and thread-safe.
//! [`WaveformAnalyzer`] wraps it for the job service (id "waveform"). Port of
//! `src/analysis/WaveformBuilder.{h,cpp}`.

use crate::{AnalysisContext, Analyzer, AudioClip, WaveformData, WaveformLevel, WaveformPoint};
use om_dsp::{design_high_pass, design_low_pass, Biquad, BiquadCoeffs};

// C++ literal, kept verbatim so the crossover coefficients match the C++ build.
#[allow(clippy::approx_constant)]
const BUTTERWORTH_Q: f32 = 0.7071;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveformOptions {
    /// Finest level: about 2.9 ms at 44.1 kHz.
    pub samples_per_point: usize,
    pub low_crossover_hz: f32,
    pub high_crossover_hz: f32,
}

impl Default for WaveformOptions {
    fn default() -> Self {
        Self {
            samples_per_point: 128,
            low_crossover_hz: 250.0,
            high_crossover_hz: 4000.0,
        }
    }
}

/// Two cascaded biquads per edge (24 dB/oct) keep the bands visually distinct.
#[derive(Debug, Clone, Copy, Default)]
struct Cascade {
    stages: [Biquad; 2],
}

impl Cascade {
    fn design(&mut self, c: BiquadCoeffs) {
        for s in &mut self.stages {
            s.set_coeffs(c);
        }
    }

    fn process(&mut self, samples: &mut [f32]) {
        for s in &mut self.stages {
            s.process(0, samples);
        }
    }
}

struct BandSplitter {
    low_pass: Cascade,
    mid_high: Cascade,
    mid_low: Cascade,
    high_pass: Cascade,
}

impl BandSplitter {
    fn new(rate: f32, low_hz: f32, high_hz: f32) -> Self {
        let mut s = Self {
            low_pass: Cascade::default(),
            mid_high: Cascade::default(),
            mid_low: Cascade::default(),
            high_pass: Cascade::default(),
        };
        s.low_pass
            .design(design_low_pass(rate, low_hz, BUTTERWORTH_Q));
        s.mid_high
            .design(design_high_pass(rate, low_hz, BUTTERWORTH_Q));
        s.mid_low
            .design(design_low_pass(rate, high_hz, BUTTERWORTH_Q));
        s.high_pass
            .design(design_high_pass(rate, high_hz, BUTTERWORTH_Q));
        s
    }

    fn process(&mut self, input: &[f32], bands: &mut [Vec<f32>; 3]) {
        for b in bands.iter_mut() {
            b.clear();
            b.extend_from_slice(input);
        }
        self.low_pass.process(&mut bands[0]);
        self.mid_high.process(&mut bands[1]);
        self.mid_low.process(&mut bands[1]);
        self.high_pass.process(&mut bands[2]);
    }
}

fn rms(v: &[f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let sum: f64 = v.iter().map(|&x| f64::from(x) * f64::from(x)).sum();
    (sum / v.len() as f64).sqrt() as f32
}

fn reduce_block(input: &[f32], bands: &[Vec<f32>; 3]) -> WaveformPoint {
    let (min, max) = input
        .iter()
        .fold((1.0f32, -1.0f32), |(lo, hi), &x| (lo.min(x), hi.max(x)));
    WaveformPoint {
        min,
        max,
        low: rms(&bands[0]),
        mid: rms(&bands[1]),
        high: rms(&bands[2]),
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WaveformBuilder {
    options: WaveformOptions,
}

impl WaveformBuilder {
    pub fn new(options: WaveformOptions) -> Self {
        Self { options }
    }

    /// `None` when cancelled; invalid (empty) data for an empty clip.
    pub fn build(&self, clip: &AudioClip, context: &dyn AnalysisContext) -> Option<WaveformData> {
        let total = clip.samples.len();
        let block = self.options.samples_per_point.max(1);
        let mut splitter = BandSplitter::new(
            clip.sample_rate as f32,
            self.options.low_crossover_hz,
            self.options.high_crossover_hz,
        );
        let mut bands: [Vec<f32>; 3] = Default::default();
        let mut base = WaveformLevel {
            samples_per_point: block,
            points: Vec::with_capacity(total / block + 1),
        };

        for start in (0..total).step_by(block) {
            if (base.points.len() & 1023) == 0 && context.is_cancelled() {
                return None;
            }
            let end = (start + block).min(total);
            let chunk = &clip.samples[start..end];
            splitter.process(chunk, &mut bands);
            base.points.push(reduce_block(chunk, &bands));
            context.set_progress(end as f32 / total as f32);
        }
        Some(WaveformData::new(clip.sample_rate, total as i64, base))
    }
}

/// [`Analyzer`] wrapper for the job service (id "waveform").
#[derive(Debug, Clone, Copy, Default)]
pub struct WaveformAnalyzer {
    builder: WaveformBuilder,
}

impl Analyzer for WaveformAnalyzer {
    fn id(&self) -> &str {
        "waveform"
    }

    fn version(&self) -> i32 {
        1
    }

    fn analyze(&self, clip: &AudioClip, context: &dyn AnalysisContext) -> Option<String> {
        let data = self.builder.build(clip, context)?;
        if context.is_cancelled() {
            return None;
        }
        Some(data.to_json())
    }
}

#[cfg(test)]
mod tests;
