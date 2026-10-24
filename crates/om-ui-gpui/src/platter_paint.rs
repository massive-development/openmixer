//! The on-screen platter: a disc with an outer ring that shows track progress, a marker that turns
//! while the deck plays, ticks on the disc, and a static centre label (the deck colour and letter;
//! no artwork, see the module note in `waveform_paint`). Everything here that decides a position or
//! an angle is pure and tested first; the painting turns the polylines into GPUI paths.
//!
//! Conventions, not copied art (ADR 0001): see `docs/design/research/deck-controls.md` section 3.
//! Scratch (the inner 85 %) maps drag angle to track position; the outer 15 % is the pitch bend
//! band, painted but not wired because the engine has no non-destructive nudge (Part B3).

use crate::paint::paint;
use gpui::{point, px, Bounds, Path, PathBuilder, Pixels, Point, Window};
use om_ui::geometry::Rect;
use om_ui::theme::Colour;
use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::TAU;

/// Largest and smallest platter diameter in px (research section 3: 160, 112 in compact).
pub const MAX_DIAMETER: i32 = 160;
pub const MIN_DIAMETER: i32 = 112;
/// Seconds of track per marker turn at rate 1.0 (33 1/3 rpm).
pub const SECONDS_PER_TURN: f64 = 1.8;
/// Width of the outer ring in px.
pub const RING_WIDTH: f32 = 4.0;
/// Share of the radius taken by the centre label.
pub const LABEL_SHARE: f32 = 0.56;
/// Radius share where the pitch bend band begins (the outer 15 %).
pub const BEND_SHARE: f32 = 0.85;
/// Marker diameter in px.
pub const MARKER_SIZE: f32 = 6.0;
/// Ticks on the disc; they turn with the marker.
const TICKS: usize = 24;
/// Polyline step in degrees for arcs and circles.
const ARC_STEP_DEG: f32 = 6.0;
/// Most vertices one platter may rebuild per frame, tested below.
pub const FRAME_VERTEX_BUDGET: usize = 400;

/// The platter's circles, in px, around `centre`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Platter {
    pub centre: (f32, f32),
    pub radius: f32,
    pub ring_outer: f32,
    pub ring_inner: f32,
    pub label_radius: f32,
    pub bend_band_inner: f32,
}

/// Which part of the platter a point is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Outside,
    /// The inner 85 %: drag to scratch.
    Scratch,
    /// The outer 15 %: pitch bend (not wired).
    Bend,
}

/// The largest platter that fits `rect`: at most [`MAX_DIAMETER`], centred.
pub fn platter_geometry(rect: Rect) -> Platter {
    let d = rect.w.min(rect.h).clamp(0, MAX_DIAMETER) as f32;
    let radius = d / 2.0;
    Platter {
        centre: (
            rect.x as f32 + rect.w as f32 / 2.0,
            rect.y as f32 + rect.h as f32 / 2.0,
        ),
        radius,
        ring_outer: radius,
        ring_inner: radius - RING_WIDTH,
        label_radius: radius * LABEL_SHARE,
        bend_band_inner: radius * BEND_SHARE,
    }
}

/// The square the platter occupies inside `rect`.
pub fn platter_rect(rect: Rect) -> Rect {
    let d = rect.w.min(rect.h).clamp(0, MAX_DIAMETER);
    Rect::new(rect.x + (rect.w - d) / 2, rect.y + (rect.h - d) / 2, d, d)
}

/// Marker angle in degrees (0 = top, clockwise, in 0..360) for a playhead at `track_seconds`.
/// The playhead already advances at the deck's rate, so one turn per [`SECONDS_PER_TURN`] of track
/// is 33 1/3 rpm at rate 1.0, faster or slower with the pitch fader, and it holds while paused or
/// stops with a seek.
pub fn marker_angle(track_seconds: f64) -> f32 {
    ((track_seconds / SECONDS_PER_TURN).rem_euclid(1.0) * 360.0) as f32
}

