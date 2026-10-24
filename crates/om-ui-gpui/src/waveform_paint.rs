//! Painting the waveforms: thin code over `om_ui::waveform`, which decides every colour, height
//! and position. A strip is three filled shapes (one per band, low at the back) instead of a quad
//! per pixel column, and the band heights come from a strip cached across frames. The overview's
//! shapes are cached whole and cloned each frame. Scrolling the zoomed strip rebuilds only the
//! three shapes from the cached heights.
//!
//! GPUI 0.2.2 can paint an image (`Window::paint_image`), but building a `RenderImage` needs the
//! `image` crate's `Frame`, which gpui does not re-export; a direct dependency would change
//! `Cargo.lock`. Filled paths avoid that and keep the cost at three shapes per strip.

use crate::paint::paint;
use gpui::{fill, point, px, size, Bounds, Path, PathBuilder, Pixels, Point, Rgba, Window};
use om_analysis::WaveformData;
use om_ui::theme::Colour;
use om_ui::waveform::{
    band_colours, overview_bands, played_edge, playhead_rect, reference_peak, Band, BandHeights,
    BandStrip, OverviewMarkers, StripSlot, StripSource, View, BAR_LINE_WIDTH, BEAT_LINE_WIDTH,
    LOOP_COLOUR, LOOP_EDGE_WIDTH, OVERVIEW_CUE_TICK_WIDTH, OVERVIEW_TICK_ALPHA,
    OVERVIEW_TICK_HEIGHT, PLAYED_BRIGHTNESS, PLAYHEAD_COLOUR, PLAYHEAD_HALO, PLAYHEAD_HALO_COLOUR,
    PLAYHEAD_WIDTH,
};
use om_ui::widgets::deck::GridLine;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// Share of the strip's half-height a full-scale band reaches, leaving a margin top and bottom.
const FILL: f32 = 0.94;
/// A band that has any height is drawn at least this far from the centre line, in px.
const MIN_HALF_PX: f32 = 0.75;

/// Identity of a loaded waveform: its address. A reload gives a different `Arc`.
pub fn track_id(data: &Arc<WaveformData>) -> usize {
    Arc::as_ptr(data) as usize
}

/// A colour at a given alpha (GPUI keeps the RGB of the source).
pub fn with_alpha(colour: Colour, alpha: f32) -> Rgba {
    Rgba {
        a: alpha,
        ..paint(colour)
    }
}

/// Alpha of the dark overlay that takes a played region down to [`PLAYED_BRIGHTNESS`].
fn played_overlay_alpha() -> f32 {
    1.0 - PLAYED_BRIGHTNESS
}

struct Cached {
    track: usize,
    reference: f32,
}

#[derive(Default)]
struct Overview {
    key: Option<(usize, [f32; 4])>,
    paths: Vec<(Colour, Path<Pixels>)>,
}

thread_local! {
    static STRIPS: RefCell<HashMap<usize, StripSlot>> = RefCell::new(HashMap::new());
    static REFERENCES: RefCell<HashMap<usize, Cached>> = RefCell::new(HashMap::new());
    static OVERVIEWS: RefCell<HashMap<usize, Overview>> = RefCell::new(HashMap::new());
}

/// The track's reference peak, computed once per loaded waveform.
fn reference_for(deck: usize, data: &Arc<WaveformData>) -> f32 {
    let track = track_id(data);
    REFERENCES.with(|cache| {
        let mut cache = cache.borrow_mut();
        match cache.get(&deck) {
            Some(c) if c.track == track => c.reference,
            _ => {
                let reference = reference_peak(&data.overview().points);
                cache.insert(deck, Cached { track, reference });
                reference
            }
        }
    })
}

fn quad(window: &mut Window, bounds: Bounds<Pixels>, x: f32, y: f32, w: f32, h: f32, c: Rgba) {
    window.paint_quad(fill(
        Bounds::new(
            point(bounds.origin.x + px(x), bounds.origin.y + px(y)),
            size(px(w), px(h)),
        ),
        c,
    ));
}

