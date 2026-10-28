//! Segmented LED meter: the segment model (counts, dB per segment, zone colours, lit runs, the hold
//! segment), a render-agnostic [`MeterPlan`], and the GPUI painter.
//!
//! Plan: `docs/superpowers/plans/2026-10-24-ui-faders-meters.md` Phases 1.4 and 3. Conventions only
//! (ADR 0001).
//!
//! Element budget (plan Phase 4): a segmented bar is NOT one box per segment. Each side is a well
//! (one box), one `canvas` that paints the coalesced lit runs, the 1 px gaps and the hold segment as
//! quads, and the clip lamp (one box). A quad is a GPU instance, not a layout element, so a 160 px
//! meter costs a handful of boxes however many segments it has.

use crate::app::Shell;
use crate::controls::BarState;
use crate::paint::{border, paint, place};
use gpui::{canvas, fill, point, prelude::*, px, size, Bounds, Context, Div, MouseButton};
use gpui::{MouseDownEvent, Rgba};
use om_ui::geometry::Rect;
use om_ui::meter::{
    column_layout, MeterScale, MeterZone, DB_TICKS, MIN_LAMP_HEIGHT, MIN_MARK_SPACING,
};
use om_ui::style::LookAndFeel;
use om_ui::theme::ThemeTokens;

/// Smallest text drawn on a control, in px.
const MIN_TEXT_PX: f32 = 12.0;
/// Static labels for [`DB_TICKS`], so no label is formatted per frame.
const TICK_LABELS: [&str; 5] = ["0", "-6", "-12", "-24", "-48"];
/// Tolerance when comparing a segment's dB with a zone threshold, so a boundary that lands exactly
/// on a segment top is not decided by float rounding.
const ZONE_EPS: f32 = 1e-3;
/// Height of the numeric peak readout under a master meter, in px (12 px text plus a hair).
const READOUT_H: i32 = 14;
/// Space between the dB scale's right edge and the bars, in px. It was 3 px, which read as touching.
const SCALE_GAP: i32 = 6;
/// How much taller the master's clip lamp is than a channel's, in px. Unsourced: the plan asks for
/// "taller on master" and the research gives 6 px, which is below today's 8 px channel lamp.
const MASTER_LAMP_EXTRA: i32 = 4;

/// A run of lit segments of one zone, as a pixel rectangle relative to the bar's top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    pub zone: MeterZone,
    /// Index of the lowest segment in the run (0 is the bottom one).
    pub first: usize,
    pub count: usize,
    pub y: i32,
    pub h: i32,
}

/// The segments of one bar. Segment 0 is the bottom one; the stack sits on the bottom of the bar
/// and any leftover pixels are above the top segment. No GPUI types.
#[derive(Debug, Clone, Copy)]
pub struct SegmentModel {
    scale: MeterScale,
    height: i32,
    seg: i32,
    gap: i32,
    n: usize,
}

impl SegmentModel {
    /// The model for a bar `height_px` tall, with the segment size, gap and thresholds from the
    /// theme (`meter_segment`, `meter_segment_gap`, `meter_floor_db`, `meter_mid_db`,
    /// `meter_high_db`).
    pub fn new(t: &ThemeTokens, height_px: i32) -> Self {
        let seg = (t.meter_segment.round() as i32).max(1);
        let gap = (t.meter_segment_gap.round() as i32).max(0);
        let height = height_px.max(0);
        let n = ((height + gap) / (seg + gap)).max(0) as usize;
        Self {
            scale: MeterScale::from_tokens(t),
            height,
            seg,
            gap,
            n,
        }
    }

    /// Segments that fit: `floor((h + gap) / (segment + gap))`.
    pub fn count(&self) -> usize {
        self.n
    }

    fn pitch(&self) -> i32 {
        self.seg + self.gap
    }

    /// Height of the stack of segments and gaps.
    pub fn stack_height(&self) -> i32 {
        if self.n == 0 {
            0
        } else {
            self.n as i32 * self.pitch() - self.gap
        }
    }

    /// Unused pixels above the top segment.
    pub fn pad_top(&self) -> i32 {
        self.height - self.stack_height()
    }

    /// Top of segment `i` (0 is the bottom one), from the bar's top.
    pub fn segment_y(&self, i: usize) -> i32 {
        self.pad_top() + (self.n - 1 - i) as i32 * self.pitch()
    }