/// Degrees of the played arc: 0 to 360 over the track. 0 for an unknown length.
pub fn progress_sweep(position_frames: i64, length_frames: i64) -> f32 {
    if length_frames <= 0 {
        return 0.0;
    }
    (position_frames as f64 / length_frames as f64).clamp(0.0, 1.0) as f32 * 360.0
}

/// Distance of `p` from the centre decides the zone.
pub fn zone_at(p: (f32, f32), platter: &Platter) -> Zone {
    let d = ((p.0 - platter.centre.0).powi(2) + (p.1 - platter.centre.1).powi(2)).sqrt();
    if d > platter.ring_outer {
        Zone::Outside
    } else if d >= platter.bend_band_inner {
        Zone::Bend
    } else {
        Zone::Scratch
    }
}

/// Angle of `p` around the centre in degrees (0 = top, clockwise, -180..=180).
pub fn pointer_angle(p: (f32, f32), platter: &Platter) -> f32 {
    let (dx, dy) = (p.0 - platter.centre.0, p.1 - platter.centre.1);
    dx.atan2(-dy).to_degrees()
}

/// The shortest signed turn from `prev` to `now`, in (-180, 180], across the +-180 seam.
pub fn angle_delta(prev: f32, now: f32) -> f32 {
    let d = (now - prev).rem_euclid(360.0);
    if d > 180.0 {
        d - 360.0
    } else {
        d
    }
}

/// Where a scratch drag points: `start_frame` plus the dragged angle (`now_angle` is the running
/// total, not wrapped) as a share of `frames_per_turn`, kept inside the track.
pub fn scratch_target(
    start_frame: i64,
    start_angle: f32,
    now_angle: f32,
    frames_per_turn: f64,
    length_frames: i64,
) -> i64 {
    let turns = f64::from(now_angle - start_angle) / 360.0;
    let target = start_frame as f64 + turns * frames_per_turn;
    target.round().clamp(0.0, length_frames.max(0) as f64) as i64
}

type Poly = Vec<(f32, f32)>;

fn circle(centre: (f32, f32), radius: f32) -> Poly {
    let n = (360.0 / ARC_STEP_DEG) as usize;
    (0..=n)
        .map(|i| {
            let a = i as f32 / n as f32 * TAU;
            (centre.0 + radius * a.sin(), centre.1 - radius * a.cos())
        })
        .collect()
}

fn arc(centre: (f32, f32), radius: f32, from_deg: f32, sweep_deg: f32) -> Poly {
    let n = (sweep_deg.abs() / ARC_STEP_DEG).ceil().max(1.0) as usize;
    (0..=n)
        .map(|i| {
            let a = (from_deg + sweep_deg * i as f32 / n as f32).to_radians();
            (centre.0 + radius * a.sin(), centre.1 - radius * a.cos())
        })
        .collect()
}

/// Static shapes: disc, ring track, the line where the bend band starts. Built once per size.
pub fn static_polylines(p: &Platter) -> Vec<Poly> {
    vec![
        circle(p.centre, p.radius - RING_WIDTH),
        circle(p.centre, p.ring_outer - RING_WIDTH / 2.0),
        circle(p.centre, p.bend_band_inner - RING_WIDTH),
    ]
}

/// Shapes rebuilt each frame: the played arc on the ring, the turning ticks, the marker.
pub struct FrameShapes {
    pub progress: Poly,
    pub ticks: Vec<Poly>,
    pub marker: Poly,
}

impl FrameShapes {
    /// Total vertices, for the budget.
    pub fn vertices(&self) -> usize {
        self.progress.len() + self.ticks.iter().map(Vec::len).sum::<usize>() + self.marker.len()
    }
}

