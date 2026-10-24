//! Painting the waveforms: thin code over `om_ui::waveform`, which decides every colour, height
//! and position. A strip is a handful of filled shapes instead of a quad per pixel column: the
//! peak silhouette, then one shape per colour bucket made of the runs of columns in that bucket
//! (at most 25 paths), or three nested shapes in the three-band style. The shapes come from a
//! strip cached across frames; the overview's are cached whole and cloned each frame. Scrolling the
//! zoomed strip rebuilds only the shapes from the cached heights and keys.
//!
//! GPUI 0.2.2 can paint an image (`Window::paint_image`), but building a `RenderImage` needs the
//! `image` crate's `Frame`, which gpui does not re-export; a direct dependency would change
//! `Cargo.lock`. Filled paths avoid that.

use crate::paint::paint;
use gpui::{fill, point, px, size, Bounds, Path, PathBuilder, Pixels, Point, Rgba, Window};
use om_analysis::WaveformData;
use om_ui::theme::Colour;
use om_ui::waveform::{
    bucket_runs, overview_shapes, played_edge, playhead_rect, reference_peak, shape_px, Band,
    BandStrip, ColumnShape, OverviewMarkers, Palette, StripSlot, StripSource, View, WaveStyle,
    BAR_LINE_WIDTH, BEAT_LINE_WIDTH, LOOP_COLOUR, LOOP_EDGE_WIDTH, OVERVIEW_CUE_TICK_WIDTH,
    OVERVIEW_TICK_ALPHA, OVERVIEW_TICK_HEIGHT, PLAYED_BRIGHTNESS, PLAYHEAD_COLOUR, PLAYHEAD_HALO,
    PLAYHEAD_HALO_COLOUR, PLAYHEAD_WIDTH,
};
use om_ui::widgets::deck::GridLine;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::Arc;

/// Alpha of the full-band peak silhouette layer (plan, Rendering model 3).
const SILHOUETTE_ALPHA: f32 = 0.35;

/// Environment variable that picks the style until a setting exists.
pub const STYLE_ENV: &str = "OPENMIXER_WAVE_STYLE";

thread_local! {
    static STYLE: Cell<Option<WaveStyle>> = const { Cell::new(None) };
}

/// The style waveforms are drawn in: [`set_style`], else `OPENMIXER_WAVE_STYLE`, else Colour.
pub fn style() -> WaveStyle {
    STYLE.with(|cell| {
        cell.get().unwrap_or_else(|| {
            let from_env = std::env::var(STYLE_ENV)
                .ok()
                .and_then(|name| WaveStyle::from_name(&name))
                .unwrap_or_default();
            cell.set(Some(from_env));
            from_env
        })
    })
}

pub fn set_style(style: WaveStyle) {
    STYLE.with(|cell| cell.set(Some(style)));
}

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
    key: Option<OverviewKey>,
    paths: Vec<(Rgba, Path<Pixels>)>,
}

/// What an overview's cached shapes were built for. A different style or theme gives a different key.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverviewKey {
    track: usize,
    bounds: [f32; 4],
    style: WaveStyle,
    dark: bool,
}

