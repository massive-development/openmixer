//! Pixel <-> frame geometry for the overview and zoomed waveforms, and the zoomed strip cache
//! decisions. Pure: no painting, so seek-by-click and scrolling can be tested headless. Includes the zoomed strip cache rules.

use om_analysis::{WaveformLevel, WaveformPoint};

mod clock;
mod bands;
// The spectral-centroid rainbow: superseded by the three fixed bands in `bands`, kept for the
// tuning example and its tests. The GPUI layer no longer paints with it.
mod colour;
mod hue;
mod markers;
mod strip;
mod zoom;

pub use clock::PlayheadClock;
pub use bands::*;
pub use colour::{hsl_to_colour, is_audible, loudness_level, wave_colour, wave_hsl, Hsl};
pub use hue::{hue_span, overview_hues, HUE_SPAN_COLUMNS, HUE_SPAN_PERCENTILES};
pub use markers::*;
pub use strip::*;
pub use zoom::*;

/// The cached strip is three views wide: one visible, one of margin each side.
pub const CACHE_VIEWS: i32 = 3;

/// One pixel column: the loudest peak of the points under it, and their merged spectrum.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Column {
    /// Loudest sample, 0..=1.
    pub peak: f32,
    /// RMS, 0..=1.
    pub loudness: f32,
    /// Spectral centroid on the stored log axis, 0..=1.
    pub centroid: f32,
    /// Spectral spread, 0..=1 of `SPREAD_MAX_OCTAVES`.
    pub spread: f32,
}

/// Folds the points a pixel column covers into one, with the same rule the pyramid levels use, so
/// a column over fine points and the coarser level over the same span agree.
pub fn column(points: &[WaveformPoint]) -> Column {
    if points.is_empty() {
        return Column::default();
    }
    let merged = WaveformPoint::merge(points);
    Column {
        peak: merged.min.abs().max(merged.max.abs()).min(1.0),
        loudness: merged.loudness,
        centroid: merged.centroid,
        spread: merged.spread,
    }
}

/// Overview: column `index` of `count` over the whole level.
pub fn overview_column(points: &[WaveformPoint], index: usize, count: usize) -> Column {
    let count = count.max(1);
    let lo = index * points.len() / count;
    let hi = ((index + 1) * points.len() / count)
        .max(lo + 1)
        .min(points.len());
    points.get(lo..hi).map(column).unwrap_or_default()
}

/// Zoomed strip: the column that starts at `frame` and spans `frames_per_pixel` frames of `level`.
/// `None` outside the level.
pub fn strip_column(level: &WaveformLevel, frame: f64, frames_per_pixel: f64) -> Option<Column> {
    if frame < 0.0 {
        return None;
    }
    let per_point = level.samples_per_point.max(1) as f64;
    let lo = (frame / per_point) as usize;
    let hi = (((frame + frames_per_pixel) / per_point).ceil() as usize)
        .max(lo + 1)
        .min(level.points.len());
    level.points.get(lo..hi).map(column)
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

    /// [`Self::x_of_frame`] for a fractional frame.
    pub fn x_of_frame_f(&self, frame: f64) -> f64 {
        self.width * 0.5 + (frame - self.centre_frame as f64) / self.frames_per_pixel
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

    fn point(peak: f32, loudness: f32, centroid: f32, spread: f32) -> WaveformPoint {
        WaveformPoint {
            min: -peak,
            max: peak,
            loudness,
            centroid,
            spread,
        }
    }

    #[test]
    fn column_takes_the_loudest_peak_and_merges_the_spectrum() {
        let c = column(&[point(0.2, 0.1, 0.2, 0.0), point(0.9, 0.3, 0.8, 0.0)]);
        assert_eq!(c.peak, 0.9);
        assert!(c.centroid > 0.5, "the louder point pulls the centroid");
        assert!((c.loudness - (0.5f32 * (0.01 + 0.09)).sqrt()).abs() < 1e-5);
        assert_eq!(column(&[]), Column::default());
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
