//! The loudness panel: a render-agnostic [`LoudnessPlan`] (the bar fills as coalesced quads, the
//! clip lamp) over om-ui's `LoudnessView`, and the GPUI painter. Like the segmented meters, a bar is
//! a well (one box) and one `canvas` that paints its quads, not one box per step.

use crate::app::Shell;
use crate::controls::button;
use crate::paint::{border, paint, place};
use gpui::{canvas, fill, point, prelude::*, px, size, Bounds, Context, Div};
use om_ui::geometry::Rect;
use om_ui::meter::MeterZone;
use om_ui::style::LookAndFeel;
use om_ui::theme::ThemeTokens;
use om_ui::widgets::loudness::{
    bar_fill, LoudnessLayout, LoudnessPanel, LoudnessView, BAR_CEILING_LUFS, BAR_FLOOR_LUFS,
};

/// Smallest text drawn on a control, in px.
const MIN_TEXT_PX: f32 = 12.0;
/// The bars turn amber at the streaming-platform target and red near the top of the range.
const MID_LUFS: f32 = -14.0;
const HIGH_LUFS: f32 = -9.0;
/// Room between a well's edge and its fill, in px.
const WELL_INSET: i32 = 1;

/// One filled run of a bar, in the panel's own coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FillQuad {
    pub rect: Rect,
    pub zone: MeterZone,
}

/// What the panel paints besides text and buttons.
#[derive(Debug, Clone, PartialEq)]
pub struct LoudnessPlan {
    /// The momentary and then the short-term bar's wells.
    pub wells: [Rect; 2],
    pub fills: [Vec<FillQuad>; 2],
    pub lamp: Rect,
    pub lamp_lit: bool,
}

impl LoudnessPlan {
    pub fn new(layout: &LoudnessLayout, view: &LoudnessView) -> Self {
        let wells = [layout.momentary.bar, layout.short_term.bar];
        Self {
            fills: [
                fill_quads(wells[0], view.momentary_fill),
                fill_quads(wells[1], view.short_term_fill),
            ],
            wells,
            lamp: layout.clip_lamp,
            lamp_lit: view.clipping,
        }
    }

    pub fn quad_count(&self) -> usize {
        self.fills.iter().map(Vec::len).sum()
    }
}

/// The runs of `well` up to `fill` (0 to 1), one per zone the fill reaches.
fn fill_quads(well: Rect, fill: f32) -> Vec<FillQuad> {
    let inner = well.reduced(WELL_INSET, WELL_INSET);
    let lit_to = (fill.clamp(0.0, 1.0) * inner.w as f32).round() as i32;
    let edges = [
        (MeterZone::Low, 0),
        (MeterZone::Mid, edge(inner.w, MID_LUFS)),
        (MeterZone::High, edge(inner.w, HIGH_LUFS)),
    ];
    let mut quads = Vec::new();
    for (i, (zone, start)) in edges.iter().enumerate() {
        let end = edges.get(i + 1).map_or(lit_to, |next| next.1.min(lit_to));
        if end > *start {
            quads.push(FillQuad {
                rect: Rect::new(inner.x + start, inner.y, end - start, inner.h),
                zone: *zone,
            });
        }
    }
    quads
}

/// Where `lufs` lands along a bar `width` px wide.
fn edge(width: i32, lufs: f32) -> i32 {
    debug_assert!(BAR_FLOOR_LUFS < lufs && lufs < BAR_CEILING_LUFS);
    (bar_fill(lufs) * width as f32).round() as i32
}

/// The toggle strip: a button that opens the panel and the momentary reading beside it.
pub fn loudness_toggle(shell: &Shell, bounds: Rect, cx: &mut Context<Shell>) -> Vec<Div> {
    let t = &shell.theme;
    let panel = &shell.loudness;
    let view = panel.view();
    let (button_rect, text_rect) = LoudnessPanel::toggle_layout(bounds);
    let colour = if view.warning.is_some() {
        t.warning
    } else {
        t.accent_for(0)
    };
    let lit = panel.is_open() || view.warning.is_some();
    vec![
        button(t, button_rect, "LUFS", lit, colour, cx, |this| {
            this.loudness.toggle();
        }),
        place(text_rect)
            .flex()
            .items_center()
            .text_size(px(MIN_TEXT_PX))
            .text_color(paint(summary_colour(t, view)))
            .child(panel.summary_text()),
    ]
}

fn summary_colour(t: &ThemeTokens, view: &LoudnessView) -> om_ui::theme::Colour {
    if view.warning.is_some() {
        t.warning
    } else if view.clipping {
        t.danger
    } else {
        t.text_primary
    }
}