    /// dB at the top of segment `i`: linear in position from the floor (bottom) to 0 dB (top).
    pub fn segment_top_db(&self, i: usize) -> f32 {
        self.scale.floor_db + (i + 1) as f32 * -self.scale.floor_db / self.n.max(1) as f32
    }

    /// The colour zone of segment `i`: red from the high threshold up, amber from the mid one,
    /// else green, judged by the segment's top dB (`MeterScale::zone` semantics).
    pub fn zone(&self, i: usize) -> MeterZone {
        let db = self.segment_top_db(i);
        if db + ZONE_EPS >= self.scale.high_db {
            MeterZone::High
        } else if db + ZONE_EPS >= self.scale.mid_db {
            MeterZone::Mid
        } else {
            MeterZone::Low
        }
    }

    /// How many segments a bar at `level` (`0..=1`) lights, counted from the bottom. Rounds up, so
    /// any audible signal lights at least the first segment.
    pub fn lit_count(&self, level: f32) -> usize {
        if level <= 0.0 || self.n == 0 {
            return 0;
        }
        let exact = level.clamp(0.0, 1.0) * self.n as f32;
        ((exact - 1e-4).ceil().max(1.0) as usize).min(self.n)
    }

    /// The lit segments as at most one run per zone (low, mid, high), each spanning its segments
    /// and the gaps between them.
    pub fn lit_runs(&self, level: f32) -> [Option<Run>; 3] {
        let lit = self.lit_count(level);
        let mut runs = [None; 3];
        let zones = [MeterZone::Low, MeterZone::Mid, MeterZone::High];
        for (slot, zone) in runs.iter_mut().zip(zones) {
            let (mut first, mut end) = (usize::MAX, 0);
            for i in 0..lit {
                if self.zone(i) == zone {
                    first = first.min(i);
                    end = i + 1;
                }
            }
            if first != usize::MAX {
                let count = end - first;
                let y = self.segment_y(end - 1);
                *slot = Some(Run {
                    zone,
                    first,
                    count,
                    y,
                    h: count as i32 * self.pitch() - self.gap,
                });
            }
        }
        runs
    }

    /// The segment that shows a hold level (`0..=1`), if there is one.
    pub fn hold_index(&self, hold: f32) -> Option<usize> {
        let lit = self.lit_count(hold);
        (lit > 0).then(|| lit - 1)
    }

    /// The hold segment as a one-segment run in its zone's colour.
    pub fn hold_run(&self, hold: f32) -> Option<Run> {
        let i = self.hold_index(hold)?;
        Some(Run {
            zone: self.zone(i),
            first: i,
            count: 1,
            y: self.segment_y(i),
            h: self.seg,
        })
    }

    /// Top row of each gap between segments, bottom gap first (`n - 1` of them).
    pub fn gap_ys(&self) -> impl Iterator<Item = i32> + '_ {
        let (n, seg) = (self.n, self.seg);
        (0..n.saturating_sub(1)).map(move |i| self.segment_y(i + 1) + seg)
    }

    /// Where a scale proportion (`0` floor .. `1` 0 dB) sits, from the bar's top.
    pub fn y_of(&self, proportion: f32) -> i32 {
        self.pad_top() + ((1.0 - proportion.clamp(0.0, 1.0)) * self.stack_height() as f32) as i32
    }
}

/// What a quad in a bar's canvas is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuadRole {
    Lit(MeterZone),
    /// A 1 px separator between segments, in the bar's border colour.
    Gap,
}

/// One rectangle painted inside a bar's canvas, relative to the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarQuad {
    pub rect: Rect,
    pub role: QuadRole,
}

/// How a meter column differs between channel and master.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeterFace {
    /// Clip lamp height in px.
    pub lamp_h: i32,
    /// Both lamps light when either side clips (master). Unsourced detail of the plan's
    /// `master_clip_lamp_lights_when_either_side_clips`.
    pub shared_lamp: bool,
    /// A numeric peak readout under the bars (master).
    pub readout: bool,
}

impl MeterFace {
    /// Channel meter: today's lamp, no readout.
    pub fn channel(t: &ThemeTokens) -> Self {
        let look = LookAndFeel::from_tokens(t);
        Self {
            lamp_h: (look.meter.lamp_height.round() as i32).max(MIN_LAMP_HEIGHT),
            shared_lamp: false,
            readout: false,
        }
    }

