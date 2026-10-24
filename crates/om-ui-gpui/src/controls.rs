//! Shared control drawing and pointer wiring: push buttons, sliders, knobs and meters. Positions
//! are relative to the parent card (which is positioned); `abs` gives window coordinates for drags.
//!
//! Knobs and faders print no value at rest. The value appears in a popup above the control while
//! it is dragged, and for knobs while the pointer hovers it (the hover is Shell state, so a hidden
//! popup never sits over a neighbouring button). A double-click puts a mixer control back to its
//! default.

use crate::app::Shell;
use crate::drag::{fraction, Target};
use crate::knob_paint::{sweep_fraction, value_of_fraction};
use crate::paint::{border, paint, place, shadow, skin};
use gpui::{div, prelude::*, px, Context, Div, MouseButton, MouseDownEvent, MouseUpEvent};
use om_ui::geometry::Rect;
use om_ui::meter::{column_layout, fitting_marks, MeterScale, MeterZone, MIN_LAMP_HEIGHT};
use om_ui::style::LookAndFeel;
use om_ui::theme::{Colour, ThemeTokens};

/// Smallest text drawn on a control, in px.
const MIN_TEXT_PX: f32 = 12.0;
/// How far above its control a value popup floats, in px.
const POPUP_LIFT: f32 = 26.0;

/// Share of a knob's sweep one wheel notch or arrow key moves (spec section 3: 1/100).
const STEP_FRACTION: f64 = 0.01;
/// The same with shift held: 1/1000.
const FINE_STEP_FRACTION: f64 = 0.001;
/// Steps a Page Up or Page Down key moves.
const PAGE_STEPS: f64 = 10.0;

/// The value after `notches` wheel notches over a knob (positive: more). One notch is 1/100 of
/// the sweep, 1/1000 with shift. Like a drag, it moves along the sweep, not the raw range.
pub fn wheel_step(
    value: f64,
    notches: f64,
    shift: bool,
    min: f64,
    detent: Option<f64>,
    max: f64,
) -> f64 {
    let step = if shift {
        FINE_STEP_FRACTION
    } else {
        STEP_FRACTION
    };
    nudge(value, notches * step, min, detent, max)
}

/// `value` moved `delta` of the sweep.
fn nudge(value: f64, delta: f64, min: f64, detent: Option<f64>, max: f64) -> f64 {
    let fraction = sweep_fraction(value, min, detent, max) + delta;
    value_of_fraction(fraction, min, detent, max)
}

/// The value a key press gives a focused knob, or None for a key the knob does not use. Up and
/// Right are one step more, Down and Left one less, Page Up and Page Down ten steps; shift makes
/// a step finer. Home goes to `reset` (the default) and End to the maximum.
pub fn key_step(
    key: &str,
    shift: bool,
    (value, reset): (f64, f64),
    (min, detent, max): (f64, Option<f64>, f64),
) -> Option<f64> {
    let step = if shift {
        FINE_STEP_FRACTION
    } else {
        STEP_FRACTION
    };
    let by = |steps: f64| Some(nudge(value, steps * step, min, detent, max));
    match key {
        "up" | "right" => by(1.0),
        "down" | "left" => by(-1.0),
        "pageup" => by(PAGE_STEPS),
        "pagedown" => by(-PAGE_STEPS),
        "home" => Some(reset),
        "end" => Some(max),
        _ => None,
    }
}

/// True when a press resets a knob: a double-click, or a click with ctrl or cmd held.
pub fn is_reset_click(click_count: usize, ctrl_or_cmd: bool) -> bool {
    click_count >= 2 || ctrl_or_cmd
}

