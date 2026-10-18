//! RBJ-cookbook biquad filters (public formulas), transposed direct form II, two channels.

use std::f32::consts::PI;

// C++ literal, kept verbatim so coefficients stay bit-identical.
#[allow(clippy::approx_constant)]
const SHELF_Q: f32 = 0.707_106_78;

/// Normalised biquad coefficients (a0 divided out).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiquadCoeffs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl Default for BiquadCoeffs {
    /// Identity (pass-through).
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }
}

fn normalised(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> BiquadCoeffs {
    BiquadCoeffs {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    }
}

/// Shared angle terms: `cs = cos(w)`, `al = sin(w) / (2q)` with `w = 2*pi*hz/sr`.
struct Angle {
    cs: f32,
    al: f32,
}

impl Angle {
    fn new(sample_rate: f32, hz: f32, q: f32) -> Self {
        let w = 2.0 * PI * hz / sample_rate;
        Self {
            cs: w.cos(),
            al: w.sin() / (2.0 * q),
        }
    }
}

pub fn design_low_shelf(sample_rate: f32, hz: f32, gain_db: f32) -> BiquadCoeffs {
    let a = 10.0_f32.powf(gain_db / 40.0);
    let w = Angle::new(sample_rate, hz, SHELF_Q);
    let t = 2.0 * a.sqrt() * w.al;
    normalised(
        a * ((a + 1.0) - (a - 1.0) * w.cs + t),
        2.0 * a * ((a - 1.0) - (a + 1.0) * w.cs),
        a * ((a + 1.0) - (a - 1.0) * w.cs - t),
        (a + 1.0) + (a - 1.0) * w.cs + t,
        -2.0 * ((a - 1.0) + (a + 1.0) * w.cs),
        (a + 1.0) + (a - 1.0) * w.cs - t,
    )
}

pub fn design_high_shelf(sample_rate: f32, hz: f32, gain_db: f32) -> BiquadCoeffs {
    let a = 10.0_f32.powf(gain_db / 40.0);
    let w = Angle::new(sample_rate, hz, SHELF_Q);
    let t = 2.0 * a.sqrt() * w.al;
    normalised(
        a * ((a + 1.0) + (a - 1.0) * w.cs + t),
        -2.0 * a * ((a - 1.0) + (a + 1.0) * w.cs),
        a * ((a + 1.0) + (a - 1.0) * w.cs - t),
        (a + 1.0) - (a - 1.0) * w.cs + t,
        2.0 * ((a - 1.0) - (a + 1.0) * w.cs),
        (a + 1.0) - (a - 1.0) * w.cs - t,
    )
}

pub fn design_peaking(sample_rate: f32, hz: f32, gain_db: f32, q: f32) -> BiquadCoeffs {
    let a = 10.0_f32.powf(gain_db / 40.0);
    let w = Angle::new(sample_rate, hz, q);
    normalised(
        1.0 + w.al * a,
        -2.0 * w.cs,
        1.0 - w.al * a,
        1.0 + w.al / a,
        -2.0 * w.cs,
        1.0 - w.al / a,
    )
}

pub fn design_low_pass(sample_rate: f32, hz: f32, q: f32) -> BiquadCoeffs {
    let w = Angle::new(sample_rate, hz, q);
    normalised(
        (1.0 - w.cs) / 2.0,
        1.0 - w.cs,
        (1.0 - w.cs) / 2.0,
        1.0 + w.al,
        -2.0 * w.cs,
        1.0 - w.al,
    )
}

pub fn design_high_pass(sample_rate: f32, hz: f32, q: f32) -> BiquadCoeffs {
    let w = Angle::new(sample_rate, hz, q);
    normalised(
        (1.0 + w.cs) / 2.0,
        -(1.0 + w.cs),
        (1.0 + w.cs) / 2.0,
        1.0 + w.al,
        -2.0 * w.cs,
        1.0 - w.al,
    )
}

/// Stereo biquad section with per-channel state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Biquad {
    k: BiquadCoeffs,
    z1: [f32; 2],
    z2: [f32; 2],
}

impl Default for Biquad {
    fn default() -> Self {
        Self {
            k: BiquadCoeffs::default(),
            z1: [0.0; 2],
            z2: [0.0; 2],
        }
    }
}

impl Biquad {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_coeffs(&mut self, coeffs: BiquadCoeffs) {
        self.k = coeffs;
    }

    pub fn reset(&mut self) {
        self.z1 = [0.0; 2];
        self.z2 = [0.0; 2];
    }

    /// Filters `samples` in place for `channel` (0 = left, 1 = right).
    pub fn process(&mut self, channel: usize, samples: &mut [f32]) {
        let k = self.k;
        let (mut s1, mut s2) = (self.z1[channel], self.z2[channel]);
        for x in samples.iter_mut() {
            let input = *x;
            let out = k.b0 * input + s1;
            s1 = k.b1 * input - k.a1 * out + s2;
            s2 = k.b2 * input - k.a2 * out;
            *x = out;
        }
        self.z1[channel] = s1;
        self.z2[channel] = s2;
    }

    /// Per-sample form for feedback loops (e.g. FX delay lines).
    pub fn process_sample(&mut self, channel: usize, input: f32) -> f32 {
        let k = self.k;
        let out = k.b0 * input + self.z1[channel];
        self.z1[channel] = k.b1 * input - k.a1 * out + self.z2[channel];
        self.z2[channel] = k.b2 * input - k.a2 * out;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::approx_constant)] // C++ Q literal
    fn low_pass_has_unity_dc_gain() {
        let c = design_low_pass(48_000.0, 1_000.0, 0.7071);
        let dc = (c.b0 + c.b1 + c.b2) / (1.0 + c.a1 + c.a2);
        assert!((dc - 1.0).abs() < 1e-5, "dc gain {dc}");
    }

    #[test]
    #[allow(clippy::approx_constant)] // C++ Q literal
    fn reset_clears_state_so_identical_input_gives_identical_output() {
        let mut filter = Biquad::new();
        filter.set_coeffs(design_high_pass(48_000.0, 500.0, 0.7071));
        let mut first = [0.3_f32, -0.2, 0.1, 0.4];
        let mut second = first;
        filter.process(0, &mut first);
        filter.reset();
        filter.process(0, &mut second);
        assert_eq!(first, second);
    }
}
