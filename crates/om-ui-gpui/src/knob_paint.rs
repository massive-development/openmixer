//! Rotary knob maths and painting. The maths is plain functions over `f64`/`f32` so it runs
//! headless; painting is a thin layer over it.
//!
//! Angles are degrees clockwise from 12 o'clock. The sweep is 270 degrees (spec: `docs/design/
//! research/knobs.md` section 3), leaving a 90 degree gap at the bottom.

use crate::paint::paint;
use gpui::{
    point, px, quad, size, BorderStyle, Bounds, Path, PathBuilder, Pixels, Point, Rgba, Window,
};
use om_ui::style::LookAndFeel;
use om_ui::theme::{Colour, ThemeTokens};

/// Angle of the minimum, degrees clockwise from 12 o'clock (7:30).
pub const SWEEP_START: f64 = -135.0;
/// Angle of the maximum (4:30).
pub const SWEEP_END: f64 = 135.0;
/// Share of the lower half of a bipolar sweep drawn in the danger colour (spec section 3: "the
/// bottom 5% of the EQ sweep"; the plan measures it from the minimum towards the detent).
pub const KILL_FRACTION: f64 = 0.05;

/// The detent, when it lies strictly inside the range and so can split the sweep.
fn usable_detent(min: f64, detent: Option<f64>, max: f64) -> Option<f64> {
    detent.filter(|d| *d > min && *d < max)
}

/// Pointer angle for `value` on a knob over `min..=max`. With a detent inside the range, each side
/// of it takes half the sweep, so the pointer sits at 12 o'clock on the detent; otherwise the map
/// is linear. Values outside the range clamp.
pub fn angle_of(value: f64, min: f64, detent: Option<f64>, max: f64) -> f64 {
    SWEEP_START + (SWEEP_END - SWEEP_START) * sweep_fraction(value, min, detent, max)
}

/// Where `value` sits along the sweep, `0..=1` (0.5 on a detent). Dragging works on this, not on
/// the raw value, so the pointer follows the mouse whatever the range.
pub fn sweep_fraction(value: f64, min: f64, detent: Option<f64>, max: f64) -> f64 {
    if max <= min {
        return 0.0;
    }
    let v = value.clamp(min, max);
    match usable_detent(min, detent, max) {
        Some(d) if v <= d => 0.5 * (v - min) / (d - min),
        Some(d) => 0.5 + 0.5 * (v - d) / (max - d),
        None => (v - min) / (max - min),
    }
}

/// Inverse of [`sweep_fraction`]; fractions outside `0..=1` clamp.
pub fn value_of_fraction(fraction: f64, min: f64, detent: Option<f64>, max: f64) -> f64 {
    let f = fraction.clamp(0.0, 1.0);
    match usable_detent(min, detent, max) {
        Some(d) if f <= 0.5 => min + (d - min) * f / 0.5,
        Some(d) => d + (max - d) * (f - 0.5) / 0.5,
        None => min + (max - min) * f,
    }
}

/// Inverse of [`angle_of`]: the value a pointer at `angle` stands for. Angles clamp to the sweep.
pub fn value_of_angle(angle: f64, min: f64, detent: Option<f64>, max: f64) -> f64 {
    let f = (angle - SWEEP_START) / (SWEEP_END - SWEEP_START);
    value_of_fraction(f, min, detent, max)
}

/// True when `value` is in the kill zone of a knob that has one (the EQ): within
/// [`KILL_FRACTION`] of the way from the minimum to the detent. Knobs without a kill, or without
/// a usable detent, never are.
pub fn in_kill_zone(value: f64, min: f64, detent: Option<f64>, has_kill: bool) -> bool {
    match usable_detent(min, detent, f64::INFINITY) {
        Some(d) if has_kill => value <= min + KILL_FRACTION * (d - min),
        _ => false,
    }
}

/// Pixels of vertical drag that span a knob's whole sweep.
pub const KNOB_TRAVEL_PX: f64 = 200.0;
/// Fine mode (shift) divides travel by this.
pub const FINE_DIVISOR: f64 = 10.0;

