//! Fader painting: the taper maths, the tick model, the cap and slot geometry, and the GPUI
//! painters for the channel/master fader, the pitch fader and the crossfader.
//!
//! Plan: `docs/superpowers/plans/2026-10-24-ui-faders-meters.md` (option 3 of its taper decision).
//! The stored parameter stays a LINEAR gain; a [`FaderLaw`] maps it to where the cap sits on screen
//! and back. Nothing here changes what a stored value sounds like. Conventions only (ADR 0001).
//!
//! The maths and the geometry are pure (no GPUI) and unit tested; the painters only turn a
//! [`FaderPlan`] into boxes, so the element count is testable too ([`FaderPlan::div_count`]).

use crate::app::Shell;
use crate::controls::draggable;
use crate::drag::Target;
use crate::paint::{paint, place, skin};
use gpui::{div, prelude::*, px, Context, Div};
use om_ui::format;
use om_ui::geometry::Rect;
use om_ui::meter::LABEL_GUTTER;
use om_ui::style::LookAndFeel;
use om_ui::theme::{Colour, ThemeTokens};

/// Smallest text drawn on a control, in px.
pub const MIN_TEXT_PX: f32 = 12.0;
/// Ticks closer than this (px along the travel) are dropped, loudest kept. The unity tick is
/// always kept. Same idea as `om_ui::meter::MIN_MARK_SPACING`, with the plan's 12 px.
pub const MIN_TICK_SPACING: f32 = 12.0;

/// Top of a master fader in dB: the stored maximum gain 2.0 (`MAX_GAIN`), exactly +6.02 dB.
const MASTER_TOP_DB: f32 = 6.020_6;
/// Law knots `(position, dB)`, bottom first. Below the first knot the gain ramps linearly from
/// silence; between knots the law is linear in dB. Channel: plan section 2 (`ChannelLaw`).
const CHANNEL_KNOTS: [(f32, f32); 2] = [(0.2, -48.0), (1.0, 0.0)];
/// Master: plan section 2 (`MasterLaw`): +6 at the top, 0 dB at 0.75, -30 dB at 0.25.
const MASTER_KNOTS: [(f32, f32); 3] = [(0.25, -30.0), (0.75, 0.0), (1.0, MASTER_TOP_DB)];

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * gain.log10()
    }
}

fn gain_from_knots(knots: &[(f32, f32)], pos: f32) -> f32 {
    let pos = pos.clamp(0.0, 1.0);
    let (p0, d0) = knots[0];
    if pos <= p0 {
        return db_to_gain(d0) * pos / p0;
    }
    for pair in knots.windows(2) {
        let ((pa, da), (pb, db)) = (pair[0], pair[1]);
        if pos <= pb {
            return db_to_gain(da + (db - da) * (pos - pa) / (pb - pa));
        }
    }
    db_to_gain(knots[knots.len() - 1].1)
}

fn pos_from_knots(knots: &[(f32, f32)], gain: f32) -> f32 {
    if gain <= 0.0 {
        return 0.0;
    }
    let (p0, d0) = knots[0];
    let g0 = db_to_gain(d0);
    if gain <= g0 {
        return p0 * gain / g0;
    }
    let db = gain_to_db(gain);
    for pair in knots.windows(2) {
        let ((pa, da), (pb, db_b)) = (pair[0], pair[1]);
        if db <= db_b {
            return pa + (pb - pa) * (db - da) / (db_b - da);
        }
    }
    1.0
}

/// How a fader's travel maps to the stored gain. The cap position `pos` is `0..=1` of travel from
/// the bottom; the gain is in stored (linear) units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FaderLaw {
    /// Today's behaviour: gain is proportional to travel, up to `max_gain` (1.0 channel, 2.0
    /// master). The Phase 2 default; the ticks sit at the true dB of the linear travel.
    Linear { max_gain: f32 },
    /// Channel fader (stored 0..1): dB-linear from 0 dB at the top to -48 dB at 0.2, then a
    /// linear gain ramp to silence.
    Channel,
    /// Master fader (stored 0..2): +6 dB at the top, 0 dB at 0.75, -30 dB at 0.25, ramp below.
    Master,
}