/// A labelled push button. `accent` is its colour when lit. Idle buttons take the hover fill.
pub fn button(
    t: &ThemeTokens,
    rect: Rect,
    label: &str,
    lit: bool,
    accent: Colour,
    cx: &mut Context<Shell>,
    on_press: impl Fn(&mut Shell) + 'static,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let hover = look.button.hover.fill;
    let el = skin(
        place(rect).flex().items_center().justify_center(),
        &if lit {
            look.lit_button(accent)
        } else {
            look.button.idle
        },
    )
    .text_size(px(t.type_caption.max(MIN_TEXT_PX)));
    let el = if lit {
        shadow(el, &look.glow(accent))
    } else {
        el.hover(move |style| style.bg(paint(hover)))
    };
    el.child(label.to_owned()).on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            on_press(this);
            cx.notify();
        }),
    )
}

/// Makes `el` start a drag on `target` when pressed; a double-click resets the control to its
/// default instead. `abs` is the control's window rectangle.
pub fn draggable(el: Div, target: Target, abs: Rect, cx: &mut Context<Shell>) -> Div {
    el.on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
            if ev.click_count >= 2 && this.reset_target(target) {
                this.end_drag();
            } else {
                this.begin_drag(target, abs, ev.position);
            }
            cx.notify();
        }),
    )
}

/// A value popup centred above a control `width` px wide, drawn at `top` (negative: above). It has
/// no pointer handling, so it never takes a press from what lies under it.
fn popup(t: &ThemeTokens, width: i32, top: f32, text: &str) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let chip = skin(div().px(px(6.)).py(px(1.)), &look.tooltip)
        .text_size(px(t.type_caption.max(MIN_TEXT_PX)))
        .whitespace_nowrap()
        .child(text.to_owned());
    div()
        .absolute()
        .left(px(0.))
        .top(px(top))
        .w(px(width as f32))
        .flex()
        .justify_center()
        .child(chip)
}

/// Vertical or horizontal slider track with its thumb at the value's fraction. Drawn in `rect`
/// (parent coordinates) and made draggable with `abs`.
pub fn slider(
    t: &ThemeTokens,
    rect: Rect,
    range: (f64, f64, f64),
    target: Target,
    abs: Rect,
    cx: &mut Context<Shell>,
) -> Div {
    slider_with_popup(t, rect, range, target, abs, None, cx)
}

/// [`slider`] with a value popup: `popup_text` is shown above the track while `Some`, which the
/// caller passes while the slider is being dragged.
pub fn slider_with_popup(
    t: &ThemeTokens,
    rect: Rect,
    (value, min, max): (f64, f64, f64),
    target: Target,
    abs: Rect,
    popup_text: Option<&str>,
    cx: &mut Context<Shell>,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let frac = fraction(value, min, max) as f32;
    let vertical = rect.h >= rect.w;
    let thumb_px = look.fader.thumb_size.round() as i32;
    let thumb = if vertical {
        let travel = (rect.h - thumb_px).max(0) as f32;
        place(Rect::new(
            0,
            ((1.0 - frac) * travel) as i32,
            rect.w,
            thumb_px,
        ))
    } else {
        let travel = (rect.w - thumb_px).max(0) as f32;
        place(Rect::new((frac * travel) as i32, 0, thumb_px, rect.h))
    };
    let track = skin(place(rect), &look.fader.track);
    let mut track = track.child(skin(thumb, &look.fader.thumb));
    if let Some(text) = popup_text {
        track = track.child(popup(t, rect.w, -POPUP_LIFT, text));
    }
    draggable(track, target, abs, cx)
}

/// What a knob prints and where its detent is.
#[derive(Debug, Clone, Copy)]
pub struct KnobFace<'a> {
    /// Printed in the middle of the knob; empty for a knob whose label sits elsewhere.
    pub label: &'a str,
    /// The value text for the popup.
    pub readout: &'a str,
    /// The value the knob rests on (its detent), marked on the body.
    pub detent: Option<f64>,
    /// The popup is shown: the knob is being dragged or hovered (see `Shell::shows_value`).
    pub active: bool,
}