    /// Master meter: a taller shared lamp and the peak readout.
    pub fn master(t: &ThemeTokens) -> Self {
        let ch = Self::channel(t);
        Self {
            lamp_h: ch.lamp_h + MASTER_LAMP_EXTRA,
            shared_lamp: true,
            readout: true,
        }
    }
}

/// One side of a meter column.
#[derive(Debug, Clone, PartialEq)]
pub struct SidePlan {
    /// The bordered well (column coordinates).
    pub well: Rect,
    /// The area inside the well the canvas covers (column coordinates).
    pub area: Rect,
    pub quads: Vec<BarQuad>,
    pub lamp: Rect,
    pub lamp_lit: bool,
}

/// One label of the dB scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkPlan {
    pub rect: Rect,
    pub text: &'static str,
}

/// Everything a meter column draws.
#[derive(Debug, Clone, PartialEq)]
pub struct MeterPlan {
    pub marks: Vec<MarkPlan>,
    pub sides: [SidePlan; 2],
    /// The numeric peak readout box, when the face asks for one.
    pub readout: Option<Rect>,
}

impl MeterPlan {
    /// Plans a meter column in `rect` (only its size is used).
    pub fn new(
        t: &ThemeTokens,
        rect: Rect,
        labels: bool,
        bars: [BarState; 2],
        face: MeterFace,
    ) -> Self {
        let look = LookAndFeel::from_tokens(t);
        let m = look.meter;
        let headroom = m.headroom as i32;
        let readout_h = if face.readout { READOUT_H } else { 0 };
        let col = column_layout(Rect::sized(rect.w, rect.h), labels);
        let well_h = (rect.h - headroom - readout_h).max(1);
        let bw = t.border_width.round().max(0.0) as i32;
        let side_inset = m.bar_inset as i32;
        let area_h = (well_h - 2 * bw).max(1);
        let model = SegmentModel::new(t, area_h);
        let any_clip = bars.iter().any(|b| b.clipping);

        let mut marks = Vec::with_capacity(DB_TICKS.len());
        if labels {
            let scale = MeterScale::from_tokens(t);
            let mut last: Option<f32> = None;
            for (db, text) in DB_TICKS.iter().zip(TICK_LABELS) {
                if *db < scale.floor_db {
                    continue;
                }
                let p = scale.proportion(*db);
                let fits = last.is_none_or(|l| (l - p) * area_h as f32 >= MIN_MARK_SPACING as f32);
                if !fits {
                    continue;
                }
                last = Some(p);
                let y = headroom + bw + model.y_of(p) - 6;
                marks.push(MarkPlan {
                    rect: Rect::new(
                        0,
                        y.clamp(0, (rect.h - 12).max(0)),
                        (col.labels.w - SCALE_GAP).max(0),
                        12,
                    ),
                    text,
                });
            }
        }

        let side = |bar: &Rect, state: BarState| {
            let well = Rect::new(bar.x, headroom, bar.w.max(1), well_h);
            let area = Rect::new(
                well.x + side_inset,
                headroom + bw,
                (well.w - 2 * side_inset).max(1),
                area_h,
            );
            let mut quads = Vec::with_capacity(model.count() + 4);
            for run in model.lit_runs(state.level).into_iter().flatten() {
                quads.push(BarQuad {
                    rect: Rect::new(0, run.y, area.w, run.h),
                    role: QuadRole::Lit(run.zone),
                });
            }
            for y in model.gap_ys() {
                quads.push(BarQuad {
                    rect: Rect::new(0, y, area.w, (t.meter_segment_gap.round() as i32).max(1)),
                    role: QuadRole::Gap,
                });
            }
            if let Some(run) = model.hold_run(state.hold) {
                quads.push(BarQuad {
                    rect: Rect::new(0, run.y, area.w, run.h),
                    role: QuadRole::Lit(run.zone),
                });
            }
            let lamp_y = (m.inset as i32).min(headroom - face.lamp_h).max(0);
            SidePlan {
                well,
                area,
                quads,
                lamp: Rect::new(area.x, lamp_y, area.w, face.lamp_h),
                lamp_lit: state.clipping || (face.shared_lamp && any_clip),
            }
        };
        let sides = [side(&col.bars[0], bars[0]), side(&col.bars[1], bars[1])];
        let readout = face.readout.then(|| {
            Rect::new(
                col.bars[0].x,
                headroom + well_h,
                col.bars[1].x + col.bars[1].w - col.bars[0].x,
                READOUT_H,
            )
        });
        Self {
            marks,
            sides,
            readout,
        }
    }

