//! Meter view state: ballistics per side, the clip latch, and the theme-driven parameters. The
//! ballistics maths lives in `om_dsp::MeterBallistics`; this is the stereo wrapper the mixer and
//! master sections use. Also provides the `MeterBar` / `StereoMeter` wrappers.

use crate::geometry::Rect;
use crate::theme::ThemeTokens;
pub use om_dsp::MeterParams;

use om_dsp::MeterBallistics;

/// Ballistics parameters from the current theme tokens.
pub fn meter_params(t: &ThemeTokens) -> MeterParams {
    MeterParams {
        floor_db: t.meter_floor_db,
        clip_level: t.meter_clip_level,
        release_db_per_second: MeterParams::default().release_db_per_second,
        hold_seconds: t.meter_hold_seconds,
        clip_seconds: t.meter_clip_seconds,
    }
}

/// The dB marks printed beside a meter, loudest first. Marks below the theme's floor are dropped
/// (see [`tick_marks`]).
pub const DB_TICKS: [f32; 5] = [0.0, -6.0, -12.0, -24.0, -48.0];
/// Narrowest a single meter bar's well is drawn, in px: 12 px of fill inside a 1 px border.
pub const MIN_BAR_WIDTH: i32 = 14;
/// Gap between the two bars of a stereo meter, in px.
pub const BAR_GAP: i32 = 2;
/// Height of the clip lamp above a bar, in px, never less than the theme's lamp.
pub const MIN_LAMP_HEIGHT: i32 = 8;
/// Width of the label gutter, in px: three glyphs at the 12 px text size.
pub const LABEL_GUTTER: i32 = 22;

/// Which colour band a level falls in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeterZone {
    /// Below the theme's mid threshold (green).
    Low,
    /// From the mid threshold up to the high one (amber).
    Mid,
    /// At or above the high threshold (red).
    High,
}

/// A bar's scale: the dB at the bottom (the floor) and the two colour thresholds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeterScale {
    pub floor_db: f32,
    pub mid_db: f32,
    pub high_db: f32,
}

impl MeterScale {
    /// The scale from the theme: `meter_floor_db`, `meter_mid_db`, `meter_high_db`.
    pub fn from_tokens(t: &ThemeTokens) -> Self {
        Self {
            floor_db: t.meter_floor_db,
            mid_db: t.meter_mid_db,
            high_db: t.meter_high_db,
        }
    }

    /// Where `db` sits on the bar, `0..=1` (the floor is 0, 0 dB is 1).
    pub fn proportion(&self, db: f32) -> f32 {
        if self.floor_db >= 0.0 {
            return 0.0;
        }
        ((db - self.floor_db) / -self.floor_db).clamp(0.0, 1.0)
    }

    /// The dB a bar proportion stands for; the inverse of [`MeterScale::proportion`].
    pub fn db_at(&self, proportion: f32) -> f32 {
        self.floor_db + proportion.clamp(0.0, 1.0) * -self.floor_db
    }

    /// The numeric peak readout under the master meter (`faders-meters.md` section 3): the held
    /// peak at `hold` (a bar proportion) in dB with one decimal, `-inf` while the bar is empty.
    pub fn peak_text(&self, hold: f32) -> String {
        if hold <= 0.0 {
            "-inf".to_owned()
        } else {
            format!("{:.1}", self.db_at(hold))
        }
    }

    /// The colour band a bar proportion falls in.
    pub fn zone(&self, proportion: f32) -> MeterZone {
        let db = self.db_at(proportion);
        if db >= self.high_db {
            MeterZone::High
        } else if db >= self.mid_db {
            MeterZone::Mid
        } else {
            MeterZone::Low
        }
    }

    /// The coloured runs that make up a bar filled to `level`, bottom first, as
    /// `(zone, from, to)` proportions. Each zone owns its own dB band, so a bar at -10 dB shows
    /// green up to -18 and amber above it, never one colour for the whole bar.
    pub fn runs(&self, level: f32) -> Vec<(MeterZone, f32, f32)> {
        let level = level.clamp(0.0, 1.0);
        let mid = self.proportion(self.mid_db);
        let high = self.proportion(self.high_db);
        [
            (MeterZone::Low, 0.0, mid),
            (MeterZone::Mid, mid, high),
            (MeterZone::High, high, 1.0),
        ]
        .into_iter()
        .filter_map(|(zone, from, to)| {
            let to = to.min(level);
            (to > from).then_some((zone, from, to))
        })
        .collect()
    }
}

/// One printed mark of the dB scale.
#[derive(Debug, Clone, PartialEq)]
pub struct TickMark {
    pub db: f32,
    /// Position on the bar, `0..=1` from the bottom.
    pub proportion: f32,
    /// `0`, `-6`, `-12`...
    pub label: String,
}