/// The value after dragging `dy_px` (positive: pointer moved up) from `anchor_value`. The travel
/// is applied to the sweep fraction, so the pointer turns the same angle per pixel whatever the
/// range; `fine` makes it [`FINE_DIVISOR`] times slower.
pub fn dragged_value(
    anchor_value: f64,
    dy_px: f64,
    fine: bool,
    min: f64,
    detent: Option<f64>,
    max: f64,
) -> f64 {
    let travel = dy_px / KNOB_TRAVEL_PX / if fine { FINE_DIVISOR } else { 1.0 };
    let fraction = sweep_fraction(anchor_value, min, detent, max) + travel;
    value_of_fraction(fraction, min, detent, max)
}

/// `value` with the knob's detent pull applied, unless `shift` is held (fine mode bypasses it).
pub fn snap_with_bypass(knob: &om_ui::control::Knob, value: f64, shift: bool) -> f64 {
    if shift {
        value
    } else {
        knob.snap_to_detent(value)
    }
}

/// Which control a knob is, for its largest cap and where its label goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KnobRole {
    /// Trim and filter: a wide strip-width bar.
    Wide,
    /// One of the three EQ cells.
    #[default]
    Band,
    /// Cue mix, phones level and master.
    Cue,
    /// FX parameter and wet/dry.
    Fx,
    /// Sampler gain and pitch.
    Sampler,
    /// The deck stem filter.
    Stem,
}

/// Gap between cap edge and track arc, px (spec section 3).
pub const TRACK_GAP: f32 = 3.0;
/// Scale ticks are drawn only on caps at least this wide, px (spec section 3).
pub const TICKS_MIN_CAP: f32 = 32.0;
/// How far above a control its value popup floats, px.
pub const POPUP_LIFT: f32 = 26.0;
/// Arc stroke on caps above [`THIN_STROKE_MAX_CAP`], px (spec section 3).
pub const THICK_STROKE: f32 = 2.5;
/// Arc stroke on caps of [`THIN_STROKE_MAX_CAP`] or less, px.
pub const THIN_STROKE: f32 = 2.0;
/// Largest cap that takes the thin stroke, px.
pub const THIN_STROKE_MAX_CAP: f32 = 28.0;
/// Clearance between a popup flipped below its knob and the track, px. Unsourced.
const POPUP_FLIP_GAP: f32 = 2.0;

impl KnobRole {
    /// Largest cap diameter, px (spec section 3).
    pub fn max_cap(self) -> f32 {
        match self {
            KnobRole::Wide | KnobRole::Band => 32.0,
            KnobRole::Cue => 36.0,
            KnobRole::Fx => 30.0,
            KnobRole::Sampler => 28.0,
            KnobRole::Stem => 24.0,
        }
    }

    /// Whether the label is drawn (the stem knob shows its value on hover only).
    pub fn shows_label(self) -> bool {
        self != KnobRole::Stem
    }
}

/// Where a knob sits inside its rect, in the rect's own coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KnobGeom {
    pub cx: f32,
    pub cy: f32,
    /// Cap radius.
    pub r_cap: f32,
    /// Track arc radius (centre line of the stroke).
    pub r_track: f32,
    /// Track and value arc stroke width.
    pub stroke: f32,
}

/// Fits a knob into `rect_w` x `rect_h`, leaving `label_h` below for a label.
pub fn knob_geometry(rect_w: f32, rect_h: f32, label_h: f32, role: KnobRole) -> KnobGeom {
    let avail = rect_w.min(rect_h - label_h).max(1.0);
    // 28 px caps and smaller take the thinner stroke, which changes the room the track needs.
    let wide_stroke_cap = avail - 2.0 * (TRACK_GAP + THICK_STROKE / 2.0);
    let stroke = if wide_stroke_cap.min(role.max_cap()) <= THIN_STROKE_MAX_CAP {
        THIN_STROKE
    } else {
        THICK_STROKE
    };
    let r_cap = ((avail - 2.0 * (TRACK_GAP + stroke / 2.0)).min(role.max_cap()) / 2.0).max(1.0);
    KnobGeom {
        cx: rect_w / 2.0,
        cy: (rect_h - label_h) / 2.0,
        r_cap,
        r_track: r_cap + TRACK_GAP,
        stroke,
    }
}

impl KnobGeom {
    /// The square that bounds the track arc: `(x, y, side)`.
    pub fn bbox(&self) -> (f32, f32, f32) {
        let r = self.hit_radius();
        (self.cx - r, self.cy - r, 2.0 * r)
    }