/// One closed shape mirrored about `mid`: the top edge left to right, then the bottom edge back.
/// `None` when the band has no height anywhere.
fn band_path(columns: &[(f32, f32)], origin: Point<Pixels>, mid: f32) -> Option<Path<Pixels>> {
    if columns.len() < 2 || columns.iter().all(|(_, h)| *h <= 0.0) {
        return None;
    }
    let at = |x: f32, y: f32| point(origin.x + px(x), origin.y + px(y));
    let mut builder = PathBuilder::fill();
    let (x0, h0) = columns[0];
    builder.move_to(at(x0, mid - h0));
    for &(x, h) in &columns[1..] {
        builder.line_to(at(x, mid - h));
    }
    for &(x, h) in columns.iter().rev() {
        builder.line_to(at(x, mid + h));
    }
    builder.close();
    builder.build().ok()
}

/// The three band shapes, back to front, with the colour each is filled with.
fn band_paths(
    columns: &[(f32, BandHeights)],
    origin: Point<Pixels>,
    height: f32,
    spectral: bool,
) -> Vec<(Colour, Path<Pixels>)> {
    let mid = height / 2.0;
    let half = mid * FILL;
    let colours = band_colours(spectral);
    Band::ALL
        .into_iter()
        .zip(colours)
        .filter_map(|(band, colour)| {
            let shape: Vec<(f32, f32)> = columns
                .iter()
                .map(|(x, h)| {
                    let v = h.get(band);
                    (
                        *x,
                        if v > 0.0 {
                            (v * half).max(MIN_HALF_PX)
                        } else {
                            0.0
                        },
                    )
                })
                .collect();
            band_path(&shape, origin, mid).map(|p| (colour, p))
        })
        .collect()
}

fn playhead(window: &mut Window, bounds: Bounds<Pixels>, x: f64, height: f32) {
    let (left, width) = playhead_rect(x);
    quad(
        window,
        bounds,
        left,
        0.0,
        width,
        height,
        paint(PLAYHEAD_HALO_COLOUR),
    );
    quad(
        window,
        bounds,
        left + PLAYHEAD_HALO,
        0.0,
        PLAYHEAD_WIDTH,
        height,
        paint(PLAYHEAD_COLOUR),
    );
}

/// Everything the zoomed strip draws on its canvas (the cue flags and number labels are elements
/// over it).
pub struct ZoomScene {
    pub deck: usize,
    pub wave: Option<Arc<WaveformData>>,
    pub view: View,
    pub length_frames: i64,
    pub grid: Vec<GridLine>,
    pub loop_span: Option<(f64, f64, bool)>,
    pub well: Colour,
}

pub fn paint_zoom(window: &mut Window, bounds: Bounds<Pixels>, scene: &ZoomScene) {
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);
    let view = &scene.view;
    let playhead_x = view.width * 0.5;

    if let Some((x0, x1, looping)) = scene.loop_span {
        let alpha = if looping {
            om_ui::waveform::LOOP_FILL_ALPHA
        } else {
            om_ui::waveform::LOOP_IDLE_FILL_ALPHA
        };
        let (a, b) = (x0.clamp(0.0, f64::from(w)), x1.clamp(0.0, f64::from(w)));
        quad(
            window,
            bounds,
            a as f32,
            0.0,
            (b - a) as f32,
            h,
            with_alpha(LOOP_COLOUR, alpha),
        );
    }

    if let Some(data) = &scene.wave {
        let spectral = data.has_spectrum();
        let level = data.level_for(view.frames_per_pixel);
        let src = StripSource {
            track: track_id(data),
            level,
            height_px: h as i32,
            length_frames: scene.length_frames,
            reference: reference_for(scene.deck, data),
            spectral,
        };
        let columns: Vec<(f32, BandHeights)> = STRIPS.with(|slots| {
            let mut slots = slots.borrow_mut();
            let strip: &BandStrip = slots.entry(scene.deck).or_default().get(&src, view);
            strip.visible(view)
        });
        for (colour, path) in band_paths(&columns, bounds.origin, h, spectral) {
            window.paint_path(path, paint(colour));
        }
    }

    // The played part, left of the playhead, keeps 45% of its brightness.
    let edge = played_edge(view, playhead_x) as f32;
    quad(
        window,
        bounds,
        0.0,
        0.0,
        edge.min(w),
        h,
        with_alpha(scene.well, played_overlay_alpha()),
    );

    for line in &scene.grid {
        let (width, alpha) = if line.bar {
            (BAR_LINE_WIDTH, om_ui::waveform::BAR_LINE_ALPHA)
        } else {
            (BEAT_LINE_WIDTH, om_ui::waveform::BEAT_LINE_ALPHA)
        };
        quad(
            window,
            bounds,
            line.x as f32 - width / 2.0,
            0.0,
            width,
            h,
            with_alpha(om_ui::waveform::GRID_COLOUR, alpha),
        );
    }

    if let Some((x0, x1, looping)) = scene.loop_span {
        let alpha = if looping {
            om_ui::waveform::LOOP_EDGE_ALPHA
        } else {
            om_ui::waveform::LOOP_IDLE_EDGE_ALPHA
        };
        for x in [x0 as f32, x1 as f32] {
            quad(
                window,
                bounds,
                x - LOOP_EDGE_WIDTH / 2.0,
                0.0,
                LOOP_EDGE_WIDTH,
                h,
                with_alpha(LOOP_COLOUR, alpha),
            );
        }
    }

    playhead(window, bounds, playhead_x, h);
}

