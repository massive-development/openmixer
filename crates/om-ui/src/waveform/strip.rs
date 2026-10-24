//! Column shapes (heights and colour bucket) for the zoomed strip and the overview, computed once
//! and reused while the view scrolls. The zoomed strip is [`CACHE_VIEWS`] views wide, so scrolling
//! is a change of offset until the playhead leaves the middle third ([`plan_strip`] decides). The
//! painting layer draws a few filled shapes from these columns instead of a quad per pixel. The
//! colour buckets are capped over the whole cached strip, so a column keeps its colour while it
//! scrolls. Pure.

use super::bands::{cap_shapes, column_shape, ColumnShape};
use super::{overview_column, plan_strip, strip_column, strip_for, StripCache, StripPlan, View};
use om_analysis::{WaveformLevel, WaveformPoint};

/// Width of one strip column in px. Two pixels halve the shape's vertex count and are still
/// finer than the eye follows on a moving waveform.
pub const STRIP_STEP_PX: f64 = 2.0;

/// What a strip is drawn from.
#[derive(Debug, Clone, Copy)]
pub struct StripSource<'level> {
    /// Identifies the waveform (its address will do); a different value drops the cached strip.
    pub track: usize,
    pub level: &'level WaveformLevel,
    pub height_px: i32,
    pub length_frames: i64,
    /// The track's [`super::reference_peak`].
    pub reference: f32,
    pub spectral: bool,
}

/// One cached strip: shapes of columns of [`STRIP_STEP_PX`] across `CACHE_VIEWS` views.
#[derive(Debug, Clone, PartialEq)]
pub struct BandStrip {
    pub cache: StripCache,
    /// Frame at the left edge of column 0.
    pub first_frame: f64,
    /// Frames one column spans.
    pub step_frames: f64,
    pub columns: Vec<ColumnShape>,
}

impl BandStrip {
    /// Renders the strip centred on `view`. Columns outside `0..length_frames` are empty.
    pub fn render(src: &StripSource, view: &View) -> Self {
        let cache = strip_for(view, src.height_px);
        let step_frames = STRIP_STEP_PX * view.frames_per_pixel;
        let first_frame =
            cache.centre_frame - f64::from(cache.width_px) * 0.5 * view.frames_per_pixel;
        let count = (f64::from(cache.width_px) / STRIP_STEP_PX).ceil() as usize;
        let mut columns: Vec<ColumnShape> = (0..count)
            .map(|i| {
                let frame = first_frame + i as f64 * step_frames;
                if frame + step_frames <= 0.0 || frame >= src.length_frames as f64 {
                    return ColumnShape::default();
                }
                strip_column(src.level, frame.max(0.0), step_frames)
                    .map(|c| column_shape(&c, src.reference, src.spectral))
                    .unwrap_or_default()
            })
            .collect();
        cap_shapes(&mut columns);
        Self {
            cache,
            first_frame,
            step_frames,
            columns,
        }
    }

    /// The columns that touch `view`, with the x of each column's centre in view pixels. Sub-pixel
    /// accurate, so a slow scroll moves smoothly.
    pub fn visible(&self, view: &View) -> Vec<(f32, ColumnShape)> {
        if self.columns.is_empty() || self.step_frames <= 0.0 {
            return Vec::new();
        }
        let from = (view.first_frame() - self.first_frame) / self.step_frames;
        let to = (view.last_frame() - self.first_frame) / self.step_frames;
        let lo = (from.floor() - 1.0).max(0.0) as usize;
        let hi = ((to.ceil() + 1.0).max(0.0) as usize).min(self.columns.len());
        (lo..hi)
            .map(|i| {
                let centre = self.first_frame + (i as f64 + 0.5) * self.step_frames;
                (view.x_of_frame_f(centre) as f32, self.columns[i])
            })
            .collect()
    }
}

/// A [`BandStrip`] kept between frames for one deck: re-rendered only when [`plan_strip`] says
/// the view left the strip's margin, or the track, scale or size changed.
#[derive(Debug, Default)]
pub struct StripSlot {
    track: usize,
    strip: Option<BandStrip>,
    /// Strips rendered into this slot, for tests and the frame-cost readout.
    pub renders: u64,
}

impl StripSlot {
    /// The strip to draw `view` from.
    pub fn get(&mut self, src: &StripSource, view: &View) -> &BandStrip {
        if self.track != src.track {
            self.track = src.track;
            self.strip = None;
        }
        let plan = plan_strip(self.strip.as_ref().map(|s| &s.cache), view, src.height_px);
        if plan == StripPlan::Render || self.strip.is_none() {
            self.strip = Some(BandStrip::render(src, view));
            self.renders += 1;
        }
        self.strip.as_ref().expect("strip rendered above")
    }
}