    /// True when `(x, y)` is on the knob: anywhere in the disc inside the track, gap included.
    pub fn hits(&self, x: f32, y: f32) -> bool {
        let (dx, dy) = (x - self.cx, y - self.cy);
        dx * dx + dy * dy <= self.hit_radius() * self.hit_radius()
    }

    /// Outer radius of the track stroke: the knob's reach.
    fn hit_radius(&self) -> f32 {
        self.r_track + self.stroke / 2.0
    }

    /// Cap diameter.
    pub fn cap_diameter(&self) -> f32 {
        2.0 * self.r_cap
    }

    /// Scale ticks are drawn only on large caps.
    pub fn shows_ticks(&self) -> bool {
        self.cap_diameter().round() >= TICKS_MIN_CAP
    }

    /// Popup top in the rect's coordinates. It floats [`POPUP_LIFT`] above the track; when that
    /// would leave the card (`rect_y` is the rect's top in the card), it goes below the knob.
    pub fn popup_top(&self, rect_y: f32) -> f32 {
        let above = self.cy - self.r_track - POPUP_LIFT;
        if rect_y + above < 0.0 {
            self.cy + self.r_track + POPUP_FLIP_GAP
        } else {
            above
        }
    }
}

/// Longest arc one `arc_to` call draws, degrees. Under 180 so the large-arc flag is never
/// ambiguous.
pub const MAX_ARC_SEGMENT: f64 = 170.0;

/// Splits the arc from `a0` to `a1` (degrees clockwise from 12 o'clock) into equal pieces, each at
/// most [`MAX_ARC_SEGMENT`] long, as increasing `(start, end)` pairs. A zero-length arc yields
/// none; reversed ends are put in order.
pub fn arc_segments(a0: f64, a1: f64) -> Vec<(f64, f64)> {
    let (a0, a1) = if a0 <= a1 { (a0, a1) } else { (a1, a0) };
    let span = a1 - a0;
    if span < 1e-9 {
        return Vec::new();
    }
    let n = (span / MAX_ARC_SEGMENT).ceil().max(1.0);
    // 180 degrees must split too: `ceil` alone would leave it whole, at the large-arc boundary.
    let n = if span >= 180.0 { n.max(2.0) } else { n };
    let step = span / n;
    (0..n as usize)
        .map(|i| {
            let start = a0 + step * i as f64;
            let end = if i + 1 == n as usize {
                a1
            } else {
                start + step
            };
            (start, end)
        })
        .collect()
}

/// The point at `angle` degrees clockwise from 12 o'clock on the circle of radius `r` about
/// `(cx, cy)`, in y-down screen coordinates.
pub fn point_on(cx: f32, cy: f32, r: f32, angle: f64) -> (f32, f32) {
    let rad = angle.to_radians();
    (cx + r * rad.sin() as f32, cy - r * rad.cos() as f32)
}

// ---- Painting ----------------------------------------------------------------------------

/// Colours one knob is painted with, all from the theme (`LookAndFeel`/`ThemeTokens`).
#[derive(Debug, Clone, Copy)]
pub struct KnobColours {
    pub track: Colour,
    /// The value arc: the channel accent.
    pub value: Colour,
    /// Arc and pointer in the kill zone.
    pub kill: Colour,
    pub cap: Colour,
    pub cap_hover: Colour,
    pub cap_border: Colour,
    pub pointer: Colour,
    /// Detent tick and scale ticks.
    pub mark: Colour,
}

impl KnobColours {
    pub fn new(look: &LookAndFeel, t: &ThemeTokens, accent: Colour) -> Self {
        Self {
            track: look.knob.track,
            value: accent,
            kill: look.knob.kill,
            cap: look.knob.body.fill,
            cap_hover: look.button.hover.fill,
            cap_border: look.knob.body.border,
            pointer: look.knob.pointer,
            mark: t.text_secondary,
        }
    }
}

/// What a knob looks like right now.
#[derive(Debug, Clone, Copy)]
pub struct KnobPaintState {
    pub geom: KnobGeom,
    /// Pointer angle, from [`angle_of`].
    pub angle: f64,
    /// The detent's angle when the knob has one inside its range.
    pub detent_angle: Option<f64>,
    /// The value arc runs from the detent instead of the minimum.
    pub bipolar: bool,
    /// In the kill zone: arc and pointer in the kill colour.
    pub killed: bool,
    pub hovered: bool,
    /// Being dragged: a slightly heavier arc.
    pub active: bool,
    pub disabled: bool,
}

