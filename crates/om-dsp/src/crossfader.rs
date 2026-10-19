//! Crossfader position, curve and per-side gains.

use std::f32::consts::FRAC_PI_2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossfaderAssign {
    /// Channel ignores the crossfader.
    Thru,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CrossfaderCurve {
    Linear,
    #[default]
    ConstantPower,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrossfaderGains {
    pub left: f32,
    pub right: f32,
}

impl Default for CrossfaderGains {
    fn default() -> Self {
        Self {
            left: 1.0,
            right: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crossfader {
    position: f32,
    curve: CrossfaderCurve,
}

impl Default for Crossfader {
    fn default() -> Self {
        Self {
            position: 0.5,
            curve: CrossfaderCurve::ConstantPower,
        }
    }
}

impl Crossfader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the position, clamped to `0..=1` (0 = left, 1 = right).
    pub fn set_position(&mut self, p: f32) {
        self.position = p.clamp(0.0, 1.0);
    }

    pub fn set_curve(&mut self, curve: CrossfaderCurve) {
        self.curve = curve;
    }

    pub fn position(&self) -> f32 {
        self.position
    }

    pub fn curve(&self) -> CrossfaderCurve {
        self.curve
    }

    pub fn gains(&self) -> CrossfaderGains {
        let p = self.position;
        match self.curve {
            CrossfaderCurve::Linear => CrossfaderGains {
                left: 1.0 - p,
                right: p,
            },
            CrossfaderCurve::ConstantPower => CrossfaderGains {
                left: (p * FRAC_PI_2).cos(),
                right: (p * FRAC_PI_2).sin(),
            },
        }
    }

    /// Gain a channel with assignment `assign` receives from `gains`.
    pub fn gain_for(gains: CrossfaderGains, assign: CrossfaderAssign) -> f32 {
        match assign {
            CrossfaderAssign::Left => gains.left,
            CrossfaderAssign::Right => gains.right,
            CrossfaderAssign::Thru => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Crossfader, CrossfaderAssign, CrossfaderCurve};

    fn approx(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn routes_assigned_channels() {
        for curve in [CrossfaderCurve::Linear, CrossfaderCurve::ConstantPower] {
            let mut xf = Crossfader::new();
            xf.set_curve(curve);

            xf.set_position(0.0);
            let g = xf.gains();
            assert!(
                approx(Crossfader::gain_for(g, CrossfaderAssign::Left), 1.0, 1e-6),
                "{curve:?} only left at 0"
            );
            assert!(approx(
                Crossfader::gain_for(g, CrossfaderAssign::Right),
                0.0,
                1e-6
            ));

            xf.set_position(1.0);
            let g = xf.gains();
            assert!(
                approx(Crossfader::gain_for(g, CrossfaderAssign::Left), 0.0, 1e-6),
                "{curve:?} only right at 1"
            );
            assert!(approx(
                Crossfader::gain_for(g, CrossfaderAssign::Right),
                1.0,
                1e-6
            ));

            xf.set_position(0.5);
            let g = xf.gains();
            let expected = if curve == CrossfaderCurve::Linear {
                0.5
            } else {
                0.5_f32.sqrt()
            };
            assert!(approx(
                Crossfader::gain_for(g, CrossfaderAssign::Left),
                expected,
                1e-6
            ));
            assert!(approx(
                Crossfader::gain_for(g, CrossfaderAssign::Right),
                expected,
                1e-6
            ));
            // Two uncorrelated equal sources: their powers sum.
            let power = 2.0 * expected * expected;
            assert!(approx(
                power,
                if curve == CrossfaderCurve::Linear {
                    0.5
                } else {
                    1.0
                },
                1e-5
            ));
        }
    }

    #[test]
    fn thru_channels_ignore_the_crossfader() {
        let mut xf = Crossfader::new();
        for p in [0.0, 0.5, 1.0] {
            xf.set_position(p);
            assert_eq!(
                Crossfader::gain_for(xf.gains(), CrossfaderAssign::Thru),
                1.0
            );
        }
    }

    #[test]
    fn curve_and_position_getters_follow_setters() {
        let mut xf = Crossfader::new();
        assert_eq!(xf.curve(), CrossfaderCurve::ConstantPower);
        xf.set_curve(CrossfaderCurve::Linear);
        assert_eq!(xf.curve(), CrossfaderCurve::Linear);
        xf.set_curve(CrossfaderCurve::ConstantPower);
        assert_eq!(xf.curve(), CrossfaderCurve::ConstantPower);

        xf.set_position(0.8);
        assert!(approx(xf.position(), 0.8, 1e-6));
        xf.set_position(2.0);
        assert_eq!(xf.position(), 1.0, "position clamps");
        xf.set_position(-1.0);
        assert_eq!(xf.position(), 0.0, "position clamps");

        xf.set_curve(CrossfaderCurve::Linear);
        xf.set_position(0.8);
        let g = xf.gains();
        assert!(approx(g.left, 0.2, 1e-6) && approx(g.right, 0.8, 1e-6));
    }
}