/// Shapes of `count` columns across the whole track, for the overview.
pub fn overview_shapes(
    points: &[WaveformPoint],
    count: usize,
    reference: f32,
    spectral: bool,
) -> Vec<ColumnShape> {
    let mut shapes: Vec<ColumnShape> = (0..count)
        .map(|i| column_shape(&overview_column(points, i, count), reference, spectral))
        .collect();
    cap_shapes(&mut shapes);
    shapes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bass-heavy level: `n` points of 128 samples with a ramping peak.
    fn level(n: usize) -> WaveformLevel {
        WaveformLevel {
            samples_per_point: 128,
            points: (0..n)
                .map(|i| {
                    let peak = 0.2 + 0.8 * (i % 100) as f32 / 100.0;
                    WaveformPoint {
                        min: -peak,
                        max: peak,
                        loudness: peak * 0.7,
                        low: peak,
                        mid: peak * 0.1,
                        high: 0.0,
                    }
                })
                .collect(),
        }
    }

    fn view(centre: i64) -> View {
        View {
            centre_frame: centre,
            frames_per_pixel: 128.0,
            width: 400.0,
        }
    }

    fn source(level: &WaveformLevel, track: usize, height_px: i32) -> StripSource<'_> {
        StripSource {
            track,
            level,
            height_px,
            length_frames: 512_000,
            reference: 1.0,
            spectral: true,
        }
    }

    #[test]
    fn strip_is_three_views_wide_and_centred() {
        let l = level(2000);
        let v = view(128_000);
        let s = BandStrip::render(&source(&l, 1, 120), &v);
        assert_eq!(s.columns.len(), (400.0 * 3.0 / STRIP_STEP_PX) as usize);
        let visible = s.visible(&v);
        assert!(
            visible.len() >= 200 && visible.len() <= 205,
            "{}",
            visible.len()
        );
        assert!(visible.first().unwrap().0 <= 1.0);
        assert!(visible.last().unwrap().0 >= 399.0);
        // The playhead frame sits at the view centre.
        let near_centre = visible
            .iter()
            .min_by(|a, b| (a.0 - 200.0).abs().total_cmp(&(b.0 - 200.0).abs()))
            .unwrap();
        assert!((near_centre.0 - 200.0).abs() <= STRIP_STEP_PX as f32);
    }

    #[test]
    fn scrolling_moves_columns_by_the_exact_pixel_distance() {
        let l = level(2000);
        let v = view(128_000);
        let s = BandStrip::render(&source(&l, 1, 120), &v);
        let here = s.visible(&v)[100];
        // 640 frames at 128 frames per pixel is 5 px to the left.
        let moved = View {
            centre_frame: 128_000 + 640,
            ..v
        };
        let expected = here.0 - 5.0;
        let found = s
            .visible(&moved)
            .into_iter()
            .filter(|(_, h)| *h == here.1)
            .min_by(|a, b| (a.0 - expected).abs().total_cmp(&(b.0 - expected).abs()))
            .unwrap();
        assert!(
            (found.0 - expected).abs() < 1e-3,
            "{} vs {expected}",
            found.0
        );
    }

    #[test]
    fn columns_outside_the_track_are_empty() {
        let l = level(100);
        let v = view(0);
        let src = StripSource {
            length_frames: 12_800,
            ..source(&l, 1, 120)
        };
        let s = BandStrip::render(&src, &v);
        let left: Vec<_> = s
            .visible(&v)
            .into_iter()
            .filter(|(x, _)| *x < 190.0)
            .collect();
        assert!(!left.is_empty());
        assert!(left.iter().all(|(_, h)| *h == ColumnShape::default()));
        let right = s.visible(&v).into_iter().find(|(x, _)| *x > 220.0).unwrap();
        assert!(right.1.silhouette > 0.0, "the track itself has height");
    }

    #[test]
    fn slot_reuses_the_strip_inside_the_margin() {
        let l = level(4000);
        let mut slot = StripSlot::default();
        let src = source(&l, 1, 120);
        slot.get(&src, &view(256_000));
        assert_eq!(slot.renders, 1);
        // 100 px of scroll is inside the 200 px margin.
        slot.get(&src, &view(256_000 + 12_800));
        assert_eq!(slot.renders, 1, "scroll reuses");
        // 300 px is beyond it.
        slot.get(&src, &view(256_000 + 38_400));
        assert_eq!(slot.renders, 2);
    }

    #[test]
    fn slot_rerenders_for_a_new_track_zoom_or_size() {
        let l = level(4000);
        let mut slot = StripSlot::default();
        let v = view(256_000);
        slot.get(&source(&l, 1, 120), &v);
        slot.get(&source(&l, 2, 120), &v);
        assert_eq!(slot.renders, 2, "new track");
        let zoomed = View {
            frames_per_pixel: 256.0,
            ..v
        };
        slot.get(&source(&l, 2, 120), &zoomed);
        assert_eq!(slot.renders, 3, "new zoom step");
        slot.get(&source(&l, 2, 90), &zoomed);
        assert_eq!(slot.renders, 4, "new height");
    }

    #[test]
    fn overview_shapes_cover_the_whole_track() {
        let l = level(1000);
        let shapes = overview_shapes(&l.points, 100, 1.0, true);
        assert_eq!(shapes.len(), 100);
        assert!(shapes.iter().all(|b| b.silhouette > 0.0 && b.body > 0.0));
        assert!(overview_shapes(&[], 10, 1.0, true)
            .iter()
            .all(|b| *b == ColumnShape::default()));
    }

    #[test]
    fn a_strip_caches_a_capped_key_per_column_and_keeps_it_while_scrolling() {
        let l = level(4000);
        let v = view(256_000);
        let s = BandStrip::render(&source(&l, 1, 120), &v);
        let mut keys: Vec<_> = s.columns.iter().map(|c| c.key).collect();
        keys.sort();
        keys.dedup();
        assert!(keys.len() <= crate::waveform::MAX_BUCKETS);
        // The same strip serves a scrolled view: a column's key does not depend on the window.
        let a = s.visible(&v);
        let b = s.visible(&View {
            centre_frame: 256_000 + 640,
            ..v
        });
        let key_at = |list: &[(f32, ColumnShape)], x: f32| {
            list.iter()
                .min_by(|p, q| (p.0 - x).abs().total_cmp(&(q.0 - x).abs()))
                .map(|c| c.1.key)
        };
        assert_eq!(key_at(&a, 150.0), key_at(&b, 145.0));
    }
}