impl FaderLaw {
    /// The largest stored gain (the top of travel).
    pub fn max_gain(self) -> f32 {
        match self {
            FaderLaw::Linear { max_gain } => max_gain,
            FaderLaw::Channel => 1.0,
            FaderLaw::Master => 2.0,
        }
    }

    /// Stored gain at cap position `pos`.
    pub fn gain_at(self, pos: f32) -> f32 {
        match self {
            FaderLaw::Linear { max_gain } => pos.clamp(0.0, 1.0) * max_gain,
            FaderLaw::Channel => gain_from_knots(&CHANNEL_KNOTS, pos),
            FaderLaw::Master => gain_from_knots(&MASTER_KNOTS, pos).min(2.0),
        }
    }

    /// Cap position for a stored gain; the inverse of [`FaderLaw::gain_at`], clamped to `0..=1`.
    pub fn pos_of(self, gain: f32) -> f32 {
        match self {
            FaderLaw::Linear { max_gain } if max_gain > 0.0 => (gain / max_gain).clamp(0.0, 1.0),
            FaderLaw::Linear { .. } => 0.0,
            FaderLaw::Channel => pos_from_knots(&CHANNEL_KNOTS, gain).clamp(0.0, 1.0),
            FaderLaw::Master => pos_from_knots(&MASTER_KNOTS, gain).clamp(0.0, 1.0),
        }
    }

    /// dB at cap position `pos`; `-inf` at silence.
    pub fn db_at(self, pos: f32) -> f32 {
        gain_to_db(self.gain_at(pos))
    }

    /// Cap position of a dB value (`-inf` gives 0).
    pub fn pos_at_db(self, db: f32) -> f32 {
        if db == f32::NEG_INFINITY {
            0.0
        } else {
            self.pos_of(db_to_gain(db))
        }
    }
}

/// The popup text for a stored gain: `-inf` at silence, else whole dB (`-6 dB`). Agrees with
/// `om_ui::control::Readout::Gain`.
pub fn db_text(gain: f32) -> String {
    if gain <= 0.0 {
        "-inf".to_owned()
    } else {
        format::whole_decibels(f64::from(gain_to_db(gain)))
    }
}

/// One scale mark beside a fader.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    /// Cap position the mark stands for, `0..=1` from the bottom.
    pub pos: f32,
    pub db: f32,
    /// Long mark, and the only kind that carries a label.
    pub major: bool,
    /// The 0 dB mark: always kept, drawn brightest.
    pub unity: bool,
    pub label: &'static str,
}

/// Channel marks (plan Task 1.2, R section 3 numbers; the dB choice is a proposal).
const CHANNEL_TICKS: [(f32, &str); 5] = [
    (0.0, "0"),
    (-6.0, "-6"),
    (-12.0, "-12"),
    (-24.0, "-24"),
    (-48.0, "-48"),
];
/// Master marks (same provenance). Unsourced: which marks are major.
const MASTER_TICKS: [(f32, &str); 6] = [
    (6.0, "+6"),
    (0.0, "0"),
    (-6.0, "-6"),
    (-12.0, "-12"),
    (-24.0, "-24"),
    (-48.0, "-48"),
];