pub fn frame_shapes(p: &Platter, sweep_deg: f32, marker_deg: f32) -> FrameShapes {
    let mid = p.ring_outer - RING_WIDTH / 2.0;
    let (inner, outer) = (
        p.label_radius + 0.12 * p.radius,
        p.label_radius + 0.26 * p.radius,
    );
    let ticks = (0..TICKS)
        .map(|i| {
            let a = (marker_deg + i as f32 * 360.0 / TICKS as f32).to_radians();
            vec![
                (p.centre.0 + inner * a.sin(), p.centre.1 - inner * a.cos()),
                (p.centre.0 + outer * a.sin(), p.centre.1 - outer * a.cos()),
            ]
        })
        .collect();
    let m = marker_deg.to_radians();
    let at = (p.centre.0 + mid * m.sin(), p.centre.1 - mid * m.cos());
    FrameShapes {
        progress: if sweep_deg > 0.0 {
            arc(p.centre, mid, 0.0, sweep_deg)
        } else {
            Vec::new()
        },
        ticks,
        marker: circle(at, MARKER_SIZE / 2.0)
            .into_iter()
            .step_by(3)
            .collect(),
    }
}

fn build(poly: &Poly, stroke: Option<f32>) -> Option<Path<Pixels>> {
    if poly.len() < 2 {
        return None;
    }
    let mut b = match stroke {
        Some(w) => PathBuilder::stroke(px(w)),
        None => PathBuilder::fill(),
    };
    let at = |(x, y): (f32, f32)| point(px(x), px(y));
    b.move_to(at(poly[0]));
    for &v in &poly[1..] {
        b.line_to(at(v));
    }
    if stroke.is_none() {
        b.close();
    }
    b.build().ok()
}

/// The three paths rebuilt each frame: played arc, ticks (one stroked path) and marker.
fn frame_paths(shapes: &FrameShapes) -> [Option<Path<Pixels>>; 3] {
    let at = |(x, y): (f32, f32)| -> Point<Pixels> { point(px(x), px(y)) };
    let mut ticks = PathBuilder::stroke(px(2.0));
    for tick in &shapes.ticks {
        ticks.move_to(at(tick[0]));
        ticks.line_to(at(tick[1]));
    }
    [
        build(&shapes.progress, Some(RING_WIDTH)),
        ticks.build().ok(),
        build(&shapes.marker, None),
    ]
}

/// What one deck's platter paints this frame.
#[derive(Clone)]
pub struct PlatterScene {
    pub deck: usize,
    pub accent: Colour,
    pub disc: Colour,
    pub track: Colour,
    pub groove: Colour,
    pub tick: Colour,
    pub marker: Colour,
    pub position_frames: i64,
    pub length_frames: i64,
    /// Playhead in seconds of track, smoothed.
    pub track_seconds: f64,
}

struct Cached {
    key: [i32; 4],
    paths: Vec<Path<Pixels>>,
}

thread_local! {
    static STATIC: RefCell<HashMap<usize, Cached>> = RefCell::new(HashMap::new());
}

