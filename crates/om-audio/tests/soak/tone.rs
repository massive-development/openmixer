//! Test-track generation and the soak's random source. Tracks are sine plus light noise, as
//! `tools/soak/ToneWriter.h` writes them, but 32-bit float WAV: the soak feeds them to the engine
//! and never to a device.

use crate::common::write_float_wav;
use std::f64::consts::PI;
use std::path::Path;

/// Deterministic SplitMix64, so a soak run is repeatable (C++ `juce::Random` seeded 1234 plays the
/// same role; the exact sequence differs).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn next_unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1_u64 << 53) as f64
    }

    /// Uniform in [0, n). `n` must be non-zero.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    pub fn coin(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}

/// Writes a stereo tone of `seconds` at `hz` (amplitude 0.4, plus +-0.05 noise per channel).
pub fn write_tone(path: &Path, hz: f64, seconds: u32, rate: u32) {
    let frames = rate as usize * seconds as usize;
    let mut noise = Rng::new(7);
    let mut samples = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let phase = 2.0 * PI * hz * i as f64 / f64::from(rate);
        let tone = 0.4 * phase.sin();
        for _ in 0..2 {
            samples.push((tone + 0.1 * (noise.next_unit() - 0.5)) as f32);
        }
    }
    write_float_wav(path, 2, rate, &samples);
}