/// The marks for `law` on a fader whose cap travels `travel_px`, loudest first. A mark closer than
/// [`MIN_TICK_SPACING`] to one already kept is dropped; the unity mark is never dropped. Major
/// marks (0 dB, the top mark and multiples of 12 dB) carry the labels.
pub fn ticks(law: FaderLaw, travel_px: f32) -> Vec<Tick> {
    let table: &[(f32, &'static str)] = match law {
        FaderLaw::Master => &MASTER_TICKS,
        FaderLaw::Channel => &CHANNEL_TICKS,
        FaderLaw::Linear { max_gain } if max_gain > 1.0 => &MASTER_TICKS,
        FaderLaw::Linear { .. } => &CHANNEL_TICKS,
    };
    let mut all: Vec<Tick> = Vec::with_capacity(table.len());
    for (i, &(db, label)) in table.iter().enumerate() {
        if db_to_gain(db) > law.max_gain() * 1.001 {
            continue;
        }
        all.push(Tick {
            pos: law.pos_at_db(db),
            db,
            major: db == 0.0 || db % 12.0 == 0.0 || i == 0,
            unity: db == 0.0,
            label,
        });
    }
    let mut kept: Vec<Tick> = all.iter().copied().filter(|t| t.unity).collect();
    for tick in all.iter().copied().filter(|t| !t.unity) {
        let clear = kept
            .iter()
            .all(|k| (k.pos - tick.pos).abs() * travel_px >= MIN_TICK_SPACING - 0.01);
        if clear {
            kept.push(tick);
        }
    }
    kept.sort_by(|a, b| b.pos.total_cmp(&a.pos));
    kept
}

/// Cap position of a pitch `rate` (1.0 = original tempo) on a fader spanning `1 +- range`.
pub fn pitch_pos(rate: f64, range: f64) -> f32 {
    if range <= 0.0 {
        return 0.5;
    }
    (((rate - (1.0 - range)) / (2.0 * range)) as f32).clamp(0.0, 1.0)
}

/// The pitch readout, one decimal and signed (`+1.2%`, `0.0%`, `-8.0%`): the existing
/// `om_ui::format::rate`, reused rather than duplicated. (The plan's text has a space before `%`.)
pub fn pitch_readout(rate: f64) -> String {
    format::rate(rate)
}

/// Where the pitch scale's marks sit: the centre notch (rate 1.0) and a minor mark at half range
/// either side. Positions are cap positions.
pub const PITCH_NOTCH_POS: f32 = 0.5;
pub const PITCH_MINOR_POS: [f32; 2] = [0.25, 0.75];

/// Crossfader mark positions as fractions of the cap's travel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrossfaderMarks {
    pub a: f32,
    pub centre: f32,
    pub b: f32,
}

pub fn crossfader_marks() -> CrossfaderMarks {
    CrossfaderMarks {
        a: 0.0,
        centre: 0.5,
        b: 1.0,
    }
}

/// What a vertical fader looks like: its law, cap and slot size, and which scale it carries.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaderFace {
    pub law: FaderLaw,
    pub scale: FaderScale,
    pub cap_w: i32,
    pub cap_h: i32,
    pub slot_w: i32,
}

/// The scale printed beside the slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaderScale {
    /// dB marks from [`ticks`].
    Gain,
    /// Centre notch and half-range minors.
    Pitch,
}

impl FaderFace {
    /// Channel fader: slot 4 px, cap 22x36 (R section 3; sizes are proposals).
    pub fn channel(law: FaderLaw) -> Self {
        Self {
            law,
            scale: FaderScale::Gain,
            cap_w: 22,
            cap_h: 36,
            slot_w: 4,
        }
    }

    /// Master fader: slot 6 px, cap 26x40.
    pub fn master(law: FaderLaw) -> Self {
        Self {
            law,
            scale: FaderScale::Gain,
            cap_w: 26,
            cap_h: 40,
            slot_w: 6,
        }
    }

    /// Pitch fader: slot 4 px, cap 22x36, centre notch. Its position is linear in the rate (see
    /// [`pitch_pos`]), so the law is the identity.
    pub fn pitch() -> Self {
        Self {
            law: FaderLaw::Linear { max_gain: 1.0 },
            scale: FaderScale::Pitch,
            cap_w: 22,
            cap_h: 36,
            slot_w: 4,
            // The range only changes the printed percentages, not the geometry.
        }
    }
}

const MAJOR_TICK_PX: i32 = 10;
const MINOR_TICK_PX: i32 = 6;
const NOTCH_W: i32 = 12;
const NOTCH_H: i32 = 2;
const CAP_LINE_H: i32 = 2;
/// Gap between a tick's end and the cap, in px.
const TICK_GAP: i32 = 2;

/// Geometry of a vertical fader in `w` x `h` px (coordinates local to the fader). The slot spans the
/// full height; the cap travels `h - cap_h` inside it, so it never leaves the slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaderGeometry {
    pub w: i32,
    pub h: i32,
    pub cap_x: i32,
    pub cap_w: i32,
    pub cap_h: i32,
    pub slot: Rect,
}