/// Paints the platter filling `bounds` (window coordinates). The disc and ring track are cached per
/// size; the played arc, ticks and marker are rebuilt every frame.
pub fn paint_platter(window: &mut Window, bounds: Bounds<Pixels>, scene: &PlatterScene) {
    let rect = Rect::new(
        f32::from(bounds.origin.x) as i32,
        f32::from(bounds.origin.y) as i32,
        f32::from(bounds.size.width) as i32,
        f32::from(bounds.size.height) as i32,
    );
    let p = platter_geometry(rect);
    if p.radius < 8.0 {
        return;
    }
    let key = [rect.x, rect.y, rect.w, rect.h];
    STATIC.with(|cache| {
        let mut cache = cache.borrow_mut();
        let entry = cache.entry(scene.deck).or_insert(Cached {
            key: [0; 4],
            paths: Vec::new(),
        });
        if entry.key != key || entry.paths.is_empty() {
            let shapes = static_polylines(&p);
            entry.paths = vec![
                build(&shapes[0], None),
                build(&shapes[1], Some(RING_WIDTH)),
                build(&shapes[2], Some(1.0)),
            ]
            .into_iter()
            .flatten()
            .collect();
            entry.key = key;
        }
        let colours = [scene.disc, scene.track, scene.groove];
        for (path, colour) in entry.paths.iter().zip(colours) {
            window.paint_path(path.clone(), paint(colour));
        }
    });

    let marker_deg = marker_angle(scene.track_seconds);
    let shapes = frame_shapes(
        &p,
        progress_sweep(scene.position_frames, scene.length_frames),
        marker_deg,
    );
    let [progress, ticks, marker] = frame_paths(&shapes);
    for (path, colour) in [
        (progress, scene.accent),
        (ticks, scene.tick),
        (marker, scene.marker),
    ] {
        if let Some(path) = path {
            window.paint_path(path, paint(colour));
        }
    }
    // The centre label is a div drawn over this canvas by the deck view.
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platter(d: i32) -> Platter {
        platter_geometry(Rect::new(0, 0, d, d))
    }

    #[test]
    fn label_is_56_percent_of_the_diameter_and_bend_band_the_outer_15() {
        let p = platter(160);
        assert!((p.label_radius * 2.0 - 0.56 * 160.0).abs() < 0.01);
        assert!((p.bend_band_inner - 0.85 * p.radius).abs() < 0.01);
        assert!((p.ring_outer - p.ring_inner - RING_WIDTH).abs() < 0.01);
    }

    #[test]
    fn geometry_scales_from_160_to_112_and_never_exceeds_160() {
        assert_eq!(platter(160).radius, 80.0);
        assert_eq!(platter(112).radius, 56.0);
        assert_eq!(platter_geometry(Rect::new(0, 0, 300, 200)).radius, 80.0);
        let wide = platter_rect(Rect::new(10, 20, 300, 130));
        assert_eq!((wide.w, wide.h), (130, 130));
        assert_eq!(wide.x, 10 + 85);
    }

    #[test]
    fn marker_turns_once_per_1_8_seconds_and_wraps() {
        assert_eq!(marker_angle(0.0), 0.0);
        assert!((marker_angle(0.9) - 180.0).abs() < 0.01);
        assert!(marker_angle(1.8) < 0.01 || marker_angle(1.8) > 359.99);
        assert!((marker_angle(1.8 * 3.0 + 0.45) - 90.0).abs() < 0.05);
        // Monotonic inside one turn, and a position before 0 stays in range.
        let mut last = -1.0;
        for i in 0..100 {
            let a = marker_angle(f64::from(i) * 0.0179);
            assert!(a > last);
            last = a;
        }
        assert!((0.0..360.0).contains(&marker_angle(-0.3)));
    }

    #[test]
    fn marker_speed_follows_the_playback_rate() {
        // A playhead moving at rate 1.1 covers 10 % more track per real second, so more angle.
        assert!(marker_angle(0.5 * 1.1) > marker_angle(0.5));
    }

    #[test]
    fn progress_sweeps_zero_to_360_over_the_track() {
        assert_eq!(progress_sweep(0, 0), 0.0);
        assert_eq!(progress_sweep(50, 0), 0.0);
        assert_eq!(progress_sweep(0, 1000), 0.0);
        assert!((progress_sweep(250, 1000) - 90.0).abs() < 1e-3);
        assert!((progress_sweep(2000, 1000) - 360.0).abs() < 1e-3);
        assert_eq!(progress_sweep(-5, 1000), 0.0);
    }

    #[test]
    fn zones_split_at_85_percent_and_the_rim() {
        let p = platter(160);
        let at = |d: f32| (80.0 + d, 80.0);
        assert_eq!(zone_at(at(0.0), &p), Zone::Scratch);
        assert_eq!(zone_at(at(67.9), &p), Zone::Scratch);
        assert_eq!(zone_at(at(68.0), &p), Zone::Bend);
        assert_eq!(zone_at(at(80.0), &p), Zone::Bend);
        assert_eq!(zone_at(at(80.5), &p), Zone::Outside);
    }

    #[test]
    fn angle_delta_takes_the_short_way_across_the_seam() {
        assert!((angle_delta(170.0, -170.0) - 20.0).abs() < 1e-4);
        assert!((angle_delta(-170.0, 170.0) + 20.0).abs() < 1e-4);
        assert!((angle_delta(10.0, 40.0) - 30.0).abs() < 1e-4);
        assert_eq!(angle_delta(30.0, 30.0), 0.0);
    }

    #[test]
    fn pointer_angle_is_zero_at_the_top_and_clockwise() {
        let p = platter(100);
        assert!(pointer_angle((50.0, 0.0), &p).abs() < 1e-3);
        assert!((pointer_angle((100.0, 50.0), &p) - 90.0).abs() < 1e-3);
        assert!((pointer_angle((0.0, 50.0), &p) + 90.0).abs() < 1e-3);
    }

    #[test]
    fn scratch_maps_angle_to_frames_and_clamps_to_the_track() {
        let per_turn = 1000.0;
        assert_eq!(scratch_target(5000, 0.0, 180.0, per_turn, 10_000), 5500);
        assert_eq!(scratch_target(5000, 0.0, -90.0, per_turn, 10_000), 4750);
        assert_eq!(scratch_target(100, 0.0, -360.0, per_turn, 10_000), 0);
        assert_eq!(scratch_target(9900, 0.0, 720.0, per_turn, 10_000), 10_000);
        assert_eq!(scratch_target(0, 0.0, 90.0, per_turn, 0), 0);
    }

    #[test]
    fn a_frame_rebuilds_under_the_vertex_budget() {
        let p = platter(160);
        let shapes = frame_shapes(&p, 360.0, 123.0);
        assert!(
            shapes.vertices() < FRAME_VERTEX_BUDGET,
            "{}",
            shapes.vertices()
        );
        let statics: usize = static_polylines(&p).iter().map(Vec::len).sum();
        assert!(statics < FRAME_VERTEX_BUDGET, "{statics}");
        assert!(frame_shapes(&p, 0.0, 0.0).progress.is_empty());
    }

    /// Cost of one platter's per-frame path building (CPU only; the GPU draw is not included).
    /// `cargo test -p om-ui-gpui --release platter_frame_cost -- --ignored --nocapture`
    #[test]
    #[ignore = "benchmark"]
    fn platter_frame_cost() {
        let p = platter(160);
        let runs = 2000;
        let start = std::time::Instant::now();
        let mut built = 0;
        for i in 0..runs {
            let shapes = frame_shapes(&p, (i % 360) as f32, (i * 7 % 360) as f32);
            built += frame_paths(&shapes).iter().flatten().count();
        }
        let per = start.elapsed().as_secs_f64() * 1000.0 / f64::from(runs);
        println!("platter frame build: {per:.4} ms per platter ({built} paths)");
        assert!(per < 0.3, "{per} ms");
    }

    #[test]
    fn a_platter_fits_inside_a_260_px_card_without_touching_the_transport() {
        use om_ui::theme::ThemeTokens;
        use om_ui::widgets::deck::{deck_controls_layout_for, PadMode};
        let theme = ThemeTokens::builtin_dark();
        let l = deck_controls_layout_for(Rect::sized(290, 262), &theme, PadMode::Cues);
        let drawer = crate::deck_view::drawer_rect(&l);
        let r = platter_rect(drawer);
        assert!(r.w >= MIN_DIAMETER, "{r:?} in {drawer:?}");
        assert!(Rect::sized(290, 262).contains(&r));
        assert!(!r.intersects(&l.transport) && !r.intersects(&l.tabs) && !r.intersects(&l.pitch));
    }
}
