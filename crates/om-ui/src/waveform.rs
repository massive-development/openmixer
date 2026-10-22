//! Pixel <-> frame geometry for the overview and zoomed waveforms, and the zoomed strip cache
//! decisions. Pure: no painting, so seek-by-click and scrolling can be tested headless. Includes the zoomed strip cache rules.

use crate::theme::{Colour, ThemeTokens};
use om_analysis::WaveformPoint;

pub const MIN_FRAMES_PER_PIXEL: f64 = 32.0;
pub const MAX_FRAMES_PER_PIXEL: f64 = 8192.0;
/// Each wheel notch multiplies or divides the zoom by this.
pub const WHEEL_ZOOM_STEP: f64 = 1.25;
/// The cached strip is three views wide: one visible, one of margin each side.
pub const CACHE_VIEWS: i32 = 3;

/// Peak and band energies of the waveform points under one pixel column.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Column {
    /// Loudest sample, 0..=1.
    pub peak: f32,
    pub low: f32,
    pub mid: f32,
    pub high: f32,
}

/// Folds the points a pixel column covers into one: the loudest peak, the mean band energy.
pub fn column(points: &[WaveformPoint]) -> Column {
    if points.is_empty() {
        return Column::default();
    }
    let n = points.len() as f32;
    let mut out = Column::default();
    for p in points {
        out.peak = out.peak.max(p.min.abs().max(p.max.abs()).min(1.0));
        out.low += p.low / n;
        out.mid += p.mid / n;
        out.high += p.high / n;
    }
    out
}

/// How loud each band usually is in one track. Music is bass heavy, so raw band levels would paint
/// nearly everything in the low colour; dividing by the track's own average shows which band stands
/// out at each point instead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandBalance {
    low: f32,
    mid: f32,
    high: f32,
}

/// Floor for a band average, so a band with no energy cannot blow up the ratio.
const BALANCE_FLOOR: f32 = 1e-4;
/// Exponent on the relative band energy: above 1 favours the standout band, so colours stay vivid
/// instead of averaging to grey.
const BAND_CONTRAST: f32 = 2.0;

impl Default for BandBalance {
    fn default() -> Self {
        Self {
            low: 1.0,
            mid: 1.0,
            high: 1.0,
        }
    }
}

impl BandBalance {
    /// The average band energy over `points` (use the overview level).
    pub fn of(points: &[WaveformPoint]) -> Self {
        if points.is_empty() {
            return Self::default();
        }
        let c = column(points);
        Self {
            low: c.low.max(BALANCE_FLOOR),
            mid: c.mid.max(BALANCE_FLOOR),
            high: c.high.max(BALANCE_FLOOR),
        }
    }

    /// Colour for a column: the three band tokens blended by how far each band stands above this
    /// track's average. Silence takes the mid colour.
    pub fn colour(&self, c: &Column, tokens: &ThemeTokens) -> Colour {
        let rel = |band: f32, avg: f32| (band / avg).powf(BAND_CONTRAST);
        let parts = [
            (tokens.wave_low, rel(c.low, self.low)),
            (tokens.wave_mid, rel(c.mid, self.mid)),
            (tokens.wave_high, rel(c.high, self.high)),
        ];
        if parts.iter().all(|(_, w)| *w <= f32::EPSILON) {
            return tokens.wave_mid;
        }
        Colour::mix(&parts)
    }
}

/// Clamps a zoom level into the supported range.
pub fn clamp_zoom(frames_per_pixel: f64) -> f64 {
    frames_per_pixel.clamp(MIN_FRAMES_PER_PIXEL, MAX_FRAMES_PER_PIXEL)
}

