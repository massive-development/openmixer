//! Painting the waveforms: thin code over `om_ui::waveform`, which decides every colour, height
//! and position. A strip is a handful of filled shapes: the Low-Band style is two (the body in
//! the deck colour, the high band in the text colour on top), the three-band style three nested
//! ones; the Colour, Mono and colour-blind styles draw the peak outline and a quad per column. The
//! shapes come from a strip cached across frames; the overview's are cached whole and cloned each
//! frame. Scrolling the zoomed strip rebuilds only the shapes from the cached heights.
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
    ColumnShape, OverviewMarkers, Palette, Reference, StripInk, StripSlot, StripSource, View,
    WaveStyle, BAR_LINE_WIDTH, BEAT_LINE_WIDTH, LOOP_EDGE_WIDTH, LOW_BAND_HIGH_ALPHA,
    OVERVIEW_CUE_FLAG, OVERVIEW_PLAYED_SHADE_ALPHA, OVERVIEW_TICK_ALPHA, OVERVIEW_TICK_HEIGHT,
    PLAYHEAD_HALO,
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

/// The style waveforms are drawn in: [`set_style`], else `OPENMIXER_WAVE_STYLE`, else Low-Band.
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

/// What an overview's cached shapes were built for. A different style or palette (a theme switch
/// changes the deck and text colours) gives a different key.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverviewKey {
    track: usize,
    bounds: [f32; 4],
    style: WaveStyle,
    colours: [Colour; 5],
}

