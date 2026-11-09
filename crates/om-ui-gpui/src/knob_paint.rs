//! Rotary knob maths and painting. The maths is plain functions over `f64`/`f32` so it runs
//! headless; painting is a thin layer over it.
//!
//! Angles are degrees clockwise from 12 o'clock. The sweep is 270 degrees (spec: `docs/design/
//! research/knobs.md` section 3), leaving a 90 degree gap at the bottom.

use crate::paint::{paint, paint_focus_ring, paint_raised};
use gpui::{point, px, size, Bounds, Path, PathBuilder, Pixels, Point, Window};
use om_ui::style::{LookAndFeel, RingStyle, ShadowStyle};
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

/// Which control a knob is, for where its label goes.
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

/// Width of the value arc and of the track it runs along, px (canvas "Knob": a 2px arc).
pub const ARC_STROKE: f32 = 2.0;
/// The cap sits this far inside the arc's outer edge, px (canvas "Knob": `inset: 5px`).
pub const CAP_INSET: f32 = 5.0;
/// How far above a control its value popup floats, px.
pub const POPUP_LIFT: f32 = 26.0;
/// Clearance between a popup flipped below its knob and the track, px. Unsourced.
const POPUP_FLIP_GAP: f32 = 2.0;

impl KnobRole {
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

/// Fits a knob of diameter `size` (the density's knob size: 34 Roomy, 26 Compact) into `rect_w` x
/// `rect_h`, leaving `label_h` below for a label; a smaller rect shrinks it. The arc is the outer
/// 2px ring and the cap sits [`CAP_INSET`] inside it.
pub fn knob_geometry(rect_w: f32, rect_h: f32, label_h: f32, size: f32) -> KnobGeom {
    let diameter = rect_w.min(rect_h - label_h).min(size).max(1.0);
    let r = diameter / 2.0;
    KnobGeom {
        cx: rect_w / 2.0,
        cy: (rect_h - label_h) / 2.0,
        r_cap: (r - CAP_INSET).max(1.0),
        r_track: (r - ARC_STROKE / 2.0).max(0.5),
        stroke: ARC_STROKE,
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

    /// True when `(x, y)` is within `radius` of the centre, or on the knob when that is larger: a
    /// small knob padded out to a minimum pointer target.
    pub fn hits_within(&self, x: f32, y: f32, radius: f32) -> bool {
        let r = radius.max(self.hit_radius());
        let (dx, dy) = (x - self.cx, y - self.cy);
        dx * dx + dy * dy <= r * r
    }

    /// Outer radius of the track stroke: the knob's reach.
    fn hit_radius(&self) -> f32 {
        self.r_track + self.stroke / 2.0
    }

    /// Cap diameter.
    pub fn cap_diameter(&self) -> f32 {
        2.0 * self.r_cap
    }

    /// Outer diameter: the arc's outer edge.
    pub fn diameter(&self) -> f32 {
        2.0 * self.hit_radius()
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
    pub cap_border: Colour,
    pub pointer: Colour,
    /// The detent mark.
    pub mark: Colour,
    /// The small drop under the cap.
    pub drop: ShadowStyle,
    /// The keyboard focus ring.
    pub ring: RingStyle,
}

impl KnobColours {
    pub fn new(look: &LookAndFeel, t: &ThemeTokens, accent: Colour) -> Self {
        Self {
            track: look.knob.track,
            value: accent,
            kill: look.knob.kill,
            cap: look.knob.body.fill,
            cap_border: look.knob.body.border,
            pointer: look.knob.pointer,
            mark: t.text_secondary,
            drop: look.knob.drop,
            ring: look.focus,
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
    /// Being dragged: the arc in the pointer colour mixed further in.
    pub active: bool,
    pub disabled: bool,
    /// Keyboard focus: the 2px ring 2px outside the arc.
    pub focused: bool,
}

/// Weight of the pointer colour mixed into the value arc on hover: the spec's "+10% lightness".
const HOVER_LIFT: f32 = 0.1;
/// The same while dragging, so a held knob reads without a heavier stroke. Unsourced.
const ACTIVE_LIFT: f32 = 0.2;
/// Opacity of a disabled knob (an empty sampler pad). Unsourced, as before.
const DISABLED_FADE: f32 = 0.4;
/// Pointer line: from this share of the cap radius out to 1 px short of the cap's edge (canvas
/// "Knob": a 2px line from the top 1px of the cap over 40% of its height).
const POINTER_INNER: f32 = 0.2;
const POINTER_WIDTH: f32 = 2.0;
/// Width of the detent mark, px.
const MARK_WIDTH: f32 = 2.0;

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

/// The square of side `diameter` centred on `(x, y)` in the box at `origin`.
fn square(origin: Point<Pixels>, (x, y): (f32, f32), diameter: f32) -> Bounds<Pixels> {
    Bounds::new(
        px_point(origin, (x - diameter / 2.0, y - diameter / 2.0)),
        size(px(diameter), px(diameter)),
    )
}

/// The value arc's colour: the kill colour in the kill zone, else the accent, lifted toward the
/// pointer colour while hovered or dragged.
fn value_colour(state: &KnobPaintState, colours: &KnobColours) -> Colour {
    let lift = if state.active {
        ACTIVE_LIFT
    } else if state.hovered {
        HOVER_LIFT
    } else {
        0.0
    };
    if state.killed {
        colours.kill
    } else {
        Colour::mix(&[(colours.value, 1.0 - lift), (colours.pointer, lift)])
    }
}

/// Paints a knob into the box whose top-left is `origin` (the geometry's coordinates are relative
/// to it): the 2px track, the value arc from the detent (or the minimum), the detent mark, the
/// raised cap with its drop and etched edge, the pointer, and the focus ring when focused. Arc ends
/// are square, as the canvas draws them.
pub fn paint_knob(
    window: &mut Window,
    origin: Point<Pixels>,
    state: &KnobPaintState,
    colours: &KnobColours,
) {
    let g = &state.geom;
    let fade = if state.disabled { DISABLED_FADE } else { 1.0 };
    let col = |c: Colour| paint(c.with_multiplied_alpha(fade));
    let centre = (g.cx, g.cy);

    if let Some(path) = arc_path(
        origin,
        centre,
        g.r_track,
        (SWEEP_START, SWEEP_END),
        g.stroke,
    ) {
        window.paint_path(path, col(colours.track));
    }
    let from = if state.bipolar {
        state.detent_angle.unwrap_or(0.0)
    } else {
        SWEEP_START
    };
    if (state.angle - from).abs() > 0.5 {
        if let Some(path) = arc_path(origin, centre, g.r_track, (from, state.angle), g.stroke) {
            window.paint_path(path, col(value_colour(state, colours)));
        }
    }

    // The detent mark sits in the gap between cap and track.
    let (inner, outer) = (g.r_cap + 0.5, g.r_track - g.stroke / 2.0);
    if let Some(angle) = state.detent_angle.filter(|_| outer > inner) {
        let (a, b) = (
            point_on(g.cx, g.cy, inner, angle),
            point_on(g.cx, g.cy, outer, angle),
        );
        if let Some(path) = line_path(origin, a, b, MARK_WIDTH) {
            window.paint_path(path, col(colours.mark));
        }
    }

    let cap = square(origin, centre, g.cap_diameter());
    let cap_colours = (col(colours.cap), col(colours.cap_border));
    paint_raised(window, cap, g.r_cap, cap_colours, &colours.drop);

    let pointer = if state.killed {
        colours.kill
    } else {
        colours.pointer
    };
    let (a, b) = (
        point_on(g.cx, g.cy, g.r_cap * POINTER_INNER, state.angle),
        point_on(g.cx, g.cy, (g.r_cap - 1.0).max(0.0), state.angle),
    );
    if let Some(path) = line_path(origin, a, b, POINTER_WIDTH) {
        window.paint_path(path, col(pointer));
    }

    if state.focused && !state.disabled {
        let outer = square(origin, centre, g.diameter());
        paint_focus_ring(window, outer, g.diameter() / 2.0, &colours.ring);
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

    const ROOMY: f32 = 34.0;
    const COMPACT: f32 = 26.0;

    #[test]
    fn geom_is_square_centred_and_the_density_size() {
        let cases = [
            (200.0, 40.0, 0.0, ROOMY),
            (58.0, 56.0, 14.0, ROOMY),
            (106.0, 32.0, 0.0, ROOMY),
            (24.0, 24.0, 0.0, COMPACT),
            (28.0, 28.0, 0.0, COMPACT),
        ];
        for (w, h, label_h, size) in cases {
            let g = knob_geometry(w, h, label_h, size);
            let avail = w.min(h - label_h);
            let (bx, by, side) = g.bbox();
            assert!(side <= avail.min(size) + 1e-3, "{w}x{h}: side {side}");
            assert!(bx >= -1e-3 && by >= -1e-3, "{w}x{h} inside the rect");
            assert!(bx + side <= w + 1e-3 && by + side <= h - label_h + 1e-3);
            assert!(near32(g.cx, w / 2.0) && near32(g.cy, (h - label_h) / 2.0));
            assert!(near32(g.stroke, 2.0), "a 2px arc");
        }
        // Room enough: the circle is the density's size, the cap 5 px inside it.
        for size in [ROOMY, COMPACT] {
            let g = knob_geometry(200.0, 200.0, 0.0, size);
            assert!(near32(g.diameter(), size));
            assert!(near32(g.cap_diameter(), size - 2.0 * CAP_INSET));
        }
        // A short cell shrinks it.
        assert!(near32(
            knob_geometry(200.0, 30.0, 0.0, ROOMY).diameter(),
            30.0
        ));
    }

    #[test]
    fn hit_test_inside_circle_only() {
        let g = knob_geometry(200.0, 40.0, 0.0, ROOMY);
        assert!(g.hits(g.cx, g.cy));
        assert!(g.hits(g.cx + 0.9 * g.r_cap, g.cy));
        // The gap at the bottom is still the knob.
        assert!(g.hits(g.cx, g.cy + g.r_track * 0.9));
        assert!(!g.hits(0.0, 0.0), "the rect corner");
        assert!(!g.hits(2.0, 20.0), "the far left of the wide bar");
        assert!(!g.hits(g.cx + g.r_track + g.stroke + 3.0, g.cy));
    }

    #[test]
    fn only_the_stem_knob_drops_its_label() {
        assert!(!KnobRole::Stem.shows_label());
        assert!(KnobRole::Band.shows_label() && KnobRole::Sampler.shows_label());
    }

    #[test]
    fn popup_anchor_is_above_knob_top() {
        let g = knob_geometry(100.0, 100.0, 0.0, ROOMY);
        let top = g.popup_top(200.0);
        assert!(near32(top, g.cy - g.r_track - POPUP_LIFT));
        // Near the card's top the popup goes below the knob instead of leaving the card.
        let tight = knob_geometry(40.0, 40.0, 0.0, ROOMY);
        assert!(tight.popup_top(5.0) >= tight.cy + tight.r_track - 1e-3);
        assert!(tight.popup_top(60.0) < tight.cy);
    }

    #[test]
    fn a_small_knob_takes_the_pointer_over_the_minimum_target() {
        let t = ThemeTokens::builtin_dark();
        let compact = t.compact.knob_size;
        let g = knob_geometry(40.0, 40.0, 0.0, compact);
        assert!(
            g.diameter() < t.min_pointer_target,
            "the Compact knob is drawn small"
        );
        let edge = g.cx + t.min_pointer_target / 2.0 - 0.5;
        assert!(!g.hits(edge, g.cy), "outside the drawn disc");
        assert!(
            g.hits_within(edge, g.cy, t.min_pointer_target / 2.0),
            "inside the padded target"
        );
        assert!(!g.hits_within(g.cx + 30.0, g.cy, t.min_pointer_target / 2.0));
    }

    #[test]
    fn hover_and_drag_lift_the_value_arc_but_not_in_the_kill_zone() {
        let t = ThemeTokens::builtin_dark();
        let look = LookAndFeel::from_tokens(&t);
        let colours = KnobColours::new(&look, &t, t.accent_for(0));
        let state = |hovered, active, killed| KnobPaintState {
            geom: knob_geometry(40.0, 40.0, 0.0, ROOMY),
            angle: 60.0,
            detent_angle: Some(0.0),
            bipolar: true,
            killed,
            hovered,
            active,
            disabled: false,
            focused: false,
        };
        let rest = value_colour(&state(false, false, false), &colours);
        assert_eq!(rest, t.accent_for(0));
        assert_ne!(value_colour(&state(true, false, false), &colours), rest);
        assert_ne!(
            value_colour(&state(false, true, false), &colours),
            value_colour(&state(true, false, false), &colours)
        );
        // The kill reads in the text colour with its "kill" word, never red (the Signal Rule).
        assert_eq!(
            value_colour(&state(true, true, true), &colours),
            t.text_primary
        );
        assert_ne!(colours.kill, t.danger);
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