/// Overview: the whole track spans the width. Click position to a seek fraction, clamped to 0..=1.
pub fn x_to_fraction(x: f64, width: f64) -> f64 {
    if width > 0.0 {
        (x / width).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Overview: frame position to x. Zero when the track has no length.
pub fn frame_to_x(frame: i64, length: i64, width: f64) -> f64 {
    if length > 0 {
        width * frame as f64 / length as f64
    } else {
        0.0
    }
}

/// Zoomed view: centred on `centre_frame`, `frames_per_pixel` sets the scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub centre_frame: i64,
    pub frames_per_pixel: f64,
    pub width: f64,
}

impl View {
    pub fn x_of_frame(&self, frame: i64) -> f64 {
        self.width * 0.5 + (frame - self.centre_frame) as f64 / self.frames_per_pixel
    }

    pub fn frame_at_x(&self, x: f64) -> f64 {
        self.centre_frame as f64 + (x - self.width * 0.5) * self.frames_per_pixel
    }

    pub fn first_frame(&self) -> f64 {
        self.frame_at_x(0.0)
    }

    pub fn last_frame(&self) -> f64 {
        self.frame_at_x(self.width)
    }
}

/// What the zoomed strip cache must do for a view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StripPlan {
    /// Reuse the cached strip, drawn `offset_px` to the left of the view's origin.
    Reuse { offset_px: i32 },
    /// Render a new strip for the view (the strip is then centred on the view).
    Render,
}

/// The cached zoomed strip: what it was rendered for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StripCache {
    pub centre_frame: f64,
    pub frames_per_pixel: f64,
    pub width_px: i32,
    pub height_px: i32,
}

/// Decides whether `cache` still serves `view` (`width_px` x `height_px` widget). A strip is stale
/// when its scale, size or width differs, or when the view has moved more than half a view width
/// away from the strip centre.
pub fn plan_strip(cache: Option<&StripCache>, view: &View, height_px: i32) -> StripPlan {
    let width = view.width.round() as i32;
    let Some(cache) = cache else {
        return StripPlan::Render;
    };
    let stale = cache.height_px != height_px
        || cache.width_px != width * CACHE_VIEWS
        || cache.frames_per_pixel != view.frames_per_pixel;
    let moved_px = (cache.centre_frame - view.centre_frame as f64) / view.frames_per_pixel;
    if stale || moved_px.abs() > view.width * 0.5 {
        return StripPlan::Render;
    }
    let offset = moved_px - view.width;
    StripPlan::Reuse {
        offset_px: offset.round() as i32,
    }
}