/// The marks of [`DB_TICKS`] that lie on a bar with `scale`, loudest first.
pub fn tick_marks(scale: &MeterScale) -> Vec<TickMark> {
    DB_TICKS
        .iter()
        .copied()
        .filter(|db| *db >= scale.floor_db)
        .map(|db| TickMark {
            db,
            proportion: scale.proportion(db),
            label: format!("{}", db as i32),
        })
        .collect()
}

/// Least space between two printed marks, in px: the 12 px text plus a hair.
pub const MIN_MARK_SPACING: i32 = 14;

/// The marks of [`tick_marks`] that fit a bar `bar_height` px tall: scanning from the loudest, a
/// mark closer than [`MIN_MARK_SPACING`] to the last one kept is dropped, so short meters print
/// fewer labels instead of overlapping ones.
pub fn fitting_marks(scale: &MeterScale, bar_height: i32) -> Vec<TickMark> {
    let mut kept: Vec<TickMark> = Vec::new();
    for mark in tick_marks(scale) {
        let fits = kept.last().is_none_or(|last| {
            ((last.proportion - mark.proportion) * bar_height as f32) >= MIN_MARK_SPACING as f32
        });
        if fits {
            kept.push(mark);
        }
    }
    kept
}

/// Rectangles of one stereo meter column: the dB labels on the left, then two bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeterColumn {
    pub labels: Rect,
    pub bars: [Rect; 2],
}

/// Width a stereo meter needs, with or without the label gutter. Each bar is at least
/// [`MIN_BAR_WIDTH`] wide whatever the theme's token says.
pub fn column_width(theme_bar_width: f32, with_labels: bool) -> i32 {
    let bar = (theme_bar_width.round() as i32).max(MIN_BAR_WIDTH);
    let labels = if with_labels { LABEL_GUTTER } else { 0 };
    labels + 2 * bar + BAR_GAP
}

/// Splits `area` into a label gutter (when `with_labels`) and two bars.
pub fn column_layout(area: Rect, with_labels: bool) -> MeterColumn {
    let mut rest = area;
    let labels = if with_labels {
        rest.remove_left(LABEL_GUTTER)
    } else {
        Rect::new(rest.x, rest.y, 0, rest.h)
    };
    let bar_w = ((rest.w - BAR_GAP) / 2).max(0);
    let left = Rect::new(rest.x, rest.y, bar_w, rest.h);
    let right = Rect::new(rest.x + bar_w + BAR_GAP, rest.y, bar_w, rest.h);
    MeterColumn {
        labels,
        bars: [left, right],
    }
}

/// One meter column: feeds linear peaks, reads level, hold tick and clip lamp. The clip lamp
/// latches: once a peak clips it stays lit until the meter is clicked.
#[derive(Debug, Clone)]
pub struct MeterBar {
    ballistics: MeterBallistics,
    latched: bool,
}

impl MeterBar {
    pub fn new(params: MeterParams) -> Self {
        Self {
            ballistics: MeterBallistics::new(params),
            latched: false,
        }
    }

    /// One UI frame: `peak` is the linear peak since the last frame, `dt` the frame time in seconds.
    /// Returns true when anything visible changed.
    pub fn update(&mut self, peak: f32, dt: f32) -> bool {
        let changed = self.ballistics.update(peak, dt);
        if self.ballistics.is_clipping() {
            self.latched = true;
        }
        changed
    }

    /// A click on the meter clears the clip lamp.
    pub fn click(&mut self) {
        self.ballistics.reset_clip();
        self.latched = false;
    }

