//! One-knob DJ filter: -1 low-pass, 0 off, +1 high-pass.

use crate::biquad::{design_high_pass, design_low_pass, Biquad};

const DEAD_ZONE: f32 = 0.02;
// C++ literal, kept verbatim so coefficients stay bit-identical.
#[allow(clippy::approx_constant)]
const Q: f32 = 0.7071;
const MIN_LOW_PASS_HZ: f32 = 40.0;
const MIN_HIGH_PASS_HZ: f32 = 20.0;
const MAX_HZ: f32 = 20_000.0;
const FORCE_REFRESH: f32 = 1000.0;

#[derive(Debug, Clone)]
pub struct SweepFilter {
    position: f32,
    applied: f32,
    active: bool,
    sample_rate: f32,
    filter: Biquad,
}

impl SweepFilter {
    pub fn new() -> Self {
        Self {
            position: 0.0,
            applied: 0.0,
            active: false,
            sample_rate: 44_100.0,
            filter: Biquad::new(),
        }
    }

    /// Sets the sweep position, clamped to `-1..=1`.
    pub fn set_position(&mut self, v: f32) {
        self.position = v.clamp(-1.0, 1.0);
    }

    pub fn position(&self) -> f32 {
        self.position
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate as f32;
        self.applied = FORCE_REFRESH;
        self.filter.reset();
        self.refresh_coefficients();
    }

    /// Filters a stereo block in place. `left` and `right` must have the same length.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        assert_eq!(left.len(), right.len(), "stereo buffers must match");
        self.refresh_coefficients();
        if !self.active {
            return;
        }
        self.filter.process(0, left);
        self.filter.process(1, right);
    }

    fn refresh_coefficients(&mut self) {
        let v = self.position;
        if v == self.applied {
            return;
        }
        self.applied = v;
        self.active = v.abs() > DEAD_ZONE;
        if !self.active {
            return;
        }

        let sr = self.sample_rate;
        let max_hz = MAX_HZ.min(sr * 0.45);
        if v < 0.0 {
            let hz = max_hz * (MIN_LOW_PASS_HZ / max_hz).powf(-v);
            self.filter.set_coeffs(design_low_pass(sr, hz, Q));
        } else {
            let hz = MIN_HIGH_PASS_HZ * (max_hz / MIN_HIGH_PASS_HZ).powf(v);
            self.filter.set_coeffs(design_high_pass(sr, hz, Q));
        }
    }
}

impl Default for SweepFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::SweepFilter;
    use crate::test_util::{db_of, rms_tail, sine, sine_rms, BLOCK, SAMPLE_RATE};

    /// Gain in dB through the sweep filter alone for a sine at `hz`, with `position` applied.
    fn gain_db(hz: f64, position: f32) -> f64 {
        let mut sweep = SweepFilter::new();
        sweep.prepare(SAMPLE_RATE);
        sweep.set_position(position);
        let mut left = sine(hz, 0.5);
        let mut right = left.clone();
        for (l, r) in left.chunks_mut(BLOCK).zip(right.chunks_mut(BLOCK)) {
            sweep.process(l, r);
        }
        db_of(rms_tail(&left) / sine_rms(0.5))
    }

    #[test]
    fn sweep_filter_low_pass_high_pass_and_bypass() {
        assert!(gain_db(10_000.0, -0.8) < -20.0, "LP removes highs");
        assert!((gain_db(40.0, -0.8) - 0.0).abs() < 1.0);
        assert!(gain_db(100.0, 0.8) < -20.0, "HP removes lows");
        assert!((gain_db(10_000.0, 0.8) - 0.0).abs() < 1.0);
        assert!((gain_db(100.0, 0.01) - 0.0).abs() < 0.01);
    }

    #[test]
    fn position_is_clamped_to_the_sweep_range() {
        let mut sweep = SweepFilter::new();
        sweep.set_position(-3.0);
        assert_eq!(sweep.position(), -1.0);
        sweep.set_position(0.25);
        assert_eq!(sweep.position(), 0.25);
    }
}