/// Everything the overview draws.
pub struct OverviewScene {
    pub deck: usize,
    pub wave: Option<Arc<WaveformData>>,
    pub markers: OverviewMarkers,
    pub well: Colour,
}

pub fn paint_overview(window: &mut Window, bounds: Bounds<Pixels>, scene: &OverviewScene) {
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);
    let m = &scene.markers;

    if let Some(data) = &scene.wave {
        let points = &data.overview().points;
        if !points.is_empty() && w >= 2.0 {
            let key = (
                track_id(data),
                [f32::from(bounds.origin.x), f32::from(bounds.origin.y), w, h],
            );
            OVERVIEWS.with(|cache| {
                let mut cache = cache.borrow_mut();
                let entry = cache.entry(scene.deck).or_default();
                if entry.key != Some(key) {
                    let spectral = data.has_spectrum();
                    let count = (w / 2.0).ceil() as usize;
                    let step = w / count as f32;
                    let heights =
                        overview_bands(points, count, reference_for(scene.deck, data), spectral);
                    let columns: Vec<(f32, BandHeights)> = heights
                        .into_iter()
                        .enumerate()
                        .map(|(i, hts)| ((i as f32 + 0.5) * step, hts))
                        .collect();
                    entry.paths = band_paths(&columns, bounds.origin, h, spectral);
                    entry.key = Some(key);
                }
                for (colour, path) in &entry.paths {
                    window.paint_path(path.clone(), paint(*colour));
                }
            });
        }
    }

    if let Some((a, b, looping)) = m.loop_span {
        let alpha = if looping {
            om_ui::waveform::LOOP_FILL_ALPHA
        } else {
            om_ui::waveform::LOOP_IDLE_FILL_ALPHA
        };
        let (a, b) = (a as f32, (b as f32).max(a as f32 + 1.0));
        quad(
            window,
            bounds,
            a,
            0.0,
            b - a,
            h,
            with_alpha(LOOP_COLOUR, alpha),
        );
        let edge = if looping {
            om_ui::waveform::LOOP_EDGE_ALPHA
        } else {
            om_ui::waveform::LOOP_IDLE_EDGE_ALPHA
        };
        for x in [a, b] {
            quad(
                window,
                bounds,
                x - 0.5,
                0.0,
                1.0,
                h,
                with_alpha(LOOP_COLOUR, edge),
            );
        }
    }

    // Bar ticks along both edges.
    for x in &m.bar_ticks {
        let c = with_alpha(om_ui::waveform::GRID_COLOUR, OVERVIEW_TICK_ALPHA);
        quad(window, bounds, *x as f32, 0.0, 1.0, OVERVIEW_TICK_HEIGHT, c);
        quad(
            window,
            bounds,
            *x as f32,
            h - OVERVIEW_TICK_HEIGHT,
            1.0,
            OVERVIEW_TICK_HEIGHT,
            c,
        );
    }

    // The played part keeps 45% of its brightness.
    let played = (m.playhead_x as f32).clamp(0.0, w);
    quad(
        window,
        bounds,
        0.0,
        0.0,
        played,
        h,
        with_alpha(scene.well, played_overlay_alpha()),
    );

    // Hot cue ticks: a full-height line and a square on top, in the cue's colour.
    for (x, colour) in &m.cues {
        let x = *x as f32 - OVERVIEW_CUE_TICK_WIDTH / 2.0;
        quad(
            window,
            bounds,
            x,
            0.0,
            OVERVIEW_CUE_TICK_WIDTH,
            h,
            paint(*colour),
        );
        quad(window, bounds, x - 1.0, 0.0, 4.0, 4.0, paint(*colour));
    }

    playhead(window, bounds, m.playhead_x, h);
}