impl FaderGeometry {
    /// The cap sits flush right with the scale to its left; a fader too narrow for the cap shrinks
    /// the cap to fit.
    pub fn new(w: i32, h: i32, face: &FaderFace) -> Self {
        let cap_w = face.cap_w.min(w).max(1);
        let cap_h = face.cap_h.min(h).max(1);
        let cap_x = (w - cap_w).max(0);
        let slot_w = face.slot_w.min(cap_w).max(1);
        Self {
            w,
            h,
            cap_x,
            cap_w,
            cap_h,
            slot: Rect::new(cap_x + (cap_w - slot_w) / 2, 0, slot_w, h.max(1)),
        }
    }

    /// Cap travel in px.
    pub fn travel(&self) -> i32 {
        (self.h - self.cap_h).max(0)
    }

    /// Top of the cap at position `pos`.
    pub fn cap_y(&self, pos: f32) -> i32 {
        ((1.0 - pos.clamp(0.0, 1.0)) * self.travel() as f32).round() as i32
    }

    pub fn cap(&self, pos: f32) -> Rect {
        Rect::new(self.cap_x, self.cap_y(pos), self.cap_w, self.cap_h)
    }

    /// The 2 px line across the cap's middle.
    pub fn cap_line(&self, pos: f32) -> Rect {
        let y = self.cap_y(pos) + self.cap_h / 2 - CAP_LINE_H / 2;
        Rect::new(self.cap_x + 3, y, (self.cap_w - 6).max(1), CAP_LINE_H)
    }

    /// Row of a 1 px tick for position `pos`: the lower row of the cap's centre line, so a tick at
    /// the cap's position reads as level with its line.
    pub fn tick_y(&self, pos: f32) -> i32 {
        self.cap_y(pos) + self.cap_h / 2
    }
}

/// One painted tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickRect {
    pub rect: Rect,
    pub unity: bool,
}

/// One painted label.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LabelRect {
    pub rect: Rect,
    pub text: &'static str,
    pub unity: bool,
}

/// Everything a vertical fader draws, as plain rectangles.
#[derive(Debug, Clone, PartialEq)]
pub struct FaderPlan {
    pub geometry: FaderGeometry,
    pub ticks: Vec<TickRect>,
    pub labels: Vec<LabelRect>,
    pub cap: Rect,
    pub cap_line: Rect,
    /// The pitch centre notch.
    pub notch: Option<Rect>,
}

impl FaderPlan {
    /// Plans a fader in `w` x `h` px with the cap at `pos`.
    pub fn new(w: i32, h: i32, face: &FaderFace, pos: f32) -> Self {
        let g = FaderGeometry::new(w, h, face);
        let room = g.cap_x - TICK_GAP;
        let mut ticks = Vec::new();
        let mut labels = Vec::new();
        let mut notch = None;
        let label_room = room - MAJOR_TICK_PX;
        let tick_at = |p: f32, len: i32, unity: bool, out: &mut Vec<TickRect>| {
            let len = len.min(room);
            if len > 0 {
                out.push(TickRect {
                    rect: Rect::new(room - len, g.tick_y(p), len, 1),
                    unity,
                });
            }
        };
        match face.scale {
            FaderScale::Gain => {
                for tick in ticks_for(face, g.travel() as f32) {
                    let len = if tick.major {
                        MAJOR_TICK_PX
                    } else {
                        MINOR_TICK_PX
                    };
                    tick_at(tick.pos, len, tick.unity, &mut ticks);
                    if tick.major && label_room >= LABEL_GUTTER {
                        labels.push(LabelRect {
                            rect: Rect::new(0, g.tick_y(tick.pos) - 6, label_room, 12),
                            text: tick.label,
                            unity: tick.unity,
                        });
                    }
                }
            }
            FaderScale::Pitch => {
                for p in PITCH_MINOR_POS {
                    tick_at(p, MINOR_TICK_PX, false, &mut ticks);
                }
                let w = NOTCH_W.min(room);
                if w > 0 {
                    notch = Some(Rect::new(room - w, g.tick_y(PITCH_NOTCH_POS), w, NOTCH_H));
                }
            }
        }
        Self {
            geometry: g,
            ticks,
            labels,
            cap: g.cap(pos),
            cap_line: g.cap_line(pos),
            notch,
        }
    }

    /// Boxes the painter emits: root, slot, cap and line, plus ticks, labels and the notch.
    pub fn div_count(&self) -> usize {
        4 + self.ticks.len() + self.labels.len() + usize::from(self.notch.is_some())
    }
}