    /// Layout boxes the painter emits: the root, the scale labels, the readout, and per side a
    /// well, a canvas and a lamp. Quads inside the canvases are not layout elements.
    pub fn div_count(&self) -> usize {
        1 + self.marks.len() + usize::from(self.readout.is_some()) + 3 * self.sides.len()
    }

    /// Quads painted across both canvases.
    pub fn quad_count(&self) -> usize {
        self.sides.iter().map(|s| s.quads.len()).sum()
    }
}

/// The peak readout text for a master meter: the larger hold of the two sides in dB, one decimal
/// (`-2.3`), `-inf` when both are empty.
pub fn peak_text(t: &ThemeTokens, bars: [BarState; 2]) -> String {
    let hold = bars[0].hold.max(bars[1].hold);
    if hold <= 0.0 {
        return "-inf".to_owned();
    }
    format!("{:.1}", MeterScale::from_tokens(t).db_at(hold))
}

fn quad_colour(m: &om_ui::style::MeterStyle, role: QuadRole) -> Rgba {
    paint(match role {
        QuadRole::Lit(MeterZone::Low) => m.low,
        QuadRole::Lit(MeterZone::Mid) => m.mid,
        QuadRole::Lit(MeterZone::High) => m.high,
        QuadRole::Gap => m.border,
    })
}

/// One stereo meter column with segmented bars. Signature matches `controls::stereo_meter`; a
/// click anywhere on the column runs `on_click` (it clears the lamps).
pub fn segmented_meter(
    t: &ThemeTokens,
    rect: Rect,
    labels: bool,
    bars: [BarState; 2],
    cx: &mut Context<Shell>,
    on_click: impl Fn(&mut Shell) + 'static,
) -> Div {
    meter_with_face(
        t,
        rect,
        labels,
        bars,
        MeterFace::channel(t),
        None,
        cx,
        on_click,
    )
}

/// [`segmented_meter`] for the master: taller shared clip lamp and a numeric peak readout under
/// the bars.
pub fn stereo_meter_with_readout(
    t: &ThemeTokens,
    rect: Rect,
    labels: bool,
    bars: [BarState; 2],
    cx: &mut Context<Shell>,
    on_click: impl Fn(&mut Shell) + 'static,
) -> Div {
    let text = peak_text(t, bars);
    meter_with_face(
        t,
        rect,
        labels,
        bars,
        MeterFace::master(t),
        Some(text),
        cx,
        on_click,
    )
}