pub fn overview_key(track: usize, bounds: [f32; 4], palette: &Palette) -> OverviewKey {
    let p = palette;
    OverviewKey {
        track,
        bounds,
        style: p.style,
        colours: [p.well, p.low, p.mid, p.high, p.silhouette],
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

/// One closed shape mirrored about `mid` through every column, `half` px from the centre each,
/// built straight from the columns (no intermediate list). `None` when nothing has height.
fn envelope_path(
    columns: &[(f32, ColumnShape)],
    origin: Point<Pixels>,
    mid: f32,
    half: impl Fn(&ColumnShape) -> f32,
) -> Option<Path<Pixels>> {
    if columns.len() < 2 || columns.iter().all(|(_, c)| half(c) <= 0.0) {
        return None;
    }
    let at = |x: f32, y: f32| point(origin.x + px(x), origin.y + px(y));
    let mut builder = PathBuilder::fill();
    let (x0, c0) = &columns[0];
    builder.move_to(at(*x0, mid - half(c0)));
    for (x, c) in &columns[1..] {
        builder.line_to(at(*x, mid - half(c)));
    }
    for (x, c) in columns.iter().rev() {
        builder.line_to(at(*x, mid + half(c)));
    }
    builder.close();
    builder.build().ok()
}

/// The Low-Band layers: the body in the deck colour, then the high band in the text colour at
/// [`LOW_BAND_HIGH_ALPHA`] on top.
fn low_band_layers(
    columns: &[(f32, ColumnShape)],
    origin: Point<Pixels>,
    height: f32,
    palette: &Palette,
) -> Vec<Layer> {
    let mid = height / 2.0;
    let body = envelope_path(columns, origin, mid, |c| shape_px(c.low_band().0, height));
    let highs = envelope_path(columns, origin, mid, |c| shape_px(c.low_band().1, height));
    body.map(|p| Layer::Path(palette.low, 1.0, p))
        .into_iter()
        .chain(highs.map(|p| Layer::Path(palette.high, LOW_BAND_HIGH_ALPHA, p)))
        .collect()
}

/// One thing a strip draws: a filled path (colour, alpha) or a column quad (x, y, width, height
/// relative to the strip, and its colour). Colours are the unplayed ones; the played part is
/// derived when painting ([`Palette::played`]).
enum Layer {
    Path(Colour, f32, Path<Pixels>),
    Quad([f32; 4], Colour),
}

/// The layers of a strip, back to front. The Low-Band style draws two paths. Colour, mono and
/// colour-blind styles draw one quad per column, tiled on whole pixels, each in its own colour
/// from the column's band weights: colour is never quantised into shared buckets, because a bucket
/// boundary between neighbouring columns shows as flicker. The three-band style draws three nested
/// paths; a waveform without spectrum is one grey silhouette path.
fn column_layers(
    columns: &[(f32, ColumnShape)],
    origin: Point<Pixels>,
    height: f32,
    palette: &Palette,
    spectral: bool,
) -> Vec<Layer> {
    if spectral && palette.style == WaveStyle::LowBand {
        return low_band_layers(columns, origin, height, palette);
    }
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

/// The playhead, `ink.playhead_width` px of signal red, with the earlier styles' halo.
fn playhead(window: &mut Window, bounds: Bounds<Pixels>, x: f64, height: f32, ink: &StripInk) {
    let (left, width) = playhead_rect(x, ink.playhead_width);
    if let Some(halo) = ink.halo {
        let c = paint(halo);
        quad(
            window,
            bounds,
            left - PLAYHEAD_HALO,
            0.0,
            PLAYHEAD_HALO,
            height,
            c,
        );
        quad(window, bounds, left + width, 0.0, PLAYHEAD_HALO, height, c);
    }
    quad(
        window,
        bounds,
        left,
        0.0,
        width,
        height,
        paint(ink.playhead),
    );
}

/// A loop's 1 px edges at `a` and `b`.
fn loop_edges(window: &mut Window, bounds: Bounds<Pixels>, (a, b): (f32, f32), h: f32, c: Rgba) {
    for x in [a, b] {
        let left = x - LOOP_EDGE_WIDTH / 2.0;
        quad(window, bounds, left, 0.0, LOOP_EDGE_WIDTH, h, c);
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
    pub ink: StripInk,
}

pub fn paint_zoom(window: &mut Window, bounds: Bounds<Pixels>, scene: &ZoomScene) {
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);
    let view = &scene.view;
    let ink = &scene.ink;
    let playhead_x = view.width * 0.5;
    let span = scene
        .loop_span
        .map(|(x0, x1, looping)| ((x0 as f32, x1 as f32), looping));

    if let Some(((a, b), looping)) = span {
        let (a, b) = (a.clamp(0.0, w), b.clamp(0.0, w));
        let fill = paint(ink.loop_fill(looping));
        quad(window, bounds, a, 0.0, b - a, h, fill);
    }

    if let Some(data) = &scene.wave {
        paint_zoom_wave(window, bounds, scene, data, playhead_x);
    }

    for line in &scene.grid {
        let width = if line.bar {
            BAR_LINE_WIDTH
        } else {
            BEAT_LINE_WIDTH
        };
        let c = ink.grid(line.bar);
        let left = line.x as f32 - width / 2.0;
        quad(window, bounds, left, 0.0, width, h, paint(c));
    }

    if let Some((edges, looping)) = span {
        let c = ink.loop_edge(looping);
        loop_edges(window, bounds, edges, h, paint(c));
    }

    playhead(window, bounds, playhead_x, h, ink);
}

/// The zoomed strip's waveform. The earlier styles dim the played part left of the playhead; the
/// Low-Band style does not (comp 4a: the centred playhead already splits the strip).
fn paint_zoom_wave(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    scene: &ZoomScene,
    data: &Arc<WaveformData>,
    playhead_x: f64,
) {
    let view = &scene.view;
    let h = f32::from(bounds.size.height);
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
    let palette = scene.ink.palette();
    let layers = column_layers(&columns, bounds.origin, h, &palette, spectral);
    let edge = match scene.ink.played_shade {
        Some(_) => 0.0,
        None => played_edge(view, playhead_x) as f32,
    };
    paint_layers(window, bounds, &layers, &palette, edge);
}

/// Everything the overview draws.
pub struct OverviewScene {
    pub deck: usize,
    pub wave: Option<Arc<WaveformData>>,
    pub markers: OverviewMarkers,
    pub ink: StripInk,
}

pub fn paint_overview(window: &mut Window, bounds: Bounds<Pixels>, scene: &OverviewScene) {
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);
    let m = &scene.markers;
    let ink = &scene.ink;

    if let Some(data) = &scene.wave {
        paint_overview_wave(window, bounds, scene, data);
    }

    // Low-Band: the played part under the ground colour (comp 4a).
    if let Some(shade) = ink.played_shade {
        let x = (m.playhead_x as f32).clamp(0.0, w);
        let c = with_alpha(shade, OVERVIEW_PLAYED_SHADE_ALPHA);
        quad(window, bounds, 0.0, 0.0, x, h, c);
    }

    if let Some((a, b, looping)) = m.loop_span {
        let (a, b) = (a as f32, (b as f32).max(a as f32 + 1.0));
        let fill = ink.loop_fill(looping);
        quad(window, bounds, a, 0.0, b - a, h, paint(fill));
        let edge = ink.loop_edge(looping);
        loop_edges(window, bounds, (a, b), h, paint(edge));
    }

    // Bar ticks along both edges.
    let tick = with_alpha(ink.ink, OVERVIEW_TICK_ALPHA);
    for x in &m.bar_ticks {
        let x = *x as f32;
        quad(window, bounds, x, 0.0, 1.0, OVERVIEW_TICK_HEIGHT, tick);
        let bottom = h - OVERVIEW_TICK_HEIGHT;
        quad(window, bounds, x, bottom, 1.0, OVERVIEW_TICK_HEIGHT, tick);
    }

    // Hot cues: the deck-colour guide and a square flag on top, as in the zoomed strip.
    let guide = ink.cue_guide();
    let (guide, flag) = (paint(guide), paint(ink.deck));
    for (x, _) in &m.cues {
        let x = *x as f32;
        quad(window, bounds, x, 0.0, ink.cue_width, h, guide);
        quad(
            window,
            bounds,
            x,
            0.0,
            OVERVIEW_CUE_FLAG,
            OVERVIEW_CUE_FLAG,
            flag,
        );
    }

    playhead(window, bounds, m.playhead_x, h, ink);
}

/// The overview's waveform, its shapes cached per deck until the track, size or palette changes.
fn paint_overview_wave(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    scene: &OverviewScene,
    data: &Arc<WaveformData>,
) {
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);
    let points = &data.overview().points;
    if points.is_empty() || w < 2.0 {
        return;
    }
    let palette = scene.ink.palette();
    let origin = [f32::from(bounds.origin.x), f32::from(bounds.origin.y), w, h];
    let key = overview_key(track_id(data), origin, &palette);
    let played_x = match scene.ink.played_shade {
        Some(_) => 0.0,
        None => scene.markers.playhead_x as f32,
    };
    OVERVIEWS.with(|cache| {
        let mut cache = cache.borrow_mut();
        let entry = cache.entry(scene.deck).or_default();
        if entry.key != Some(key) {
            let spectral = data.has_spectrum();
            let count = (w / 2.0).ceil() as usize;
            let step = w / count as f32;
            let shapes = overview_shapes(points, count, reference_for(scene.deck, data), spectral);
            let columns: Vec<(f32, ColumnShape)> = shapes
                .into_iter()
                .enumerate()
                .map(|(i, shape)| ((i as f32 + 0.5) * step, shape))
                .collect();
            entry.layers = column_layers(&columns, bounds.origin, h, &palette, spectral);
            entry.key = Some(key);
        }
        paint_layers(window, bounds, &entry.layers, &palette, played_x);
    });
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
    fn low_band_draws_the_body_in_the_deck_colour_and_the_highs_on_top() {
        let columns = demo_columns(300);
        let t = om_ui::theme::ThemeTokens::builtin_dark();
        let p = StripInk::new(&t, WaveStyle::LowBand, 0).palette();
        let layers = column_layers(&columns, point(px(0.0), px(0.0)), 48.0, &p, true);
        let drawn: Vec<(Colour, f32)> = layers
            .iter()
            .filter_map(|l| match l {
                Layer::Path(c, a, _) => Some((*c, *a)),
                Layer::Quad(..) => None,
            })
            .collect();
        assert_eq!(drawn.len(), layers.len(), "paths only");
        assert_eq!(
            drawn,
            [
                (t.accent_for(0), 1.0),
                (t.text_primary, LOW_BAND_HIGH_ALPHA)
            ]
        );
        let grey = column_layers(&columns, point(px(0.0), px(0.0)), 48.0, &p, false);
        assert_eq!(grey.len(), 1, "no spectrum: one silhouette");
    }

    #[test]
    fn a_style_or_theme_change_invalidates_the_overview() {
        let bounds = [0.0, 0.0, 600.0, 48.0];
        let dark = om_ui::theme::ThemeTokens::builtin_dark();
        let p = |style| StripInk::new(&dark, style, 0).palette();
        let base = overview_key(7, bounds, &p(WaveStyle::Colour));
        assert_eq!(base, overview_key(7, bounds, &p(WaveStyle::Colour)));
        assert_ne!(base, overview_key(7, bounds, &p(WaveStyle::Mono)));
        assert_ne!(base, overview_key(8, bounds, &p(WaveStyle::Colour)));
        assert_ne!(
            base,
            overview_key(7, [0.0, 0.0, 601.0, 48.0], &p(WaveStyle::Colour))
        );
        let low = overview_key(7, bounds, &p(WaveStyle::LowBand));
        let other_deck = StripInk::new(&dark, WaveStyle::LowBand, 1).palette();
        assert_ne!(low, overview_key(7, bounds, &other_deck), "deck colour");
        let mut light = dark.clone();
        light.text_primary = light.well;
        let relit = StripInk::new(&light, WaveStyle::LowBand, 0).palette();
        assert_ne!(low, overview_key(7, bounds, &relit), "theme colours");
    }
}