/// A compact knob for cards that draw their own (the deck's stem filter): the label on top and the
/// value at the bottom of the body, always visible, because those knobs are too small for a popup.
/// A bipolar range is marked at zero.
pub fn knob(
    t: &ThemeTokens,
    rect: Rect,
    (label, readout): (&str, &str),
    (value, min, max): (f64, f64, f64),
    target: Target,
    abs: Rect,
    cx: &mut Context<Shell>,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let (inset_s, inset_l) = (look.inset_small, look.inset_large);
    // A narrow knob drops the unit so the readout stays on one line.
    let readout = if rect.w < 48 {
        readout.strip_suffix(" dB").unwrap_or(readout)
    } else {
        readout
    };
    // A very narrow knob has room for the value only.
    let label = if rect.w < 36 { "" } else { label };
    let frac = fraction(value, min, max) as f32;
    let fill_h = ((rect.h as f32 - inset_l) * frac) as i32;
    let mut body = skin(place(rect), &look.knob.body)
        .overflow_hidden()
        .text_color(paint(look.knob.readout.text))
        .child(
            place(Rect::new(
                0,
                rect.h - fill_h - inset_s as i32,
                rect.w,
                fill_h,
            ))
            .bg(paint(look.knob.fill.with_multiplied_alpha(0.45)))
            .rounded(px(look.bar_radius)),
        );
    if min < 0.0 && max > 0.0 {
        let zero = fraction(0.0, min, max) as f32;
        let y = ((rect.h as f32 - inset_l) * (1.0 - zero)) as i32 + inset_s as i32;
        body = body.child(
            place(Rect::new(0, y, rect.w, 1))
                .bg(paint(look.knob.readout.text.with_multiplied_alpha(0.5))),
        );
    }
    let body = body
        .child(
            div()
                .absolute()
                .left(px(inset_l))
                .top(px(inset_s))
                .child(label.to_owned()),
        )
        .child(
            div()
                .absolute()
                .left(px(inset_l))
                .bottom(px(inset_s))
                .child(readout.to_owned()),
        );
    draggable(body, target, abs, cx)
}

/// A knob: a body with the value drawn as a fill from its detent (or bottom) to the value, a
/// centre mark at the detent, the label in the middle, and, while `face.active`, the value in a
/// popup above it. Hovering the knob tells the Shell, which turns `active` on.
pub fn knob_face(
    t: &ThemeTokens,
    rect: Rect,
    face: KnobFace<'_>,
    (value, min, max): (f64, f64, f64),
    target: Target,
    abs: Rect,
    cx: &mut Context<Shell>,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let inset = look.inset_small;
    let span = (rect.h as f32 - 2.0 * inset).max(1.0);
    let y_of = |v: f64| inset + span * (1.0 - fraction(v, min, max) as f32);
    let base_y = face.detent.map_or(inset + span, y_of);
    let value_y = y_of(value);
    let (top, bottom) = if value_y < base_y {
        (value_y, base_y)
    } else {
        (base_y, value_y)
    };
    let mut body = skin(place(Rect::sized(rect.w, rect.h)), &look.knob.body)
        .overflow_hidden()
        .text_color(paint(look.knob.readout.text))
        .text_size(px(t.type_caption.max(MIN_TEXT_PX)))
        .child(
            place(Rect::new(0, top as i32, rect.w, (bottom - top) as i32))
                .bg(paint(look.knob.fill.with_multiplied_alpha(0.45))),
        );
    if let Some(d) = face.detent {
        body = body.child(
            place(Rect::new(0, y_of(d) as i32, rect.w, 1))
                .bg(paint(look.knob.readout.text.with_multiplied_alpha(0.5))),
        );
    }
    if !face.label.is_empty() {
        body = body.child(
            div()
                .absolute()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(paint(look.knob.label.text))
                .child(face.label.to_owned()),
        );
    }
    // The pressable, hoverable part fills the wrapper; the popup is the wrapper's other child, so
    // it exists (and takes the pointer) only while shown, and then it takes nothing.
    let pad = draggable(div().size_full().child(body), target, abs, cx);
    let pad = pad
        .id(gpui::ElementId::Name(format!("knob-{target:?}").into()))
        .on_hover(cx.listener(move |this, over: &bool, _, cx| {
            if *over {
                this.hovered = Some(target);
            } else if this.hovered == Some(target) {
                this.hovered = None;
            }
            cx.notify();
        }));
    let mut outer = place(rect).child(pad);
    if face.active {
        let text = if face.label.is_empty() {
            face.readout.to_owned()
        } else {
            format!("{} {}", face.label, face.readout)
        };
        outer = outer.child(popup(t, rect.w, -POPUP_LIFT, &text));
    }
    outer
}

