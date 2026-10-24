//! Rotary knob maths and painting. The maths is plain functions over `f64`/`f32` so it runs
//! headless; painting is a thin layer over it.
//!
//! Angles are degrees clockwise from 12 o'clock. The sweep is 270 degrees (spec: `docs/design/
//! research/knobs.md` section 3), leaving a 90 degree gap at the bottom.

/// Angle of the minimum, degrees clockwise from 12 o'clock (7:30).
pub const SWEEP_START: f64 = -135.0;
/// Angle of the maximum (4:30).
pub const SWEEP_END: f64 = 135.0;
/// Share of the lower half of a bipolar sweep drawn in the danger colour (spec section 3: "the
/// bottom 5% of the EQ sweep"; the plan measures it from the minimum towards the detent).
pub const KILL_FRACTION: f64 = 0.05;

/// The detent, when it lies strictly inside the range and so can split the sweep.
fn usable_detent(min: f64, detent: Option<f64>, max: f64) -> Option<f64> {
    detent.filter(|d| *d > min && *d < max)
}

/// Pointer angle for `value` on a knob over `min..=max`. With a detent inside the range, each side
/// of it takes half the sweep, so the pointer sits at 12 o'clock on the detent; otherwise the map
/// is linear. Values outside the range clamp.
pub fn angle_of(value: f64, min: f64, detent: Option<f64>, max: f64) -> f64 {
    SWEEP_START + (SWEEP_END - SWEEP_START) * sweep_fraction(value, min, detent, max)
}

/// Where `value` sits along the sweep, `0..=1` (0.5 on a detent). Dragging works on this, not on
/// the raw value, so the pointer follows the mouse whatever the range.
pub fn sweep_fraction(value: f64, min: f64, detent: Option<f64>, max: f64) -> f64 {
    if max <= min {
        return 0.0;
    }
    let v = value.clamp(min, max);
    match usable_detent(min, detent, max) {
        Some(d) if v <= d => 0.5 * (v - min) / (d - min),
        Some(d) => 0.5 + 0.5 * (v - d) / (max - d),
        None => (v - min) / (max - min),
    }
}

/// Inverse of [`sweep_fraction`]; fractions outside `0..=1` clamp.
pub fn value_of_fraction(fraction: f64, min: f64, detent: Option<f64>, max: f64) -> f64 {
    let f = fraction.clamp(0.0, 1.0);
    match usable_detent(min, detent, max) {
        Some(d) if f <= 0.5 => min + (d - min) * f / 0.5,
        Some(d) => d + (max - d) * (f - 0.5) / 0.5,
        None => min + (max - min) * f,
    }
}

/// Inverse of [`angle_of`]: the value a pointer at `angle` stands for. Angles clamp to the sweep.
pub fn value_of_angle(angle: f64, min: f64, detent: Option<f64>, max: f64) -> f64 {
    let f = (angle - SWEEP_START) / (SWEEP_END - SWEEP_START);
    value_of_fraction(f, min, detent, max)
}

/// True when `value` is in the kill zone of a knob that has one (the EQ): within
/// [`KILL_FRACTION`] of the way from the minimum to the detent. Knobs without a kill, or without
/// a usable detent, never are.
pub fn in_kill_zone(value: f64, min: f64, detent: Option<f64>, has_kill: bool) -> bool {
    match usable_detent(min, detent, f64::INFINITY) {
        Some(d) if has_kill => value <= min + KILL_FRACTION * (d - min),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EQ: (f64, Option<f64>, f64) = (-48.0, Some(0.0), 6.0);
    const TRIM: (f64, Option<f64>, f64) = (0.0, Some(1.0), 4.0);
    const FILTER: (f64, Option<f64>, f64) = (-1.0, Some(0.0), 1.0);

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn unipolar_ends_hit_sweep_ends() {
        assert!(near(angle_of(0.0, 0.0, None, 1.0), -135.0));
        assert!(near(angle_of(1.0, 0.0, None, 1.0), 135.0));
        assert!(near(angle_of(0.5, 0.0, None, 1.0), 0.0));
    }

    #[test]
    fn eq_detent_sits_at_twelve_oclock() {
        let (min, d, max) = EQ;
        assert!(near(angle_of(0.0, min, d, max), 0.0));
        assert!(near(angle_of(-48.0, min, d, max), -135.0));
        assert!(near(angle_of(6.0, min, d, max), 135.0));
        assert!(near(angle_of(-24.0, min, d, max), -67.5));
        assert!(near(angle_of(3.0, min, d, max), 67.5));
    }

    #[test]
    fn trim_unity_at_twelve() {
        let (min, d, max) = TRIM;
        assert!(near(angle_of(1.0, min, d, max), 0.0));
        assert!(near(angle_of(0.0, min, d, max), -135.0));
        assert!(near(angle_of(4.0, min, d, max), 135.0));
    }

    #[test]
    fn filter_is_symmetric() {
        let (min, d, max) = FILTER;
        assert!(near(angle_of(0.5, min, d, max), 67.5));
        assert!(near(angle_of(-0.5, min, d, max), -67.5));
    }

    #[test]
    fn round_trip() {
        for (min, d, max) in [EQ, TRIM, FILTER] {
            for i in 0..200 {
                let v = min + (max - min) * f64::from(i) / 199.0;
                let back = value_of_angle(angle_of(v, min, d, max), min, d, max);
                assert!((back - v).abs() < 1e-9, "{v} -> {back}");
            }
        }
    }

    #[test]
    fn out_of_range_clamps() {
        let (min, d, max) = EQ;
        assert!(near(angle_of(100.0, min, d, max), 135.0));
        assert!(near(angle_of(-100.0, min, d, max), -135.0));
        assert!(near(value_of_angle(400.0, min, d, max), 6.0));
        assert!(near(value_of_angle(-400.0, min, d, max), -48.0));
    }

    #[test]
    fn degenerate_detent_falls_back_to_linear() {
        // A detent on the minimum (or the maximum, or outside) cannot split the sweep.
        for d in [0.0, 1.0, -3.0, 9.0] {
            assert!(near(angle_of(0.5, 0.0, Some(d), 1.0), 0.0), "detent {d}");
            assert!(near(angle_of(0.0, 0.0, Some(d), 1.0), -135.0));
            assert!(near(angle_of(1.0, 0.0, Some(d), 1.0), 135.0));
        }
        assert!(near(angle_of(0.5, 1.0, None, 1.0), -135.0), "empty range");
    }

    #[test]
    fn kill_zone() {
        let (min, d, max) = EQ;
        assert!(in_kill_zone(-48.0, min, d, true));
        assert!(in_kill_zone(-46.0, min, d, true), "within 5% of 48 = 2.4");
        assert!(!in_kill_zone(-40.0, min, d, true));
        assert!(!in_kill_zone(-48.0, min, d, false), "only knobs with a kill");
        assert!(!in_kill_zone(0.0, 0.0, None, true), "needs a detent");
        let _ = max;
    }
}
