//! The format 1 analysis (three biquad bands, RMS per block), kept only as the cost baseline for
//! the benchmark test: the new analysis must stay within 3 times this.

use crate::AudioClip;
use om_dsp::{design_high_pass, design_low_pass, Biquad, BiquadCoeffs};

// Literal kept verbatim so the crossover coefficients stay bit-identical.
#[allow(clippy::approx_constant)]
const BUTTERWORTH_Q: f32 = 0.7071;

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

fn reduce_block(input: &[f32], bands: &[Vec<f32>; 3]) -> [f32; 5] {
    let (min, max) = input
        .iter()
        .fold((1.0f32, -1.0f32), |(lo, hi), &x| (lo.min(x), hi.max(x)));
    [min, max, rms(&bands[0]), rms(&bands[1]), rms(&bands[2])]
}

/// The old base-level pass over `clip` (128-sample points, 250 Hz and 4 kHz crossovers).
pub fn build_base_level(clip: &AudioClip) -> Vec<[f32; 5]> {
    let block = 128;
    let mut splitter = BandSplitter::new(clip.sample_rate as f32, 250.0, 4000.0);
    let mut bands: [Vec<f32>; 3] = Default::default();
    let mut points = Vec::with_capacity(clip.samples.len() / block + 1);
    for chunk in clip.samples.chunks(block) {
        splitter.process(chunk, &mut bands);
        points.push(reduce_block(chunk, &bands));
    }
    points
}

/// A format 1 payload for `clip`, as the old build stored it (5 bytes per point).
pub fn to_v1_json(clip: &AudioClip) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let signed = |v: f32| ((v.clamp(-1.0, 1.0) + 1.0) * 127.5).round() as u8;
    let level = |v: f32| (v.clamp(0.0, 1.0).sqrt() * 255.0).round() as u8;
    let bytes: Vec<u8> = build_base_level(clip)
        .iter()
        .flat_map(|p| {
            [
                signed(p[0]),
                signed(p[1]),
                level(p[2]),
                level(p[3]),
                level(p[4]),
            ]
        })
        .collect();
    serde_json::json!({
        "version": 1,
        "sampleRate": clip.sample_rate,
        "numSamples": clip.samples.len(),
        "samplesPerPoint": 128,
        "points": STANDARD.encode(bytes),
    })
    .to_string()
}