pub fn overview_key(track: usize, bounds: [f32; 4], style: WaveStyle, dark: bool) -> OverviewKey {
    OverviewKey {
        track,
        bounds,
        style,
        dark,
    }
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

/// Closed shapes mirrored about `mid`, one per run: the top edge left to right, then the bottom
/// edge back. Runs with no height anywhere are skipped; `None` when nothing is left.
fn runs_path(runs: &[&[(f32, f32)]], origin: Point<Pixels>, mid: f32) -> Option<Path<Pixels>> {
    let at = |x: f32, y: f32| point(origin.x + px(x), origin.y + px(y));
    let mut builder = PathBuilder::fill();
    let mut any = false;
    for columns in runs {
        if columns.len() < 2 || columns.iter().all(|(_, h)| *h <= 0.0) {
            continue;
        }
        any = true;
        let (x0, h0) = columns[0];
        builder.move_to(at(x0, mid - h0));
        for &(x, h) in &columns[1..] {
            builder.line_to(at(x, mid - h));
        }
        for &(x, h) in columns.iter().rev() {
            builder.line_to(at(x, mid + h));
        }
        builder.close();
    }
    if any {
        builder.build().ok()
    } else {
        None
    }
}

/// The shapes of a strip, back to front, with the colour each is filled with. Colour, mono and
/// colour-blind styles draw the silhouette, then one path per colour bucket; the three-band style
/// draws three nested paths; a waveform without spectrum is one grey silhouette.
fn column_paths(
    columns: &[(f32, ColumnShape)],
    origin: Point<Pixels>,
    height: f32,
    palette: &Palette,
    spectral: bool,
) -> Vec<(Rgba, Path<Pixels>)> {
    let mid = height / 2.0;
    let mut out = Vec::new();
    let silhouette: Vec<(f32, f32)> = columns
        .iter()
        .map(|(x, c)| (*x, shape_px(c.silhouette, height)))
        .collect();
    if !spectral {
        out.extend(
            runs_path(&[&silhouette], origin, mid)
                .map(|p| (with_alpha(palette.silhouette, 1.0), p)),
        );
        return out;
    }
    if palette.style == WaveStyle::ThreeBand {
        for (band, colour) in Band::ALL.into_iter().zip(palette.bands()) {
            let shape: Vec<(f32, f32)> = columns
                .iter()
                .map(|(x, c)| (*x, shape_px(c.bands.get(band), height)))
                .collect();
            out.extend(runs_path(&[&shape], origin, mid).map(|p| (with_alpha(colour, 1.0), p)));
        }
        return out;
    }
    out.extend(
        runs_path(&[&silhouette], origin, mid)
            .map(|p| (with_alpha(palette.silhouette, SILHOUETTE_ALPHA), p)),
    );
    let body: Vec<(f32, f32)> = columns
        .iter()
        .map(|(x, c)| (*x, shape_px(c.body, height)))
        .collect();
    let keys: Vec<_> = columns.iter().map(|(_, c)| c.key).collect();
    for (key, ranges) in bucket_runs(&keys) {
        // One extra column at the end of each run, shared with the next run, so runs do not gap.
        let runs: Vec<&[(f32, f32)]> = ranges
            .iter()
            .map(|r| &body[r.start..(r.end + 1).min(body.len())])
            .collect();
        out.extend(
            runs_path(&runs, origin, mid).map(|p| (with_alpha(palette.bucket_colour(key), 1.0), p)),
        );
    }
    out
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
        let columns: Vec<(f32, ColumnShape)> = STRIPS.with(|slots| {
            let mut slots = slots.borrow_mut();
            let strip: &BandStrip = slots.entry(scene.deck).or_default().get(&src, view);
            strip.visible(view)
        });
        let palette = Palette::new(style(), scene.well);
        for (colour, path) in column_paths(&columns, bounds.origin, h, &palette, spectral) {
            window.paint_path(path, colour);
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
            let palette = Palette::new(style(), scene.well);
            let key = overview_key(
                track_id(data),
                [f32::from(bounds.origin.x), f32::from(bounds.origin.y), w, h],
                palette.style,
                palette.dark,
            );
            OVERVIEWS.with(|cache| {
                let mut cache = cache.borrow_mut();
                let entry = cache.entry(scene.deck).or_default();
                if entry.key != Some(key) {
                    let spectral = data.has_spectrum();
                    let count = (w / 2.0).ceil() as usize;
                    let step = w / count as f32;
                    let shapes =
                        overview_shapes(points, count, reference_for(scene.deck, data), spectral);
                    let columns: Vec<(f32, ColumnShape)> = shapes
                        .into_iter()
                        .enumerate()
                        .map(|(i, shape)| ((i as f32 + 0.5) * step, shape))
                        .collect();
                    entry.paths = column_paths(&columns, bounds.origin, h, &palette, spectral);
                    entry.key = Some(key);
                }
                for (colour, path) in &entry.paths {
                    window.paint_path(path.clone(), *colour);
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