/// Extra arc stroke while dragged, px (spec section 3).
const ACTIVE_STROKE_EXTRA: f32 = 0.5;
/// Weight of the pointer colour mixed into the value arc on hover: the spec's "+10% lightness".
const HOVER_LIFT: f32 = 0.1;
/// Pointer line: from this share of the cap radius to [`POINTER_OUTER`], px wide.
const POINTER_INNER: f32 = 0.35;
const POINTER_OUTER: f32 = 0.85;
const POINTER_WIDTH: f32 = 2.0;
/// Width of the detent and scale marks, px.
const MARK_WIDTH: f32 = 2.0;
/// Diameter of the dot a bipolar knob shows at rest, px.
const REST_DOT: f32 = 2.0;

fn px_point(origin: Point<Pixels>, p: (f32, f32)) -> Point<Pixels> {
    point(origin.x + px(p.0), origin.y + px(p.1))
}

/// A stroked arc of the circle about `(cx, cy)` from `a0` to `a1` degrees, in the box at `origin`.
/// None for an empty arc.
fn arc_path(
    origin: Point<Pixels>,
    (cx, cy): (f32, f32),
    r: f32,
    (a0, a1): (f64, f64),
    stroke: f32,
) -> Option<Path<Pixels>> {
    let segments = arc_segments(a0, a1);
    let (first, _) = *segments.first()?;
    let mut builder = PathBuilder::stroke(px(stroke));
    builder.move_to(px_point(origin, point_on(cx, cy, r, first)));
    for (_, end) in segments {
        builder.arc_to(
            point(px(r), px(r)),
            px(0.0),
            false,
            true,
            px_point(origin, point_on(cx, cy, r, end)),
        );
    }
    builder.build().ok()
}

/// A straight stroked line between two points of the box at `origin`.
fn line_path(
    origin: Point<Pixels>,
    from: (f32, f32),
    to: (f32, f32),
    width: f32,
) -> Option<Path<Pixels>> {
    let mut builder = PathBuilder::stroke(px(width));
    builder.move_to(px_point(origin, from));
    builder.line_to(px_point(origin, to));
    builder.build().ok()
}

/// A filled disc of `diameter` centred on `(x, y)`; with `border`, a 1 px ring of that colour.
fn disc(
    window: &mut Window,
    origin: Point<Pixels>,
    (x, y): (f32, f32),
    diameter: f32,
    fill: Rgba,
    border: Option<Rgba>,
) {
    let bounds = Bounds::new(
        px_point(origin, (x - diameter / 2.0, y - diameter / 2.0)),
        size(px(diameter), px(diameter)),
    );
    let ring = border.map_or(0.0, |_| 1.0);
    window.paint_quad(quad(
        bounds,
        px(diameter / 2.0),
        fill,
        px(ring),
        border.unwrap_or(fill),
        BorderStyle::Solid,
    ));
}

