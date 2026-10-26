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
use gpui::{
    fill, point, px, size, Bounds, ContentMask, Path, PathBuilder, Pixels, Point, Rgba, Window,
};
use om_analysis::WaveformData;
use om_ui::theme::Colour;
use om_ui::waveform::{
    column_tiles, overview_shapes, played_edge, playhead_rect, shape_px, Band, BandStrip,
    ColumnShape, OverviewMarkers, Palette, Reference, StripSlot, StripSource, View, WaveStyle,
    BAR_LINE_WIDTH, BEAT_LINE_WIDTH, LOOP_COLOUR, LOOP_EDGE_WIDTH, OVERVIEW_CUE_TICK_WIDTH,
    OVERVIEW_TICK_ALPHA, OVERVIEW_TICK_HEIGHT, PLAYHEAD_COLOUR, PLAYHEAD_HALO,
    PLAYHEAD_HALO_COLOUR, PLAYHEAD_WIDTH,
};
use om_ui::widgets::deck::GridLine;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::Arc;

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

/// Alpha of the translucent peak outline (plan, Rendering model 3).
const SILHOUETTE_ALPHA: f32 = 0.35;

struct Cached {
    track: usize,
    reference: Reference,
}

#[derive(Default)]
struct Overview {
    key: Option<OverviewKey>,
    layers: Vec<Layer>,
}

/// What an overview's cached shapes were built for. A different style gives a different key (the strip background is the same in every theme).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverviewKey {
    track: usize,
    bounds: [f32; 4],
    style: WaveStyle,
}

pub fn overview_key(track: usize, bounds: [f32; 4], style: WaveStyle) -> OverviewKey {
    OverviewKey {
        track,
        bounds,
        style,
    }
}

thread_local! {
    static STRIPS: RefCell<HashMap<usize, StripSlot>> = RefCell::new(HashMap::new());
    static REFERENCES: RefCell<HashMap<usize, Cached>> = RefCell::new(HashMap::new());
    static OVERVIEWS: RefCell<HashMap<usize, Overview>> = RefCell::new(HashMap::new());
}