    /// True from the first clipping peak until [`MeterBar::click`]: the lamp stays lit after the
    /// ballistics' own timer ran out, so a clip that happened while looking away is still seen.
    pub fn is_latched(&self) -> bool {
        self.latched || self.ballistics.is_clipping()
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

    #[test]
    fn the_peak_readout_is_one_decimal_db_or_minus_inf() {
        let scale = MeterScale {
            floor_db: -48.0,
            mid_db: -12.0,
            high_db: -3.0,
        };
        assert_eq!(scale.peak_text(0.0), "-inf");
        assert_eq!(scale.peak_text(1.0), "0.0");
        assert_eq!(scale.peak_text(scale.proportion(-2.3)), "-2.3");
    }

    fn params() -> MeterParams {
        MeterParams {
            floor_db: -48.0,
            clip_level: 0.99,
            release_db_per_second: 13.0,
            hold_seconds: 1.0,
            clip_seconds: 2.0,
        }
    }

    #[test]
    fn defaults_come_from_the_theme() {
        let p = meter_params(&ThemeTokens::builtin_dark());
        assert!(p.floor_db < 0.0);
        assert!(p.release_db_per_second > 0.0);
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

    fn scale() -> MeterScale {
        MeterScale {
            floor_db: -48.0,
            mid_db: -18.0,
            high_db: -6.0,
        }
    }

    #[test]
    fn proportions_and_decibels_are_inverse() {
        let s = scale();
        assert_eq!(s.proportion(0.0), 1.0);
        assert_eq!(s.proportion(-48.0), 0.0);
        assert_eq!(s.proportion(-96.0), 0.0, "below the floor clamps");
        assert!((s.proportion(-24.0) - 0.5).abs() < 1e-6);
        for p in [0.0, 0.25, 0.5, 0.9, 1.0] {
            assert!((s.proportion(s.db_at(p)) - p).abs() < 1e-5);
        }
    }

    #[test]
    fn zones_follow_the_theme_thresholds_in_decibels() {
        let s = scale();
        assert_eq!(s.zone(s.proportion(-30.0)), MeterZone::Low);
        assert_eq!(s.zone(s.proportion(-18.5)), MeterZone::Low);
        assert_eq!(s.zone(s.proportion(-17.9)), MeterZone::Mid);
        assert_eq!(s.zone(s.proportion(-7.0)), MeterZone::Mid);
        assert_eq!(s.zone(s.proportion(-5.9)), MeterZone::High);
        assert_eq!(s.zone(1.0), MeterZone::High);
        // The old hard-coded 0.6 level would have been amber at -19.2 dB.
        assert_eq!(s.zone(0.61), MeterZone::Low);
    }

    #[test]
    fn runs_stack_the_zones_up_to_the_level() {
        let s = scale();
        assert!(s.runs(0.0).is_empty());
        let quiet = s.runs(s.proportion(-30.0));
        assert_eq!(quiet.len(), 1);
        assert_eq!(quiet[0].0, MeterZone::Low);
        let hot = s.runs(1.0);
        let zones: Vec<_> = hot.iter().map(|r| r.0).collect();
        assert_eq!(zones, [MeterZone::Low, MeterZone::Mid, MeterZone::High]);
        for pair in hot.windows(2) {
            assert_eq!(pair[0].2, pair[1].1, "runs are contiguous");
        }
        assert_eq!(hot.last().map(|r| r.2), Some(1.0));
        assert!((hot[0].2 - s.proportion(-18.0)).abs() < 1e-6);
    }

    #[test]
    fn tick_marks_cover_the_scale_and_drop_those_under_the_floor() {
        let marks = tick_marks(&scale());
        let labels: Vec<_> = marks.iter().map(|m| m.label.as_str()).collect();
        assert_eq!(labels, ["0", "-6", "-12", "-24", "-48"]);
        assert_eq!(marks[0].proportion, 1.0);
        assert_eq!(marks[4].proportion, 0.0);
        assert!(marks.windows(2).all(|w| w[0].proportion > w[1].proportion));
        let shallow = MeterScale {
            floor_db: -30.0,
            ..scale()
        };
        assert_eq!(tick_marks(&shallow).len(), 4, "-48 is below a -30 floor");
    }

    #[test]
    fn a_short_meter_prints_fewer_labels_rather_than_overlapping_ones() {
        let s = scale();
        assert_eq!(
            fitting_marks(&s, 300).len(),
            5,
            "a tall bar has room for all"
        );
        let short = fitting_marks(&s, 60);
        assert!(short.len() < 5);
        assert_eq!(short[0].label, "0", "0 dB is always kept");
        for pair in short.windows(2) {
            let gap = (pair[0].proportion - pair[1].proportion) * 60.0;
            assert!(gap >= MIN_MARK_SPACING as f32, "{pair:?}");
        }
    }

    #[test]
    fn each_bar_is_at_least_twelve_pixels_wide() {
        // The old 12 px token gave two ~9 px bars once the borders were taken off; the well is
        // 14 px so 12 px of fill show inside the 1 px border.
        for with_labels in [false, true] {
            let w = column_width(12.0, with_labels);
            let col = column_layout(Rect::new(0, 0, w, 100), with_labels);
            for bar in col.bars {
                assert!(bar.w - 2 >= 12, "{bar:?}");
            }
            assert!(!col.bars[0].intersects(&col.bars[1]));
            assert_eq!(col.labels.w > 0, with_labels);
        }
        assert!(column_width(4.0, false) >= 2 * MIN_BAR_WIDTH, "token floor");
    }

    #[test]
    fn the_clip_lamp_latches_until_clicked() {
        let mut bar = MeterBar::new(params());
        assert!(!bar.is_latched());
        bar.update(1.0, FRAME);
        for _ in 0..(5.0 / FRAME) as usize {
            bar.update(0.0, FRAME);
        }
        assert!(!bar.ballistics().is_clipping(), "the timer ran out");
        assert!(bar.is_latched(), "the lamp did not");
        bar.click();
        assert!(!bar.is_latched());
    }

    #[test]
    fn stereo_meter_feeds_each_side_separately() {
        let mut meter = StereoMeter::new(params());
        meter.update(0.5, 0.0, FRAME);
        assert!(meter.left.ballistics().level() > 0.5);
        assert_eq!(meter.right.ballistics().level(), 0.0);
    }
}
