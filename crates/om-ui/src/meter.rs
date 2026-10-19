//! Meter view state: ballistics per side, the clip latch, and the theme-driven parameters. The
//! ballistics maths lives in `om_dsp::MeterBallistics`; this is the stereo wrapper the mixer and
//! master sections use. Also provides the `MeterBar` / `StereoMeter` wrappers.

use crate::theme::ThemeTokens;
pub use om_dsp::MeterParams;

use om_dsp::MeterBallistics;

/// Ballistics parameters from the current theme tokens.
pub fn meter_params(t: &ThemeTokens) -> MeterParams {
    MeterParams {
        floor_db: t.meter_floor_db,
        clip_level: t.meter_clip_level,
        release_seconds: t.meter_release_seconds,
        hold_seconds: t.meter_hold_seconds,
        clip_seconds: t.meter_clip_seconds,
    }
}

/// One meter column: feeds linear peaks, reads level, hold tick and clip lamp.
#[derive(Debug, Clone)]
pub struct MeterBar {
    ballistics: MeterBallistics,
}

impl MeterBar {
    pub fn new(params: MeterParams) -> Self {
        Self {
            ballistics: MeterBallistics::new(params),
        }
    }

    /// One UI frame: `peak` is the linear peak since the last frame, `dt` the frame time in seconds.
    /// Returns true when anything visible changed.
    pub fn update(&mut self, peak: f32, dt: f32) -> bool {
        self.ballistics.update(peak, dt)
    }

    /// A click on the meter clears the clip lamp.
    pub fn click(&mut self) {
        self.ballistics.reset_clip();
    }

    pub fn ballistics(&self) -> &MeterBallistics {
        &self.ballistics
    }
}

/// A stereo meter: one [`MeterBar`] per side, each fed its own peak.
#[derive(Debug, Clone)]
pub struct StereoMeter {
    pub left: MeterBar,
    pub right: MeterBar,
}

impl StereoMeter {
    pub fn new(params: MeterParams) -> Self {
        Self {
            left: MeterBar::new(params),
            right: MeterBar::new(params),
        }
    }

    /// Feeds both sides for one frame.
    pub fn update(&mut self, left_peak: f32, right_peak: f32, dt: f32) {
        self.left.update(left_peak, dt);
        self.right.update(right_peak, dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 30.0;

    fn params() -> MeterParams {
        MeterParams {
            floor_db: -48.0,
            clip_level: 0.99,
            release_seconds: 0.5,
            hold_seconds: 1.0,
            clip_seconds: 2.0,
        }
    }

    #[test]
    fn defaults_come_from_the_theme() {
        let p = meter_params(&ThemeTokens::builtin_dark());
        assert!(p.floor_db < 0.0);
        assert!(p.release_seconds > 0.0);
        assert!(p.clip_level > 0.5);
    }

    #[test]
    fn meter_bar_follows_updates_and_clears_clip_on_click() {
        let mut bar = MeterBar::new(params());
        bar.update(1.0, FRAME);
        assert!(bar.ballistics().is_clipping());
        assert!(bar.ballistics().level() > 0.9);
        bar.click();
        assert!(!bar.ballistics().is_clipping());
    }

    #[test]
    fn stereo_meter_feeds_each_side_separately() {
        let mut meter = StereoMeter::new(params());
        meter.update(0.5, 0.0, FRAME);
        assert!(meter.left.ballistics().level() > 0.5);
        assert_eq!(meter.right.ballistics().level(), 0.0);
    }
}