fn ticks_for(face: &FaderFace, travel: f32) -> Vec<Tick> {
    ticks(face.law, travel)
}

/// Paints a vertical fader in `rect` (parent coordinates) with its cap at `pos`. `accent` colours
/// the cap line while dragging (a popup being shown means a drag is in progress) and the pitch
/// notch. Made draggable with `abs`.
#[allow(clippy::too_many_arguments)]
pub fn fader(
    t: &ThemeTokens,
    rect: Rect,
    face: &FaderFace,
    pos: f32,
    accent: Colour,
    target: Target,
    abs: Rect,
    popup_text: Option<&str>,
    cx: &mut Context<Shell>,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let plan = FaderPlan::new(rect.w, rect.h, face, pos);
    let dragging = popup_text.is_some();
    let mut root = place(rect);
    root = root.child(skin(place(plan.geometry.slot), &look.fader.track));
    for tick in &plan.ticks {
        let colour = if tick.unity {
            t.text_primary
        } else {
            t.text_muted
        };
        root = root.child(place(tick.rect).bg(paint(colour)));
    }
    for label in &plan.labels {
        let colour = if label.unity {
            t.text_primary
        } else {
            t.text_secondary
        };
        root = root.child(
            place(label.rect)
                .flex()
                .justify_end()
                .text_size(px(MIN_TEXT_PX))
                .line_height(px(12.))
                .text_color(paint(colour))
                .child(label.text),
        );
    }
    if let Some(notch) = plan.notch {
        root = root.child(place(notch).bg(paint(accent)));
    }
    root = root.child(skin(place(plan.cap), &look.fader.thumb));
    root = root.child(place(plan.cap_line).bg(paint(if dragging {
        accent
    } else {
        t.text_secondary
    })));
    if let Some(text) = popup_text {
        root = root.child(popup_chip(t, rect.w, text));
    }
    draggable(root, target, abs, cx)
}

/// Crossfader cap size (R section 3; a proposal) and slot thickness.
const XF_CAP_W: i32 = 28;
const XF_CAP_H: i32 = 20;
const XF_SLOT_H: i32 = 4;
const XF_CENTRE_TICK: i32 = 10;
const XF_END_TICK: i32 = 6;

/// Geometry of the horizontal crossfader in `w` x `h` px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrossfaderGeometry {
    pub w: i32,
    pub h: i32,
    pub cap_w: i32,
    pub cap_h: i32,
    pub slot: Rect,
}

impl CrossfaderGeometry {
    pub fn new(w: i32, h: i32) -> Self {
        let cap_w = XF_CAP_W.min(w).max(1);
        let cap_h = XF_CAP_H.min(h).max(1);
        let slot_h = XF_SLOT_H.min(h).max(1);
        Self {
            w,
            h,
            cap_w,
            cap_h,
            slot: Rect::new(0, (h - slot_h) / 2, w.max(1), slot_h),
        }
    }

    pub fn travel(&self) -> i32 {
        (self.w - self.cap_w).max(0)
    }

    pub fn cap_x(&self, pos: f32) -> i32 {
        (pos.clamp(0.0, 1.0) * self.travel() as f32).round() as i32
    }

    pub fn cap(&self, pos: f32) -> Rect {
        Rect::new(
            self.cap_x(pos),
            (self.h - self.cap_h) / 2,
            self.cap_w,
            self.cap_h,
        )
    }

    /// Column of the cap's centre line (and of a mark at `pos`).
    pub fn mark_x(&self, pos: f32) -> i32 {
        self.cap_x(pos) + self.cap_w / 2
    }

    pub fn cap_line(&self, pos: f32) -> Rect {
        let cap = self.cap(pos);
        Rect::new(
            self.mark_x(pos) - 1,
            cap.y + 3,
            CAP_LINE_H,
            (cap.h - 6).max(1),
        )
    }
}

/// Everything the crossfader draws.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossfaderPlan {
    pub geometry: CrossfaderGeometry,
    /// End marks (A, B) and the centre mark, as 1 px wide columns centred on the slot.
    pub end_ticks: [Rect; 2],
    pub centre_tick: Rect,
    pub cap: Rect,
    pub cap_line: Rect,
}

