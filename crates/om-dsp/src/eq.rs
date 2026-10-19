//! Three-band DJ EQ: low shelf 250 Hz, mid peak 1 kHz, high shelf 4 kHz; -48 dB acts as a kill.

use crate::biquad::{design_high_shelf, design_low_shelf, design_peaking, Biquad};

const LOW_HZ: f32 = 250.0;
const MID_HZ: f32 = 1000.0;
const HIGH_HZ: f32 = 4000.0;
// Literal kept verbatim so coefficients stay bit-identical.
#[allow(clippy::approx_constant)]
const MID_Q: f32 = 0.7071;
/// Sentinel that never equals a real gain, so the first `prepare` designs every band.
const FORCE_REFRESH: f32 = 1000.0;

#[derive(Debug, Clone)]
pub struct ThreeBandEq {
    gains: [f32; 3],
    applied: [f32; 3],
    bands: [Biquad; 3],
    sample_rate: f32,
}

impl ThreeBandEq {
    pub const MIN_DB: f32 = -48.0;
    pub const MAX_DB: f32 = 6.0;

    pub fn new() -> Self {
        Self {
            gains: [0.0; 3],
            applied: [0.0; 3],
            bands: [Biquad::new(); 3],
            sample_rate: 44_100.0,
        }
    }

    /// Sets the gain of `band` (0 = low, 1 = mid, 2 = high); indices above 2 address the high band.
    /// The value is clamped to `MIN_DB..=MAX_DB`.
    pub fn set_gain_db(&mut self, band: usize, db: f32) {
        self.gains[band.min(2)] = db.clamp(Self::MIN_DB, Self::MAX_DB);
    }

    pub fn gain_db(&self, band: usize) -> f32 {
        self.gains[band.min(2)]
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate as f32;
        self.applied = [FORCE_REFRESH; 3];
        for band in &mut self.bands {
            band.reset();
        }
        self.refresh_coefficients();
    }

    /// Filters a stereo block in place. `left` and `right` must have the same length.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        assert_eq!(left.len(), right.len(), "stereo buffers must match");
        self.refresh_coefficients();
        for band in &mut self.bands {
            band.process(0, left);
            band.process(1, right);
        }
    }

    fn refresh_coefficients(&mut self) {
        let now = self.gains;
        let sr = self.sample_rate;
        if now[0] != self.applied[0] {
            self.bands[0].set_coeffs(design_low_shelf(sr, LOW_HZ, now[0]));
        }
        if now[1] != self.applied[1] {
            self.bands[1].set_coeffs(design_peaking(sr, MID_HZ, now[1], MID_Q));
        }
        if now[2] != self.applied[2] {
            self.bands[2].set_coeffs(design_high_shelf(sr, HIGH_HZ, now[2]));
        }
        self.applied = now;
    }
}

impl Default for ThreeBandEq {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::ThreeBandEq;
    use crate::test_util::{BLOCK, FRAMES, SAMPLE_RATE};
    use om_testing::level::{gain_to_db, rms_tail, sine_rms};
    use om_testing::signals::sine_amp;

    /// Gain in dB through the EQ alone for a sine at `hz`, with `setup` applied first.
    fn gain_db(hz: f64, setup: impl FnOnce(&mut ThreeBandEq)) -> f64 {
        let mut eq = ThreeBandEq::new();
        eq.prepare(SAMPLE_RATE);
        setup(&mut eq);
        let mut left = sine_amp(hz, 0.5, SAMPLE_RATE, FRAMES);
        let mut right = left.clone();
        for (l, r) in left.chunks_mut(BLOCK).zip(right.chunks_mut(BLOCK)) {
            eq.process(l, r);
        }
        gain_to_db(rms_tail(&left) / sine_rms(0.5))
    }

    #[test]
    fn flat_settings_are_unity_gain() {
        for hz in [60.0, 1000.0, 10_000.0] {
            let g = gain_db(hz, |_| {});
            assert!((g - 0.0).abs() < 0.1, "hz={hz} gain={g}");
        }
    }

    #[test]
    fn three_band_eq_cuts_and_boosts_the_intended_band() {
        assert!(
            gain_db(60.0, |e| e.set_gain_db(0, -48.0)) < -30.0,
            "low kill"
        );
        assert!((gain_db(10_000.0, |e| e.set_gain_db(0, -48.0)) - 0.0).abs() < 0.5);
        assert!(
            gain_db(1000.0, |e| e.set_gain_db(1, -48.0)) < -30.0,
            "mid kill"
        );
        assert!((gain_db(60.0, |e| e.set_gain_db(1, -48.0)) - 0.0).abs() < 5.0);
        assert!(
            gain_db(12_000.0, |e| e.set_gain_db(2, -48.0)) < -30.0,
            "high kill"
        );
        assert!((gain_db(100.0, |e| e.set_gain_db(2, -48.0)) - 0.0).abs() < 0.5);
        assert!((gain_db(60.0, |e| e.set_gain_db(0, 6.0)) - 6.0).abs() < 1.0);
        assert!((gain_db(1000.0, |e| e.set_gain_db(1, 6.0)) - 6.0).abs() < 0.3);
        assert!((gain_db(12_000.0, |e| e.set_gain_db(2, 6.0)) - 6.0).abs() < 1.0);
    }

    #[test]
    fn gains_are_clamped_to_the_kill_and_boost_range() {
        let mut eq = ThreeBandEq::new();
        eq.set_gain_db(1, -999.0);
        assert_eq!(eq.gain_db(1), ThreeBandEq::MIN_DB);
        eq.set_gain_db(0, 99.0);
        assert_eq!(eq.gain_db(0), ThreeBandEq::MAX_DB);
        eq.set_gain_db(9, 1.0);
        assert_eq!(
            eq.gain_db(2),
            1.0,
            "out-of-range band addresses the high band"
        );
    }
}