/// A new strip for `view`, as the cache records it.
pub fn strip_for(view: &View, height_px: i32) -> StripCache {
    StripCache {
        centre_frame: view.centre_frame as f64,
        frames_per_pixel: view.frames_per_pixel,
        width_px: view.width.round() as i32 * CACHE_VIEWS,
        height_px,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(peak: f32, low: f32, mid: f32, high: f32) -> WaveformPoint {
        WaveformPoint {
            min: -peak,
            max: peak,
            low,
            mid,
            high,
        }
    }

    #[test]
    fn column_takes_the_loudest_peak_and_the_mean_bands() {
        let c = column(&[point(0.2, 0.1, 0.0, 0.0), point(0.9, 0.3, 0.2, 0.0)]);
        assert_eq!(c.peak, 0.9);
        assert!((c.low - 0.2).abs() < 1e-6);
        assert!((c.mid - 0.1).abs() < 1e-6);
        assert_eq!(column(&[]), Column::default());
    }

    #[test]
    fn a_standout_band_sets_the_colour_even_when_bass_dominates_the_track() {
        let t = ThemeTokens::builtin_dark();
        // A bass-heavy track: low is ten times mid and high on average.
        let avg = [point(0.5, 0.5, 0.05, 0.05), point(0.5, 0.5, 0.05, 0.05)];
        let balance = BandBalance::of(&avg);
        let bass = balance.colour(&column(&[point(0.5, 0.5, 0.05, 0.05)]), &t);
        let hats = balance.colour(&column(&[point(0.5, 0.05, 0.05, 0.5)]), &t);
        assert_eq!(
            bass,
            Colour::mix(&[(t.wave_low, 1.0), (t.wave_mid, 1.0), (t.wave_high, 1.0)])
        );
        // Hats stand out in the high band, so they lean to the high colour, not the low one.
        let to_high = |c: Colour| {
            let d = |a: Colour| {
                let (x, y) = (a.argb(), c.argb());
                (0..3)
                    .map(|i| {
                        let s = 8 * i;
                        (((x >> s) & 0xFF) as i32 - ((y >> s) & 0xFF) as i32).abs()
                    })
                    .sum::<i32>()
            };
            d(t.wave_high) < d(t.wave_low)
        };
        assert!(to_high(hats));
        assert!(!to_high(Colour::mix(&[(t.wave_low, 1.0)])));
    }

    #[test]
    fn silence_takes_the_mid_colour_and_mix_handles_edges() {
        let t = ThemeTokens::builtin_dark();
        let balance = BandBalance::default();
        assert_eq!(balance.colour(&Column::default(), &t), t.wave_mid);
        assert_eq!(Colour::mix(&[]), Colour::from_argb(0));
        assert_eq!(Colour::mix(&[(t.wave_low, 0.0)]), t.wave_low);
        assert_eq!(Colour::mix(&[(t.wave_low, 3.0)]), t.wave_low);
    }

    #[test]
    fn maps_pixels_and_frames() {
        assert_eq!(x_to_fraction(50.0, 200.0), 0.25);
        assert_eq!(x_to_fraction(5.0, 0.0), 0.0, "zero width is safe");
        assert_eq!(x_to_fraction(-30.0, 400.0), 0.0);
        assert_eq!(x_to_fraction(999.0, 400.0), 1.0);
        assert_eq!(frame_to_x(500, 1000, 200.0), 100.0);
        assert_eq!(frame_to_x(500, 0, 200.0), 0.0);
    }

    #[test]
    fn view_centres_the_playhead() {
        let view = View {
            centre_frame: 10_000,
            frames_per_pixel: 100.0,
            width: 400.0,
        };
        assert_eq!(
            view.x_of_frame(10_000),
            200.0,
            "the playhead frame sits at the centre"
        );
        assert_eq!(view.x_of_frame(10_100), 201.0);
        assert_eq!(view.frame_at_x(0.0), -10_000.0);
        assert_eq!(view.last_frame(), 30_000.0);
    }

    #[test]
    fn zoom_is_clamped() {
        assert_eq!(clamp_zoom(1.0), MIN_FRAMES_PER_PIXEL);
        assert_eq!(clamp_zoom(1.0e9), MAX_FRAMES_PER_PIXEL);
        assert_eq!(clamp_zoom(512.0), 512.0);
    }

    #[test]
    fn strip_is_reused_within_the_margin_and_rerendered_beyond_it() {
        let view = View {
            centre_frame: 0,
            frames_per_pixel: 32.0,
            width: 400.0,
        };
        let strip = strip_for(&view, 120);
        assert_eq!(plan_strip(None, &view, 120), StripPlan::Render);
        assert_eq!(
            plan_strip(Some(&strip), &view, 120),
            StripPlan::Reuse { offset_px: -400 }
        );

        // 100 px of scroll is inside the 200 px margin.
        let near = View {
            centre_frame: 3200,
            ..view
        };
        assert!(matches!(
            plan_strip(Some(&strip), &near, 120),
            StripPlan::Reuse { .. }
        ));
        // 500 px is beyond it.
        let far = View {
            centre_frame: 16_000,
            ..view
        };
        assert_eq!(plan_strip(Some(&strip), &far, 120), StripPlan::Render);
    }

    #[test]
    fn strip_is_rerendered_when_scale_or_size_changes() {
        let view = View {
            centre_frame: 0,
            frames_per_pixel: 256.0,
            width: 400.0,
        };
        let strip = strip_for(&view, 120);
        let zoomed = View {
            frames_per_pixel: 512.0,
            ..view
        };
        assert_eq!(plan_strip(Some(&strip), &zoomed, 120), StripPlan::Render);
        let resized = View {
            width: 300.0,
            ..view
        };
        assert_eq!(plan_strip(Some(&strip), &resized, 120), StripPlan::Render);
        assert_eq!(
            plan_strip(Some(&strip), &view, 100),
            StripPlan::Render,
            "height change"
        );
    }
}