/// The track's reference peak, computed once per loaded waveform.
fn reference_for(deck: usize, data: &Arc<WaveformData>) -> Reference {
    let track = track_id(data);
    REFERENCES.with(|cache| {
        let mut cache = cache.borrow_mut();
        match cache.get(&deck) {
            Some(c) if c.track == track => c.reference,
            _ => {
                let reference = Reference::of(&data.overview().points);
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

/// One thing a strip draws: a filled path (colour, alpha) or a column quad (x, y, width, height
/// relative to the strip, and its colour). Colours are the unplayed ones; the played part is
/// derived when painting ([`Palette::played`]).
enum Layer {
    Path(Colour, f32, Path<Pixels>),
    Quad([f32; 4], Colour),
}

/// The layers of a strip, back to front. Colour, mono and colour-blind styles draw one quad per
/// column, tiled on whole pixels, each in its own colour from the column's band weights: colour
/// is never quantised into shared buckets, because a bucket boundary between neighbouring columns
/// shows as flicker. The three-band style draws three nested paths; a waveform without spectrum is
/// one grey silhouette path.
fn column_layers(
    columns: &[(f32, ColumnShape)],
    origin: Point<Pixels>,
    height: f32,
    palette: &Palette,
    spectral: bool,
) -> Vec<Layer> {
    let mid = height / 2.0;
    let mut out = Vec::new();
    if !spectral || palette.style == WaveStyle::ThreeBand {
        let draw: Vec<(Colour, Vec<(f32, f32)>)> = if spectral {
            Band::ALL
                .into_iter()
                .zip(palette.bands())
                .map(|(band, colour)| {
                    let shape = columns
                        .iter()
                        .map(|(x, c)| (*x, shape_px(c.bands.get(band), height)))
                        .collect();
                    (colour, shape)
                })
                .collect()
        } else {
            let shape = columns
                .iter()
                .map(|(x, c)| (*x, shape_px(c.silhouette, height)))
                .collect();
            vec![(palette.silhouette, shape)]
        };
        for (colour, shape) in draw {
            out.extend(runs_path(&[&shape], origin, mid).map(|p| Layer::Path(colour, 1.0, p)));
        }
        return out;
    }
    // The peak outline behind the bodies, as a translucent shadow (Mixxx draws its unscaled
    // envelope outside the scaled signal at alpha 0.4; the plan says 0.35).
    let outline: Vec<(f32, f32)> = columns
        .iter()
        .map(|(x, c)| (*x, shape_px(c.silhouette, height)))
        .collect();
    out.extend(
        runs_path(&[&outline], origin, mid)
            .map(|p| Layer::Path(palette.silhouette, SILHOUETTE_ALPHA, p)),
    );
    let centres: Vec<f32> = columns.iter().map(|(x, _)| *x).collect();
    for ((left, width), (_, c)) in column_tiles(&centres).into_iter().zip(columns) {
        let half = shape_px(c.body, height);
        if half > 0.0 {
            out.push(Layer::Quad(
                [left as f32, mid - half, width as f32, 2.0 * half],
                palette.colour(c.weights),
            ));
        }
    }
    out
}

/// Paints `layers`; the part left of `played_x` (strip pixels) is drawn in the played colours.
fn paint_layers(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    layers: &[Layer],
    palette: &Palette,
    played_x: f32,
) {
    let mask = |from: f32, to: f32| {
        Some(ContentMask {
            bounds: Bounds::new(
                point(bounds.origin.x + px(from), bounds.origin.y),
                size(px((to - from).max(0.0)), bounds.size.height),
            ),
        })
    };
    let width = f32::from(bounds.size.width);
    let split = played_x.clamp(0.0, width);
    for layer in layers {
        match layer {
            Layer::Path(colour, alpha, path) => {
                if split > 0.0 {
                    window.with_content_mask(mask(0.0, split), |w| {
                        w.paint_path(path.clone(), with_alpha(palette.played(*colour), *alpha));
                    });
                }
                if split < width {
                    window.with_content_mask(mask(split, width), |w| {
                        w.paint_path(path.clone(), with_alpha(*colour, *alpha));
                    });
                }
            }
            Layer::Quad([x, y, w, h], colour) => {
                let c = if x + w / 2.0 < split {
                    palette.played(*colour)
                } else {
                    *colour
                };
                quad(window, bounds, *x, *y, *w, *h, with_alpha(c, 1.0));
            }
        }
    }
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

/// Diagonal hatch lines over a loop, as one filled path of 2 px wide parallelograms.
fn paint_hatch(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    span: (f32, f32),
    h: f32,
    colour: Rgba,
) {
    let lines =
        om_ui::waveform::loop_hatch_lines(span.0, span.1, h, om_ui::waveform::LOOP_HATCH_SPACING);
    if lines.is_empty() {
        return;
    }
    let at = |x: f32, y: f32| point(bounds.origin.x + px(x), bounds.origin.y + px(y));
    let half = om_ui::waveform::LOOP_HATCH_WIDTH / 2.0;
    let mut builder = PathBuilder::fill();
    for ((bx, by), (tx, ty)) in lines {
        builder.move_to(at(bx - half, by));
        builder.line_to(at(bx + half, by));
        builder.line_to(at(tx + half, ty));
        builder.line_to(at(tx - half, ty));
        builder.close();
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, colour);
    }
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
        let layers = column_layers(&columns, bounds.origin, h, &palette, spectral);
        // The played part is left of the playhead.
        let edge = played_edge(view, playhead_x) as f32;
        paint_layers(window, bounds, &layers, &palette, edge);
    }

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

    if let (Some((x0, x1, looping)), WaveStyle::Colour) = (scene.loop_span, style()) {
        // The loop green is close to the mid band green: a top bar and a hatch set it apart.
        let (a, b) = (
            x0.clamp(0.0, f64::from(w)) as f32,
            x1.clamp(0.0, f64::from(w)) as f32,
        );
        let edge = if looping {
            om_ui::waveform::LOOP_EDGE_ALPHA
        } else {
            om_ui::waveform::LOOP_IDLE_EDGE_ALPHA
        };
        quad(
            window,
            bounds,
            a,
            0.0,
            b - a,
            om_ui::waveform::LOOP_TOP_BAR_HEIGHT,
            with_alpha(LOOP_COLOUR, edge),
        );
        paint_hatch(
            window,
            bounds,
            (a, b),
            h,
            with_alpha(LOOP_COLOUR, om_ui::waveform::LOOP_HATCH_ALPHA),
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
                    entry.layers = column_layers(&columns, bounds.origin, h, &palette, spectral);
                    entry.key = Some(key);
                }
                paint_layers(window, bounds, &entry.layers, &palette, m.playhead_x as f32);
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

#[cfg(test)]
mod tests {
    use super::*;
    use om_analysis::{demo_loop, AudioClip, NullAnalysisContext, WaveformBuilder};

    fn demo_columns(count: usize) -> Vec<(f32, ColumnShape)> {
        let clip = AudioClip::new(demo_loop(64.0, 124.0, 44_100.0), 44_100.0);
        let data = WaveformBuilder::default()
            .build(&clip, &NullAnalysisContext)
            .expect("not cancelled");
        let points = &data.overview().points;
        let shapes = overview_shapes(points, count, Reference::of(points), true);
        shapes
            .into_iter()
            .enumerate()
            .map(|(i, s)| (i as f32 * 2.0 + 1.0, s))
            .collect()
    }

    fn palette(style: WaveStyle) -> Palette {
        Palette::new(style, om_ui::theme::ThemeTokens::builtin_dark().well)
    }

    #[test]
    fn colour_styles_draw_one_quad_per_column_and_three_band_draws_three_paths() {
        let columns = demo_columns(300);
        let origin = point(px(0.0), px(0.0));
        for style in [WaveStyle::Colour, WaveStyle::Mono, WaveStyle::ColourBlind] {
            let layers = column_layers(&columns, origin, 48.0, &palette(style), true);
            // One translucent peak outline, then one quad per column.
            let paths = layers
                .iter()
                .filter(|l| matches!(l, Layer::Path(..)))
                .count();
            let quads = layers.len() - paths;
            assert_eq!(paths, 1, "{style:?}");
            assert!(quads <= columns.len(), "{style:?}");
            assert!(quads > columns.len() / 2, "{style:?}");
        }
        let three = column_layers(&columns, origin, 48.0, &palette(WaveStyle::ThreeBand), true);
        assert_eq!(three.len(), 3);
        let grey = column_layers(&columns, origin, 48.0, &palette(WaveStyle::Colour), false);
        assert_eq!(grey.len(), 1, "no spectrum: one silhouette");
    }

    #[test]
    fn neighbouring_quads_tile_with_no_gap() {
        let columns = demo_columns(300);
        let layers = column_layers(
            &columns,
            point(px(0.0), px(0.0)),
            48.0,
            &palette(WaveStyle::Colour),
            true,
        );
        let quads: Vec<[f32; 4]> = layers
            .iter()
            .filter_map(|l| match l {
                Layer::Quad(r, _) => Some(*r),
                Layer::Path(..) => None,
            })
            .collect();
        for pair in quads.windows(2) {
            assert!(pair[0][0] + pair[0][2] <= pair[1][0] + 1e-3, "overlap");
        }
    }

    #[test]
    fn a_style_change_invalidates_the_overview() {
        let bounds = [0.0, 0.0, 600.0, 48.0];
        let base = overview_key(7, bounds, WaveStyle::Colour);
        assert_eq!(base, overview_key(7, bounds, WaveStyle::Colour));
        assert_ne!(base, overview_key(7, bounds, WaveStyle::Mono));
        assert_ne!(base, overview_key(8, bounds, WaveStyle::Colour));
        assert_ne!(
            base,
            overview_key(7, [0.0, 0.0, 601.0, 48.0], WaveStyle::Colour)
        );
    }
}