impl CrossfaderPlan {
    pub fn new(w: i32, h: i32, pos: f32) -> Self {
        let g = CrossfaderGeometry::new(w, h);
        let marks = crossfader_marks();
        let mid = g.slot.y + g.slot.h / 2;
        let tick = |p: f32, len: i32| Rect::new(g.mark_x(p), mid - len / 2, 1, len);
        Self {
            geometry: g,
            end_ticks: [tick(marks.a, XF_END_TICK), tick(marks.b, XF_END_TICK)],
            centre_tick: tick(marks.centre, XF_CENTRE_TICK),
            cap: g.cap(pos),
            cap_line: g.cap_line(pos),
        }
    }

    pub fn div_count(&self) -> usize {
        // root, slot, 3 ticks, cap, cap line
        7
    }
}

/// Paints the horizontal crossfader in `rect` with its cap at `pos` (`0` = A, `1` = B).
#[allow(clippy::too_many_arguments)]
pub fn crossfader(
    t: &ThemeTokens,
    rect: Rect,
    pos: f32,
    accent: Colour,
    target: Target,
    abs: Rect,
    popup_text: Option<&str>,
    cx: &mut Context<Shell>,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let plan = CrossfaderPlan::new(rect.w, rect.h, pos);
    let mut root = place(rect).child(skin(place(plan.geometry.slot), &look.fader.track));
    for tick in plan.end_ticks {
        root = root.child(place(tick).bg(paint(t.text_muted)));
    }
    root = root
        .child(place(plan.centre_tick).bg(paint(t.text_primary)))
        .child(skin(place(plan.cap), &look.fader.thumb))
        .child(place(plan.cap_line).bg(paint(if popup_text.is_some() {
            accent
        } else {
            t.text_secondary
        })));
    if let Some(text) = popup_text {
        root = root.child(popup_chip(t, rect.w, text));
    }
    draggable(root, target, abs, cx)
}