/// Paints a knob into the box whose top-left is `origin` (the geometry's coordinates are relative
/// to it).
///
/// gpui 0.2.2 strokes have butt caps and `PathBuilder` offers no way to change that without a
/// direct `lyon` dependency, so arc ends are capped with small discs.
pub fn paint_knob(
    window: &mut Window,
    origin: Point<Pixels>,
    state: &KnobPaintState,
    colours: &KnobColours,
) {
    let g = &state.geom;
    let fade = if state.disabled { 0.4 } else { 1.0 };
    let col = |c: Colour| paint(c.with_multiplied_alpha(fade));
    let stroke = g.stroke
        + if state.active {
            ACTIVE_STROKE_EXTRA
        } else {
            0.0
        };
    let centre = (g.cx, g.cy);

    let value_colour = if state.killed {
        colours.kill
    } else if state.hovered {
        Colour::mix(&[
            (colours.value, 1.0 - HOVER_LIFT),
            (colours.pointer, HOVER_LIFT),
        ])
    } else {
        colours.value
    };

    // Track, then the value arc over it. Both end in discs for round caps.
    let (track_a, value_a) = (colours.track, value_colour);
    if let Some(path) = arc_path(
        origin,
        centre,
        g.r_track,
        (SWEEP_START, SWEEP_END),
        g.stroke,
    ) {
        window.paint_path(path, col(track_a));
    }
    for angle in [SWEEP_START, SWEEP_END] {
        let p = point_on(g.cx, g.cy, g.r_track, angle);
        disc(window, origin, p, g.stroke, col(track_a), None);
    }
    let (from, to) = if state.bipolar {
        (state.detent_angle.unwrap_or(0.0), state.angle)
    } else {
        (SWEEP_START, state.angle)
    };
    if (to - from).abs() > 0.5 {
        if let Some(path) = arc_path(origin, centre, g.r_track, (from, to), stroke) {
            window.paint_path(path, col(value_a));
        }
        for angle in [from, to] {
            let p = point_on(g.cx, g.cy, g.r_track, angle);
            disc(window, origin, p, stroke, col(value_a), None);
        }
    } else if state.bipolar {
        // At rest on the detent: a dot where the arc would start.
        let p = point_on(g.cx, g.cy, g.r_track, state.detent_angle.unwrap_or(0.0));
        disc(window, origin, p, REST_DOT, col(colours.mark), None);
    }

    // Detent and scale marks sit in the gap between cap and track so they stay inside the box.
    let inner = g.r_cap + 0.5;
    let outer = (g.r_track - g.stroke / 2.0).max(inner + 0.5);
    let mut mark_angles: Vec<f64> = Vec::new();
    if g.shows_ticks() {
        mark_angles.extend([SWEEP_START, SWEEP_END]);
    }
    mark_angles.extend(state.detent_angle);
    for angle in mark_angles {
        let (a, b) = (
            point_on(g.cx, g.cy, inner, angle),
            point_on(g.cx, g.cy, outer, angle),
        );
        if let Some(path) = line_path(origin, a, b, MARK_WIDTH) {
            window.paint_path(path, col(colours.mark));
        }
    }

    // Cap.
    let cap_fill = if state.hovered {
        colours.cap_hover
    } else {
        colours.cap
    };
    disc(
        window,
        origin,
        centre,
        2.0 * g.r_cap,
        col(cap_fill),
        Some(col(colours.cap_border)),
    );

    // Pointer.
    let pointer = if state.killed {
        colours.kill
    } else {
        colours.pointer
    };
    let (a, b) = (
        point_on(g.cx, g.cy, g.r_cap * POINTER_INNER, state.angle),
        point_on(g.cx, g.cy, g.r_cap * POINTER_OUTER, state.angle),
    );
    if let Some(path) = line_path(origin, a, b, POINTER_WIDTH) {
        window.paint_path(path, col(pointer));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EQ: (f64, Option<f64>, f64) = (-48.0, Some(0.0), 6.0);
    const TRIM: (f64, Option<f64>, f64) = (0.0, Some(1.0), 4.0);
    const FILTER: (f64, Option<f64>, f64) = (-1.0, Some(0.0), 1.0);

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn unipolar_ends_hit_sweep_ends() {
        assert!(near(angle_of(0.0, 0.0, None, 1.0), -135.0));
        assert!(near(angle_of(1.0, 0.0, None, 1.0), 135.0));
        assert!(near(angle_of(0.5, 0.0, None, 1.0), 0.0));
    }

    #[test]
    fn eq_detent_sits_at_twelve_oclock() {
        let (min, d, max) = EQ;
        assert!(near(angle_of(0.0, min, d, max), 0.0));
        assert!(near(angle_of(-48.0, min, d, max), -135.0));
        assert!(near(angle_of(6.0, min, d, max), 135.0));
        assert!(near(angle_of(-24.0, min, d, max), -67.5));
        assert!(near(angle_of(3.0, min, d, max), 67.5));
    }

    #[test]
    fn trim_unity_at_twelve() {
        let (min, d, max) = TRIM;
        assert!(near(angle_of(1.0, min, d, max), 0.0));
        assert!(near(angle_of(0.0, min, d, max), -135.0));
        assert!(near(angle_of(4.0, min, d, max), 135.0));
    }

    #[test]
    fn filter_is_symmetric() {
        let (min, d, max) = FILTER;
        assert!(near(angle_of(0.5, min, d, max), 67.5));
        assert!(near(angle_of(-0.5, min, d, max), -67.5));
    }

    #[test]
    fn round_trip() {
        for (min, d, max) in [EQ, TRIM, FILTER] {
            for i in 0..200 {
                let v = min + (max - min) * f64::from(i) / 199.0;
                let back = value_of_angle(angle_of(v, min, d, max), min, d, max);
                assert!((back - v).abs() < 1e-9, "{v} -> {back}");
            }
        }
    }

    #[test]
    fn out_of_range_clamps() {
        let (min, d, max) = EQ;
        assert!(near(angle_of(100.0, min, d, max), 135.0));
        assert!(near(angle_of(-100.0, min, d, max), -135.0));
        assert!(near(value_of_angle(400.0, min, d, max), 6.0));
        assert!(near(value_of_angle(-400.0, min, d, max), -48.0));
    }

    #[test]
    fn degenerate_detent_falls_back_to_linear() {
        // A detent on the minimum (or the maximum, or outside) cannot split the sweep.
        for d in [0.0, 1.0, -3.0, 9.0] {
            assert!(near(angle_of(0.5, 0.0, Some(d), 1.0), 0.0), "detent {d}");
            assert!(near(angle_of(0.0, 0.0, Some(d), 1.0), -135.0));
            assert!(near(angle_of(1.0, 0.0, Some(d), 1.0), 135.0));
        }
        assert!(near(angle_of(0.5, 1.0, None, 1.0), -135.0), "empty range");
    }

    #[test]
    fn kill_zone() {
        let (min, d, max) = EQ;
        assert!(in_kill_zone(-48.0, min, d, true));
        assert!(in_kill_zone(-46.0, min, d, true), "within 5% of 48 = 2.4");
        assert!(!in_kill_zone(-40.0, min, d, true));
        assert!(
            !in_kill_zone(-48.0, min, d, false),
            "only knobs with a kill"
        );
        assert!(!in_kill_zone(0.0, 0.0, None, true), "needs a detent");
        let _ = max;
    }

    #[test]
    fn up_is_more() {
        let (min, d, max) = FILTER;
        assert!(dragged_value(0.0, 10.0, false, min, d, max) > 0.0);
        assert!(dragged_value(0.0, -10.0, false, min, d, max) < 0.0);
        assert!(near(dragged_value(0.0, 200.0, false, 0.0, None, 1.0), 1.0));
        assert!(near(dragged_value(0.0, 100.0, false, 0.0, None, 1.0), 0.5));
    }

    #[test]
    fn shift_is_ten_times_finer() {
        let (min, d, max) = TRIM;
        let fine = dragged_value(1.0, 20.0, true, min, d, max);
        let coarse = dragged_value(1.0, 2.0, false, min, d, max);
        assert!(near(fine, coarse));
    }

    #[test]
    fn eq_drag_tracks_pointer_angle() {
        let (min, d, max) = EQ;
        // 200 px is 270 degrees, so 50 px is 67.5 degrees: +3 dB, not the +13.5 dB a linear
        // drag over the 54 dB range would give.
        let v = dragged_value(0.0, 50.0, false, min, d, max);
        assert!(near(angle_of(v, min, d, max), 67.5));
        assert!(near(v, 3.0));
    }

    #[test]
    fn clamps_at_ends() {
        let (min, d, max) = EQ;
        assert!(near(dragged_value(0.0, 1000.0, false, min, d, max), 6.0));
        assert!(near(dragged_value(0.0, -1000.0, false, min, d, max), -48.0));
    }

    #[test]
    fn shift_bypasses_detent_pull() {
        let eq =
            om_ui::control::Knob::new("EQ", om_ui::control::Readout::Decibels, -48.0, 6.0, 0.0)
                .with_detent();
        assert_eq!(snap_with_bypass(&eq, 1.0, false), 0.0);
        assert_eq!(snap_with_bypass(&eq, 1.0, true), 1.0);
        assert_eq!(snap_with_bypass(&eq, 5.0, false), 5.0);
    }

    fn near32(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn geom_is_square_and_centred() {
        let cases = [
            (200.0, 40.0, 0.0, KnobRole::Wide),
            (58.0, 56.0, 14.0, KnobRole::Band),
            (106.0, 32.0, 0.0, KnobRole::Cue),
            (24.0, 24.0, 0.0, KnobRole::Stem),
            (28.0, 28.0, 0.0, KnobRole::Sampler),
        ];
        for (w, h, label_h, role) in cases {
            let g = knob_geometry(w, h, label_h, role);
            let avail = w.min(h - label_h);
            let (bx, by, side) = g.bbox();
            assert!(side <= avail + 1e-3, "{role:?}: side {side} > {avail}");
            assert!(g.cap_diameter() <= role.max_cap() + 1e-3, "{role:?}");
            assert!(bx >= -1e-3 && by >= -1e-3, "{role:?} inside the rect");
            assert!(bx + side <= w + 1e-3 && by + side <= h - label_h + 1e-3);
            assert!(near32(g.cx, w / 2.0), "{role:?} centred in x");
            assert!(near32(g.cy, (h - label_h) / 2.0), "{role:?} centred in y");
        }
        // The wide bar's knob fills its height, about 32 px.
        let wide = knob_geometry(200.0, 40.0, 0.0, KnobRole::Wide);
        assert!((wide.cap_diameter() - 32.0).abs() <= 1.0);
        // A roomy rect is capped at the role's maximum.
        let big = knob_geometry(200.0, 200.0, 0.0, KnobRole::Cue);
        assert!(near32(big.cap_diameter(), 36.0));
    }

    #[test]
    fn hit_test_inside_circle_only() {
        let g = knob_geometry(200.0, 40.0, 0.0, KnobRole::Wide);
        assert!(g.hits(g.cx, g.cy));
        assert!(g.hits(g.cx + 0.9 * g.r_cap, g.cy));
        // The gap at the bottom is still the knob.
        assert!(g.hits(g.cx, g.cy + g.r_track * 0.9));
        assert!(!g.hits(0.0, 0.0), "the rect corner");
        assert!(!g.hits(2.0, 20.0), "the far left of the wide bar");
        assert!(!g.hits(g.cx + g.r_track + g.stroke + 3.0, g.cy));
    }

    #[test]
    fn small_knob_drops_ticks_and_label() {
        let sampler = knob_geometry(28.0, 28.0, 0.0, KnobRole::Sampler);
        assert!(!sampler.shows_ticks());
        assert!(!knob_geometry(24.0, 24.0, 0.0, KnobRole::Stem).shows_ticks());
        let big = knob_geometry(100.0, 100.0, 0.0, KnobRole::Cue);
        assert!(big.shows_ticks());
        assert!(!KnobRole::Stem.shows_label());
        assert!(KnobRole::Band.shows_label());
        // A cap of 28 px or less uses the thinner stroke.
        assert!(near32(sampler.stroke, 2.0));
        assert!(near32(big.stroke, 2.5));
    }

    #[test]
    fn popup_anchor_is_above_knob_top() {
        let g = knob_geometry(100.0, 100.0, 0.0, KnobRole::Cue);
        let top = g.popup_top(200.0);
        assert!(near32(top, g.cy - g.r_track - POPUP_LIFT));
        // Near the card's top the popup goes below the knob instead of leaving the card.
        let tight = knob_geometry(40.0, 40.0, 0.0, KnobRole::Cue);
        assert!(tight.popup_top(5.0) >= tight.cy + tight.r_track - 1e-3);
        assert!(tight.popup_top(60.0) < tight.cy);
    }

    #[test]
    fn arc_segments_split_at_most_170_degrees() {
        let full = arc_segments(-135.0, 135.0);
        assert_eq!(full.len(), 2);
        assert!(near(full[0].0, -135.0) && near(full[1].1, 135.0));
        assert!(near(full[0].1, full[1].0), "pieces join");
        assert!(full.iter().all(|(a, b)| b - a <= MAX_ARC_SEGMENT + 1e-9));
        assert_eq!(arc_segments(0.0, 100.0), vec![(0.0, 100.0)]);
        assert_eq!(arc_segments(180.0, 0.0).len(), 2, "exactly 180 is split");
    }

    #[test]
    fn arc_segments_empty_and_reversed() {
        assert!(arc_segments(0.0, 0.0).is_empty());
        assert!(arc_segments(30.0, 30.0 + 1e-12).is_empty());
        assert_eq!(arc_segments(100.0, 0.0), arc_segments(0.0, 100.0));
    }

    #[test]
    fn point_on_runs_clockwise_from_twelve() {
        let close = |p: (f32, f32), q: (f32, f32)| near32(p.0, q.0) && near32(p.1, q.1);
        assert!(close(point_on(10.0, 10.0, 5.0, 0.0), (10.0, 5.0)));
        assert!(close(point_on(10.0, 10.0, 5.0, 90.0), (15.0, 10.0)));
        assert!(close(point_on(10.0, 10.0, 5.0, 180.0), (10.0, 15.0)));
        assert!(close(point_on(10.0, 10.0, 5.0, -90.0), (5.0, 10.0)));
    }
}