#[allow(clippy::too_many_arguments)]
fn meter_with_face(
    t: &ThemeTokens,
    rect: Rect,
    labels: bool,
    bars: [BarState; 2],
    face: MeterFace,
    readout: Option<String>,
    cx: &mut Context<Shell>,
    on_click: impl Fn(&mut Shell) + 'static,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let m = look.meter;
    let plan = MeterPlan::new(t, rect, labels, bars, face);
    let mut el = place(rect);
    for mark in &plan.marks {
        el = el.child(
            place(mark.rect)
                .flex()
                .justify_end()
                .text_size(px(MIN_TEXT_PX))
                .line_height(px(12.))
                .text_color(paint(t.text_secondary))
                .child(mark.text),
        );
    }
    for side in &plan.sides {
        el = el.child(
            border(place(side.well), t.border_width)
                .border_color(paint(m.border))
                .bg(paint(m.well))
                .rounded(px(m.radius)),
        );
        let quads = side.quads.clone();
        let colours: Vec<Rgba> = quads.iter().map(|q| quad_colour(&m, q.role)).collect();
        el = el.child(
            place(side.area).child(
                canvas(
                    |_, _, _| (),
                    move |bounds: Bounds<gpui::Pixels>, _, window, _| {
                        for (q, colour) in quads.iter().zip(&colours) {
                            let r = Bounds::new(
                                point(
                                    bounds.origin.x + px(q.rect.x as f32),
                                    bounds.origin.y + px(q.rect.y as f32),
                                ),
                                size(px(q.rect.w as f32), px(q.rect.h as f32)),
                            );
                            window.paint_quad(fill(r, *colour));
                        }
                    },
                )
                .size_full(),
            ),
        );
        el = el.child(
            place(side.lamp)
                .bg(paint(if side.lamp_lit { m.lamp_on } else { m.lamp_off }))
                .rounded(px(m.radius)),
        );
    }
    if let (Some(r), Some(text)) = (plan.readout, readout) {
        el = el.child(
            place(r)
                .flex()
                .justify_center()
                .text_size(px(MIN_TEXT_PX))
                .line_height(px(12.))
                .text_color(paint(t.text_secondary))
                .child(text),
        );
    }
    el.on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            on_click(this);
            cx.notify();
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_ui::theme::ThemeTokens;

    fn tokens() -> ThemeTokens {
        ThemeTokens::builtin_dark()
    }

    fn with_thresholds(mid: f32, high: f32) -> ThemeTokens {
        let mut t = tokens();
        t.meter_mid_db = mid;
        t.meter_high_db = high;
        t
    }

    /// 239 px at 4+1 gives 48 segments: exactly 1 dB each from -48, so thresholds land on tops.
    const ONE_DB_HEIGHT: i32 = 239;

    #[test]
    fn segment_count_for_height() {
        let t = tokens();
        assert_eq!(SegmentModel::new(&t, 160).count(), 32);
        assert_eq!(SegmentModel::new(&t, 4).count(), 1);
        assert_eq!(SegmentModel::new(&t, 3).count(), 0);
        assert_eq!(SegmentModel::new(&t, 0).count(), 0);
        assert_eq!(SegmentModel::new(&t, -5).count(), 0);
        for h in 4..400 {
            assert!(SegmentModel::new(&t, h).count() >= 1);
        }
        // Never panics for a tiny bar, whatever is asked of it.
        let tiny = SegmentModel::new(&t, 2);
        assert_eq!(tiny.lit_count(1.0), 0);
        assert!(tiny.lit_runs(1.0).iter().all(Option::is_none));
        assert!(tiny.hold_run(1.0).is_none());
        assert_eq!(tiny.gap_ys().count(), 0);
    }

    #[test]
    fn segment_top_db_is_linear_in_position() {
        let t = tokens();
        let m = SegmentModel::new(&t, ONE_DB_HEIGHT);
        assert_eq!(m.count(), 48);
        assert!(
            (m.segment_top_db(47) - 0.0).abs() < 1e-4,
            "the top segment ends at 0 dB"
        );
        assert!((m.segment_top_db(0) - -47.0).abs() < 1e-4);
        for i in 1..48 {
            let step = m.segment_top_db(i) - m.segment_top_db(i - 1);
            assert!((step - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn segment_colour_thresholds() {
        for (mid, high) in [(-18.0, -6.0), (-12.0, -3.0)] {
            let t = with_thresholds(mid, high);
            let m = SegmentModel::new(&t, ONE_DB_HEIGHT);
            let tops = |i: usize| m.segment_top_db(i).round() as i32;
            for i in 0..48 {
                let db = tops(i) as f32;
                let expected = if db >= high {
                    MeterZone::High
                } else if db >= mid {
                    MeterZone::Mid
                } else {
                    MeterZone::Low
                };
                assert_eq!(m.zone(i), expected, "segment top {db} dB with {mid}/{high}");
            }
            // The boundary segments themselves.
            let at = |db: f32| (db + 48.0) as usize - 1;
            assert_eq!(m.zone(at(mid)), MeterZone::Mid, "exactly {mid} dB is amber");
            assert_eq!(m.zone(at(mid) - 1), MeterZone::Low);
            assert_eq!(
                m.zone(at(high)),
                MeterZone::High,
                "exactly {high} dB is red"
            );
            assert_eq!(m.zone(at(high) - 1), MeterZone::Mid);
        }
    }

    #[test]
    fn lit_count_matches_level() {
        let t = tokens();
        let m = SegmentModel::new(&t, 159); // 32 segments
        assert_eq!(m.count(), 32);
        assert_eq!(m.lit_count(0.0), 0);
        assert_eq!(m.lit_count(1.0), 32);
        assert_eq!(m.lit_count(0.5), 16);
        assert_eq!(m.lit_count(1.5), 32, "clamped");
        assert_eq!(m.lit_count(-1.0), 0);
        assert_eq!(
            m.lit_count(0.5 + 0.01),
            17,
            "just above a boundary lights the next segment"
        );
    }

    #[test]
    fn quiet_signal_lights_first_segment() {
        let t = tokens();
        let m = SegmentModel::new(&t, ONE_DB_HEIGHT);
        let level = (-47.0 - t.meter_floor_db) / -t.meter_floor_db;
        assert_eq!(m.lit_count(level), 1);
        assert_eq!(m.lit_count(1e-6), 1);
    }

    #[test]
    fn hold_segment_index_matches_hold_level_and_has_zone_colour() {
        let t = tokens();
        let m = SegmentModel::new(&t, ONE_DB_HEIGHT);
        assert_eq!(m.hold_index(0.0), None);
        assert_eq!(m.hold_index(1.0), Some(47));
        let run = m.hold_run(1.0).expect("a full-scale hold");
        assert_eq!(run.zone, MeterZone::High);
        assert_eq!((run.count, run.h), (1, 4));
        assert_eq!(run.y, m.segment_y(47));
        let low = m.hold_run(0.2).expect("a low hold");
        assert_eq!(low.zone, MeterZone::Low);
        assert_eq!(m.hold_index(0.2), Some(m.lit_count(0.2) - 1));
    }

    #[test]
    fn runs_of_lit_segments_are_coalesced_per_zone() {
        let t = tokens();
        let m = SegmentModel::new(&t, ONE_DB_HEIGHT);
        let runs = m.lit_runs(1.0);
        assert!(
            runs.iter().all(Option::is_some),
            "all three zones at full level"
        );
        let total: usize = runs.iter().flatten().map(|r| r.count).sum();
        assert_eq!(total, 48);
        let low = runs[0].unwrap();
        assert_eq!(low.first, 0);
        assert_eq!(
            low.h,
            low.count as i32 * 5 - 1,
            "segments plus the gaps between them"
        );
        // The run stack is contiguous: each run ends where the next begins (less the gap).
        let (mid, high) = (runs[1].unwrap(), runs[2].unwrap());
        assert_eq!(mid.y + mid.h + 1, low.y);
        assert_eq!(high.y + high.h + 1, mid.y);
        // A quiet bar lights one zone only; silence lights none.
        let quiet = m.lit_runs(0.1);
        assert_eq!(quiet.iter().flatten().count(), 1);
        assert!(m.lit_runs(0.0).iter().all(Option::is_none));
    }

    #[test]
    fn gaps_sit_between_segments() {
        let t = tokens();
        let m = SegmentModel::new(&t, ONE_DB_HEIGHT);
        let gaps: Vec<i32> = m.gap_ys().collect();
        assert_eq!(gaps.len(), 47);
        for (i, y) in gaps.iter().enumerate() {
            assert_eq!(
                *y,
                m.segment_y(i + 1) + 4,
                "gap {i} is just under segment {}",
                i + 1
            );
            assert_eq!(*y + 1, m.segment_y(i), "and just over segment {i}");
        }
        assert_eq!(m.segment_y(0) + 4, m.stack_height() + m.pad_top());
    }

    fn bars(level: f32, hold: f32, clip_l: bool, clip_r: bool) -> [BarState; 2] {
        [
            BarState {
                level,
                hold,
                clipping: clip_l,
            },
            BarState {
                level,
                hold,
                clipping: clip_r,
            },
        ]
    }

    #[test]
    fn element_budget() {
        let t = tokens();
        for h in [60, 96, 160, 240, 400, 700] {
            for (face, labels) in [
                (MeterFace::channel(&t), true),
                (MeterFace::channel(&t), false),
                (MeterFace::master(&t), true),
            ] {
                let plan = MeterPlan::new(
                    &t,
                    Rect::new(0, 0, 60, h),
                    labels,
                    bars(1.0, 1.0, true, true),
                    face,
                );
                assert!(plan.div_count() <= 24, "h={h}: {} boxes", plan.div_count());
                // Quads are cheap, but bounded by the segment count anyway.
                let segs = SegmentModel::new(&t, h).count();
                assert!(
                    plan.quad_count() <= 2 * (segs + 4),
                    "h={h}: {} quads",
                    plan.quad_count()
                );
            }
        }
    }

    #[test]
    fn master_clip_lamp_lights_when_either_side_clips() {
        let t = tokens();
        let rect = Rect::new(0, 0, 60, 200);
        let master = MeterPlan::new(
            &t,
            rect,
            false,
            bars(0.5, 0.5, true, false),
            MeterFace::master(&t),
        );
        assert!(master.sides[0].lamp_lit && master.sides[1].lamp_lit);
        let none = MeterPlan::new(
            &t,
            rect,
            false,
            bars(0.5, 0.5, false, false),
            MeterFace::master(&t),
        );
        assert!(!none.sides[0].lamp_lit && !none.sides[1].lamp_lit);
        // A channel meter keeps one lamp per side.
        let ch = MeterPlan::new(
            &t,
            rect,
            false,
            bars(0.5, 0.5, true, false),
            MeterFace::channel(&t),
        );
        assert!(ch.sides[0].lamp_lit && !ch.sides[1].lamp_lit);
    }

    #[test]
    fn master_lamp_is_taller_and_off_colour_differs_from_the_well() {
        let t = tokens();
        assert!(MeterFace::master(&t).lamp_h > MeterFace::channel(&t).lamp_h);
        let m = LookAndFeel::from_tokens(&t).meter;
        assert_ne!(m.lamp_off, m.well);
        assert_ne!(m.lamp_off, m.lamp_on);
    }

    #[test]
    fn labels_are_static_and_fit() {
        let t = tokens();
        let plan = MeterPlan::new(
            &t,
            Rect::new(0, 0, 60, 200),
            true,
            bars(0.0, 0.0, false, false),
            MeterFace::channel(&t),
        );
        assert!(plan.marks.len() >= 3 && plan.marks[0].text == "0");
        for pair in plan.marks.windows(2) {
            assert!(pair[1].rect.y - pair[0].rect.y >= MIN_MARK_SPACING);
        }
        let none = MeterPlan::new(
            &t,
            Rect::new(0, 0, 60, 200),
            false,
            bars(0.0, 0.0, false, false),
            MeterFace::channel(&t),
        );
        assert!(none.marks.is_empty());
    }

    #[test]
    fn the_scale_keeps_clear_of_the_bars() {
        let t = tokens();
        let plan = MeterPlan::new(
            &t,
            Rect::new(0, 0, 60, 200),
            true,
            bars(0.0, 0.0, false, false),
            MeterFace::channel(&t),
        );
        let bar_x = plan.sides[0].well.x;
        assert!(!plan.marks.is_empty());
        for mark in &plan.marks {
            let gap = bar_x - (mark.rect.x + mark.rect.w);
            assert!(
                gap >= SCALE_GAP,
                "'{}' ends {gap} px from the bars",
                mark.text
            );
        }
    }

    #[test]
    fn peak_text_is_one_decimal_db() {
        let t = tokens();
        assert_eq!(peak_text(&t, bars(0.0, 0.0, false, false)), "-inf");
        assert_eq!(peak_text(&t, bars(0.0, 1.0, false, false)), "0.0");
        let hold = (-6.0 - t.meter_floor_db) / -t.meter_floor_db;
        assert_eq!(peak_text(&t, bars(0.0, hold, false, false)), "-6.0");
    }

    /// Perf gate (plan Phase 4): the CPU cost of planning every meter of the mixer at full level,
    /// each frame. Prints figures; run with
    /// `cargo test -p om-ui-gpui --lib perf_gate -- --ignored --nocapture`.
    #[test]
    #[ignore = "prints timings; not a pass/fail test"]
    fn perf_gate_plan_cost() {
        let t = tokens();
        let rect = Rect::new(0, 0, 60, 240);
        let full = bars(1.0, 1.0, true, true);
        let frames = 2000;
        let start = std::time::Instant::now();
        let mut boxes = 0;
        let mut quads = 0;
        for _ in 0..frames {
            // Four channel meters and the master, as the mixer draws them.
            for face in [
                MeterFace::channel(&t),
                MeterFace::channel(&t),
                MeterFace::channel(&t),
                MeterFace::channel(&t),
                MeterFace::master(&t),
            ] {
                let plan = MeterPlan::new(&t, rect, true, full, face);
                boxes = boxes.max(plan.div_count());
                quads = quads.max(plan.quad_count());
            }
        }
        let per_frame = start.elapsed().as_secs_f64() * 1000.0 / frames as f64;
        eprintln!("perf gate: 5 meters planned in {per_frame:.4} ms/frame (debug build); max {boxes} boxes and {quads} quads per meter");
    }
}