/// A value popup centred above a fader `width` px wide. No pointer handling.
fn popup_chip(t: &ThemeTokens, width: i32, text: &str) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let chip = skin(div().px(px(6.)).py(px(1.)), &look.tooltip)
        .text_size(px(t.type_caption.max(MIN_TEXT_PX)))
        .whitespace_nowrap()
        .child(text.to_owned());
    div()
        .absolute()
        .left(px(0.))
        .top(px(-26.))
        .w(px(width as f32))
        .flex()
        .justify_center()
        .child(chip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_ui::widgets::pitch::RANGES;

    fn approx(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    const LAWS: [FaderLaw; 4] = [
        FaderLaw::Channel,
        FaderLaw::Master,
        FaderLaw::Linear { max_gain: 1.0 },
        FaderLaw::Linear { max_gain: 2.0 },
    ];

    #[test]
    fn channel_law_pos_one_is_unity_and_pos_zero_is_silent() {
        let l = FaderLaw::Channel;
        assert!(approx(l.gain_at(1.0), 1.0, 1e-6));
        assert_eq!(l.gain_at(0.0), 0.0);
        assert!(approx(l.db_at(0.2), -48.0, 1e-3), "the knee is -48 dB");
        assert_eq!(l.db_at(0.0), f32::NEG_INFINITY);
    }

    #[test]
    fn master_law_unity_at_three_quarters_and_plus_six_at_top() {
        let l = FaderLaw::Master;
        assert!(approx(l.gain_at(0.75), 1.0, 1e-5));
        assert!(approx(l.gain_at(1.0), 2.0, 1e-4), "the stored maximum");
        assert!(approx(l.db_at(0.25), -30.0, 1e-3));
        assert_eq!(l.gain_at(0.0), 0.0);
    }

    #[test]
    fn law_round_trips() {
        for law in LAWS {
            for i in 0..=200 {
                let pos = i as f32 / 200.0;
                let back = law.pos_of(law.gain_at(pos));
                assert!(
                    approx(back, pos, 1e-4),
                    "{law:?} pos {pos} came back {back}"
                );
            }
        }
    }

    #[test]
    fn law_is_monotonic_and_continuous_at_the_ramp_join() {
        for law in LAWS {
            let mut last = -1.0;
            for i in 0..=1000 {
                let g = law.gain_at(i as f32 / 1000.0);
                assert!(g >= last, "{law:?} not monotonic at {i}");
                last = g;
            }
        }
        // Either side of each ramp join the gain differs by a hair, not a jump.
        for (law, join) in [(FaderLaw::Channel, 0.2), (FaderLaw::Master, 0.25)] {
            let below = law.gain_at(join - 1e-4);
            let above = law.gain_at(join + 1e-4);
            assert!(above - below < 1e-3, "{law:?}: {below} -> {above}");
        }
    }

    #[test]
    fn linear_law_is_the_identity() {
        let l = FaderLaw::Linear { max_gain: 1.0 };
        for i in 0..=20 {
            let p = i as f32 / 20.0;
            assert!(approx(l.gain_at(p), p, 1e-6));
            assert!(approx(l.pos_of(p), p, 1e-6));
        }
        assert!(approx(
            FaderLaw::Linear { max_gain: 2.0 }.gain_at(0.5),
            1.0,
            1e-6
        ));
    }

    #[test]
    fn gain_to_db_floor() {
        assert_eq!(gain_to_db(0.0), f32::NEG_INFINITY);
        assert_eq!(db_text(0.0), "-inf");
        assert_eq!(db_text(0.5), "-6 dB");
        assert_eq!(db_text(1.0), "0 dB");
        assert_eq!(FaderLaw::Channel.pos_at_db(f32::NEG_INFINITY), 0.0);
    }

    #[test]
    fn channel_ticks_are_at_the_planned_dbs() {
        let t = ticks(FaderLaw::Channel, 1000.0);
        let dbs: Vec<f32> = t.iter().map(|k| k.db).collect();
        assert_eq!(dbs, vec![0.0, -6.0, -12.0, -24.0, -48.0]);
        assert!(approx(t[0].pos, 1.0, 1e-4) && t[0].unity);
        assert!(approx(t[4].pos, 0.2, 1e-3));
    }

    #[test]
    fn master_ticks_are_at_the_planned_dbs() {
        let t = ticks(FaderLaw::Master, 1000.0);
        let dbs: Vec<f32> = t.iter().map(|k| k.db).collect();
        assert_eq!(dbs, vec![6.0, 0.0, -6.0, -12.0, -24.0, -48.0]);
        assert!(approx(t[1].pos, 0.75, 1e-4) && t[1].unity);
    }

    #[test]
    fn ticks_are_dropped_when_closer_than_12px() {
        let tight = ticks(FaderLaw::Channel, 60.0);
        for pair in tight.windows(2) {
            assert!((pair[0].pos - pair[1].pos) * 60.0 >= MIN_TICK_SPACING - 1e-3);
        }
        assert!(tight.len() < 5);
        assert!(tight.iter().any(|k| k.unity), "unity is always kept");
        // The linear travel bunches the quiet marks together at the bottom.
        let linear = ticks(FaderLaw::Linear { max_gain: 1.0 }, 140.0);
        assert!(linear.len() < 5 && linear.iter().any(|k| k.unity));
        // Unity survives even where a neighbour would crowd it out.
        assert!(ticks(FaderLaw::Master, 30.0).iter().any(|k| k.unity));
    }

    #[test]
    fn labels_only_on_major_ticks() {
        let t = ticks(FaderLaw::Master, 1000.0);
        let majors: Vec<&str> = t.iter().filter(|k| k.major).map(|k| k.label).collect();
        assert_eq!(majors, vec!["+6", "0", "-12", "-24", "-48"]);
        let plan = FaderPlan::new(60, 160, &FaderFace::master(FaderLaw::Master), 0.5);
        assert_eq!(plan.labels.len(), 5, "a wide fader labels its majors");
        assert!(plan.ticks.len() > plan.labels.len());
    }

    #[test]
    fn label_text_min_12px() {
        let plan = FaderPlan::new(60, 160, &FaderFace::channel(FaderLaw::Channel), 0.5);
        assert!(!plan.labels.is_empty());
        for l in &plan.labels {
            assert!(l.rect.h as f32 >= MIN_TEXT_PX, "label box {} px", l.rect.h);
        }
        // A fader too narrow for a label column prints unlabelled ticks.
        let narrow = FaderPlan::new(40, 160, &FaderFace::channel(FaderLaw::Channel), 0.5);
        assert!(narrow.labels.is_empty() && !narrow.ticks.is_empty());
    }

    #[test]
    fn pitch_notch_is_at_centre_of_travel() {
        for range in RANGES {
            assert!(approx(pitch_pos(1.0, range), PITCH_NOTCH_POS, 1e-6));
            assert!(approx(pitch_pos(1.0 + range, range), 1.0, 1e-6));
            assert!(approx(pitch_pos(1.0 - range, range), 0.0, 1e-6));
        }
    }

    #[test]
    fn pitch_readout_one_decimal_signed() {
        assert_eq!(pitch_readout(1.012), "+1.2%");
        assert_eq!(pitch_readout(1.0), "0.0%");
        assert_eq!(pitch_readout(0.92), "-8.0%");
    }

    #[test]
    fn pitch_minor_ticks_at_half_range() {
        assert_eq!(PITCH_MINOR_POS, [0.25, 0.75]);
        for range in RANGES {
            assert!(approx(pitch_pos(1.0 + range / 2.0, range), 0.75, 1e-6));
        }
        let plan = FaderPlan::new(60, 140, &FaderFace::pitch(), 0.5);
        assert_eq!(plan.ticks.len(), 2);
        let notch = plan.notch.expect("a pitch fader has a centre notch");
        assert_eq!(notch.w, 12);
        assert_eq!(notch.y, plan.geometry.tick_y(0.5));
    }

    #[test]
    fn crossfader_marks_are_a_centre_and_b() {
        let m = crossfader_marks();
        assert_eq!((m.a, m.centre, m.b), (0.0, 0.5, 1.0));
        let plan = CrossfaderPlan::new(200, 46, 0.5);
        let g = plan.geometry;
        assert_eq!(plan.centre_tick.x, g.mark_x(0.5));
        assert_eq!(plan.end_ticks[0].x, g.cap_w / 2);
        assert_eq!(plan.end_ticks[1].x, g.travel() + g.cap_w / 2);
        assert!(plan.centre_tick.h > plan.end_ticks[0].h);
    }

    #[test]
    fn crossfader_cap_stays_inside_the_slot_and_lines_up_with_marks() {
        for pos in [0.0, 0.5, 1.0] {
            let plan = CrossfaderPlan::new(200, 46, pos);
            let cap = plan.cap;
            assert!(cap.x >= 0 && cap.x + cap.w <= 200);
            assert_eq!(plan.cap_line.x + 1, plan.geometry.mark_x(pos));
        }
    }

    #[test]
    fn cap_stays_inside_track() {
        for face in [
            FaderFace::channel(FaderLaw::Channel),
            FaderFace::master(FaderLaw::Master),
        ] {
            let g = FaderGeometry::new(60, 140, &face);
            for pos in [0.0, 0.3, 1.0] {
                let cap = g.cap(pos);
                assert!(cap.y >= g.slot.y && cap.y + cap.h <= g.slot.y + g.slot.h);
                assert!(cap.x >= 0 && cap.x + cap.w <= 60);
            }
            assert_eq!(g.cap(1.0).y, 0);
            assert_eq!(g.cap(0.0).y + g.cap(0.0).h, 140);
        }
    }

    #[test]
    fn tick_y_equals_cap_centre_y() {
        let g = FaderGeometry::new(60, 140, &FaderFace::channel(FaderLaw::Channel));
        for i in 0..=20 {
            let pos = i as f32 / 20.0;
            let line = g.cap_line(pos);
            let y = g.tick_y(pos);
            assert!(
                y >= line.y && y < line.y + line.h,
                "tick row {y} outside the cap line {}..{}",
                line.y,
                line.y + line.h
            );
        }
    }

    #[test]
    fn element_budget() {
        for (w, h) in [(40, 96), (60, 160), (80, 400)] {
            for face in [
                FaderFace::channel(FaderLaw::Channel),
                FaderFace::master(FaderLaw::Master),
                FaderFace::channel(FaderLaw::Linear { max_gain: 1.0 }),
                FaderFace::pitch(),
            ] {
                let plan = FaderPlan::new(w, h, &face, 0.4);
                assert!(plan.div_count() <= 24, "{w}x{h}: {}", plan.div_count());
            }
        }
        assert!(CrossfaderPlan::new(200, 46, 0.5).div_count() <= 24);
    }
}