/// One side of a stereo meter, as the view model reports it.
#[derive(Debug, Clone, Copy, Default)]
pub struct BarState {
    /// Bar height, `0..=1`.
    pub level: f32,
    /// Peak-hold tick height, `0..=1`.
    pub hold: f32,
    /// The latched clip lamp.
    pub clipping: bool,
}

/// One stereo meter column: an optional dB scale, then two bars coloured by dB zone (the theme's
/// `meter_mid_db` / `meter_high_db`), each with a peak-hold tick and a latching clip lamp above it.
/// A click anywhere on the column runs `on_click` (it clears the lamps).
pub fn stereo_meter(
    t: &ThemeTokens,
    rect: Rect,
    labels: bool,
    bars: [BarState; 2],
    cx: &mut Context<Shell>,
    on_click: impl Fn(&mut Shell) + 'static,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let m = look.meter;
    let scale = MeterScale::from_tokens(t);
    let headroom = m.headroom as i32;
    let col = column_layout(Rect::sized(rect.w, rect.h), labels);
    let well_h = (rect.h - headroom).max(1);
    let tick_h = t.meter_hold_tick_height.round().max(1.0) as i32;
    let lamp_h = (m.lamp_height.round() as i32).max(MIN_LAMP_HEIGHT);
    let zone_colour = |zone: MeterZone| match zone {
        MeterZone::Low => m.low,
        MeterZone::Mid => m.mid,
        MeterZone::High => m.high,
    };
    // A proportion's y inside the column: the well's top is 1.0, its bottom 0.0.
    let y_of = |p: f32| headroom + ((1.0 - p.clamp(0.0, 1.0)) * well_h as f32) as i32;
    let marks = fitting_marks(&scale, well_h);

    let mut el = place(rect);
    for mark in &marks {
        if labels {
            let ty = (y_of(mark.proportion) - 6).clamp(0, (rect.h - 12).max(0));
            el = el.child(
                place(Rect::new(0, ty, col.labels.w - 3, 12))
                    .flex()
                    .justify_end()
                    .text_size(px(MIN_TEXT_PX))
                    .line_height(px(12.))
                    .text_color(paint(t.text_secondary))
                    .child(mark.label.clone()),
            );
        }
    }
    for (bar, state) in col.bars.iter().zip(bars) {
        let well = Rect::new(bar.x, headroom, bar.w.max(1), well_h);
        el = el.child(
            border(place(well), t.border_width)
                .border_color(paint(m.border))
                .bg(paint(m.well))
                .rounded(px(m.radius)),
        );
        let side = m.bar_inset as i32;
        let inner_w = (well.w - 2 * side).max(1);
        for mark in &marks {
            if mark.proportion > 0.0 && mark.proportion < 1.0 {
                el = el.child(
                    place(Rect::new(well.x + side, y_of(mark.proportion), inner_w, 1))
                        .bg(paint(m.border)),
                );
            }
        }
        for (zone, from, to) in scale.runs(state.level) {
            let (y_top, y_bottom) = (y_of(to), y_of(from));
            el = el.child(
                place(Rect::new(
                    well.x + side,
                    y_top,
                    inner_w,
                    (y_bottom - y_top).max(1),
                ))
                .bg(paint(zone_colour(zone))),
            );
        }
        if state.hold > 0.0 {
            let hold_zone = scale.zone(state.hold);
            let y = (y_of(state.hold) - tick_h / 2).clamp(headroom, rect.h - tick_h);
            el = el.child(
                place(Rect::new(well.x + side, y, inner_w, tick_h))
                    .bg(paint(zone_colour(hold_zone))),
            );
        }
        el = el.child(
            place(Rect::new(well.x + side, m.inset as i32, inner_w, lamp_h))
                .bg(paint(if state.clipping {
                    m.lamp_on
                } else {
                    m.lamp_off
                }))
                .rounded(px(m.radius)),
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

/// A text caption at `rect` (parent coordinates). 12 px at least.
pub fn caption(t: &ThemeTokens, rect: Rect, text: &str) -> Div {
    let style = LookAndFeel::from_tokens(t).panel_caption;
    place(rect)
        .text_size(px(style.font_size.max(MIN_TEXT_PX)))
        .text_color(paint(style.text))
        .child(text.to_owned())
}

/// Text centred in `rect`, for a label that rides on another control (the end marks of the
/// crossfader, the names inside the phones sliders). It has no pointer handling, so the control
/// under it still takes the press.
pub fn mark(t: &ThemeTokens, rect: Rect, text: &str) -> Div {
    place(rect)
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(MIN_TEXT_PX))
        .text_color(paint(t.text_secondary))
        .child(text.to_owned())
}

/// Releases the drag when the pointer goes up anywhere in the window.
pub fn release_on(el: Div, cx: &mut Context<Shell>) -> Div {
    el.on_mouse_up(
        MouseButton::Left,
        cx.listener(|this, _: &MouseUpEvent, _, cx| {
            this.end_drag();
            cx.notify();
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knob_paint::{angle_of, sweep_fraction};

    const EQ: (f64, Option<f64>, f64) = (-48.0, Some(0.0), 6.0);

    fn frac(v: f64) -> f64 {
        sweep_fraction(v, EQ.0, EQ.1, EQ.2)
    }

    #[test]
    fn wheel_notch_is_one_hundredth_of_the_sweep() {
        let (min, d, max) = EQ;
        let up = wheel_step(0.0, 1.0, false, min, d, max);
        assert!((frac(up) - 0.51).abs() < 1e-9);
        let down = wheel_step(0.0, -3.0, false, min, d, max);
        assert!((frac(down) - 0.47).abs() < 1e-9);
        let fine = wheel_step(0.0, 1.0, true, min, d, max);
        assert!((frac(fine) - 0.501).abs() < 1e-9);
        assert!(angle_of(up, min, d, max) > 0.0);
        assert_eq!(wheel_step(6.0, 5.0, false, min, d, max), 6.0, "clamps");
    }

    #[test]
    fn keys_step_reset_and_jump() {
        let (min, d, max) = EQ;
        let key = |k: &str, shift: bool| key_step(k, shift, (0.0, 0.0), (min, d, max));
        let step = |v: f64| (frac(v) - 0.5).abs();
        assert!((step(key("up", false).unwrap()) - 0.01).abs() < 1e-9);
        assert!(key("right", false).unwrap() > 0.0);
        assert!(key("down", false).unwrap() < 0.0);
        assert!(key("left", false).unwrap() < 0.0);
        assert!((step(key("up", true).unwrap()) - 0.001).abs() < 1e-9);
        assert!((step(key("pageup", false).unwrap()) - 0.1).abs() < 1e-9);
        assert!(key("pagedown", false).unwrap() < 0.0);
        assert_eq!(key("end", false), Some(6.0));
        assert_eq!(
            key_step("home", false, (3.0, 1.0), (min, d, max)),
            Some(1.0)
        );
        assert_eq!(key("x", false), None);
        assert_eq!(key("enter", false), None);
    }

    #[test]
    fn ctrl_click_resets() {
        assert!(is_reset_click(2, false));
        assert!(is_reset_click(1, true));
        assert!(is_reset_click(3, true));
        assert!(!is_reset_click(1, false));
    }
}