/// The open panel at `bounds` (card coordinates): M and S bars, integrated, range, true peak with
/// its clip lamp, the Reset button and, when frames were dropped, the warning line.
pub fn loudness_panel(shell: &Shell, bounds: Rect, cx: &mut Context<Shell>) -> Div {
    let t = &shell.theme;
    let look = LookAndFeel::from_tokens(t);
    let m = look.meter;
    let view = shell.loudness.view();
    let layout = LoudnessPanel::layout(Rect::sized(bounds.w, bounds.h));
    let plan = LoudnessPlan::new(&layout, view);
    let mut el = place(bounds)
        .debug_selector(|| "loudness:panel".to_owned())
        .bg(paint(t.surface))
        .border_1()
        .border_color(paint(t.border))
        .rounded(px(t.control_radius))
        .text_color(paint(t.text_primary))
        .child(text(layout.title, "LOUDNESS", true, t.text_secondary))
        .child(button(
            t,
            layout.reset,
            "Reset",
            false,
            t.accent_for(0),
            cx,
            |this| this.engine.loudness().reset(),
        ));
    let rows = [
        (&layout.momentary, "M", &view.momentary_text),
        (&layout.short_term, "S", &view.short_term_text),
    ];
    for (row, label, value) in rows {
        el = el
            .child(text(row.label, label, false, t.text_secondary))
            .child(text(row.value, value, true, t.text_primary));
    }
    for well in plan.wells {
        el = el.child(
            border(place(well), t.border_width)
                .border_color(paint(m.border))
                .bg(paint(m.well))
                .rounded(px(m.radius)),
        );
    }
    el = el.child(fills_canvas(shell, &plan));
    let int = format!("I {}", view.integrated_text);
    let lra = format!("LRA {}", view.range_text);
    let tp = format!("TP {} dBTP", view.true_peak_text);
    el = el
        .child(text(layout.integrated, &int, false, t.text_primary))
        .child(text(layout.range, &lra, false, t.text_primary))
        .child(text(layout.true_peak, &tp, false, t.text_primary))
        .child(
            place(plan.lamp)
                .bg(paint(if plan.lamp_lit { t.danger } else { m.lamp_off }))
                .rounded(px(m.radius))
                .debug_selector(|| lamp_tag(plan.lamp_lit).to_owned()),
        );
    if let Some(warning) = &view.warning {
        el = el.child(
            text(layout.warning, warning, false, t.warning)
                .debug_selector(|| "loudness:warning".to_owned()),
        );
    }
    el
}

/// Selector of the clip lamp, which tells a test whether it is lit.
fn lamp_tag(lit: bool) -> &'static str {
    if lit {
        "loudness:lamp:lit"
    } else {
        "loudness:lamp:off"
    }
}

fn text(rect: Rect, label: &str, right: bool, colour: om_ui::theme::Colour) -> Div {
    let el = place(rect)
        .flex()
        .items_center()
        .text_size(px(MIN_TEXT_PX))
        .line_height(px(14.))
        .text_color(paint(colour));
    if right {
        el.justify_end().child(label.to_owned())
    } else {
        el.child(label.to_owned())
    }
}

/// One canvas over the whole panel that paints every bar quad.
fn fills_canvas(shell: &Shell, plan: &LoudnessPlan) -> Div {
    let m = LookAndFeel::from_tokens(&shell.theme).meter;
    let quads: Vec<(Rect, gpui::Rgba)> = plan
        .fills
        .iter()
        .flatten()
        .map(|q| {
            let colour = match q.zone {
                MeterZone::Low => m.low,
                MeterZone::Mid => m.mid,
                MeterZone::High => m.high,
            };
            (q.rect, paint(colour))
        })
        .collect();
    div_full().child(
        canvas(
            |_, _, _| (),
            move |bounds: Bounds<gpui::Pixels>, _, window, _| {
                for (r, colour) in &quads {
                    let rect = Bounds::new(
                        point(
                            bounds.origin.x + px(r.x as f32),
                            bounds.origin.y + px(r.y as f32),
                        ),
                        size(px(r.w as f32), px(r.h as f32)),
                    );
                    window.paint_quad(fill(rect, *colour));
                }
            },
        )
        .size_full(),
    )
}

fn div_full() -> Div {
    gpui::div().absolute().top(px(0.)).left(px(0.)).size_full()
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_engine::LoudnessReading;
    use om_ui::widgets::loudness::PANEL_HEIGHT;

    fn plan(r: &LoudnessReading) -> LoudnessPlan {
        let layout = LoudnessPanel::layout(Rect::sized(109, PANEL_HEIGHT));
        LoudnessPlan::new(&layout, &LoudnessView::from_reading(r))
    }

    fn reading(momentary: f32, peak: f32) -> LoudnessReading {
        LoudnessReading {
            momentary_lufs: momentary,
            short_term_lufs: -30.0,
            integrated_lufs: -20.0,
            loudness_range_lu: 4.0,
            true_peak_dbtp: [peak, peak],
            dropped_frames: 0,
            frames_measured: 1,
        }
    }

    #[test]
    fn nothing_measured_paints_no_fill_and_an_unlit_lamp() {
        let p = plan(&LoudnessReading::SILENT);
        assert_eq!(p.quad_count(), 0);
        assert!(!p.lamp_lit);
    }

    #[test]
    fn a_quiet_bar_is_one_low_run_inside_its_well() {
        let p = plan(&reading(-30.0, -6.0));
        let [q] = p.fills[0].as_slice() else {
            panic!("one run expected");
        };
        assert_eq!(q.zone, MeterZone::Low);
        assert!(p.wells[0].contains(&q.rect));
    }

    #[test]
    fn a_loud_bar_runs_through_all_three_zones_without_gaps() {
        let p = plan(&reading(-2.0, -6.0));
        let runs = &p.fills[0];
        let zones: Vec<_> = runs.iter().map(|q| q.zone).collect();
        assert_eq!(zones, [MeterZone::Low, MeterZone::Mid, MeterZone::High]);
        for pair in runs.windows(2) {
            assert_eq!(pair[0].rect.right(), pair[1].rect.x);
        }
        assert!(p.wells[0].contains(&runs[2].rect));
    }

    #[test]
    fn the_lamp_follows_the_true_peak() {
        assert!(plan(&reading(-20.0, 0.5)).lamp_lit);
        assert!(!plan(&reading(-20.0, -3.0)).lamp_lit);
    }
}
