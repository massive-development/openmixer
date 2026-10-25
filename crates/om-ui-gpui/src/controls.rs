//! Shared control drawing and pointer wiring: push buttons, sliders, knobs and meters. Positions
//! are relative to the parent card (which is positioned); `abs` gives window coordinates for drags.
//!
//! Knobs and faders print no value at rest. The value appears in a popup above the control while
//! it is dragged, and for knobs while the pointer hovers it (the hover is Shell state, so a hidden
//! popup never sits over a neighbouring button). A double-click puts a mixer control back to its
//! default.

use crate::app::Shell;
use crate::drag::{fraction, Target};
use crate::fader_paint::{self, FaderFace, FaderLaw};
use crate::knob_paint::{
    angle_of, in_kill_zone, knob_geometry, paint_knob, sweep_fraction, value_of_fraction,
    KnobColours, KnobPaintState, KnobRole, POPUP_LIFT,
};
use crate::meter_paint;
use crate::paint::{paint, place, shadow, skin};
use gpui::{
    canvas, div, prelude::*, px, Bounds, Context, Div, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseUpEvent, Pixels, ScrollWheelEvent, Window,
};
use om_ui::geometry::Rect;
use om_ui::style::LookAndFeel;
use om_ui::theme::{Colour, ThemeTokens};

/// Smallest text drawn on a control, in px.
const MIN_TEXT_PX: f32 = 12.0;
/// A knob cell this tall or taller puts its label under the knob; shorter ones put it beside.
const LABEL_BELOW_MIN_H: i32 = 48;
/// Gap between the left edge of a cell and a knob that has its label beside it, px. Unsourced.
const KNOB_EDGE_PAD: f32 = 4.0;
/// Space between a knob's track and a label beside it, px. Unsourced.
const LABEL_GAP: f32 = 6.0;
/// Pixels one wheel line is worth when the wheel reports pixels.
const WHEEL_LINE_PX: f32 = 16.0;

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

/// A transparent layer over its parent that shows `text` in a tooltip bubble while the pointer
/// rests on it. It takes no clicks of its own, so the parent's handlers still run.
pub(crate) fn tip_overlay(
    t: &ThemeTokens,
    id: impl Into<String>,
    text: impl Into<String>,
) -> gpui::Stateful<Div> {
    let style = LookAndFeel::from_tokens(t).tooltip;
    let text = text.into();
    div()
        .id(gpui::ElementId::Name(id.into().into()))
        .absolute()
        .size_full()
        .tooltip(move |_, cx| {
            cx.new(|_| crate::deck_view::Tip {
                text: text.clone(),
                fill: style.fill,
                ink: style.text,
            })
            .into()
        })
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
    let vertical = rect.h >= rect.w;
    let accent = match target {
        Target::Channel(ch) => t.accent_for(ch as i32),
        _ => t.brand,
    };
    if target == Target::Crossfader && !vertical {
        let pos = fraction(value, min, max) as f32;
        return fader_paint::crossfader(t, rect, pos, accent, target, abs, popup_text, cx);
    }
    if vertical {
        // The stored value stays linear; Linear is today's cap travel (plan Phase 2 default). The
        // tapered laws are wired at the call sites in Phase 5.
        let law = FaderLaw::Linear {
            max_gain: max as f32,
        };
        let face = if target == Target::Master {
            FaderFace::master(law)
        } else {
            FaderFace::channel(law)
        };
        let pos = fraction(value, min, max) as f32;
        return fader_paint::fader(t, rect, &face, pos, accent, target, abs, popup_text, cx);
    }
    // Other horizontal sliders (stems, phones) keep the plain track and thumb.
    let look = LookAndFeel::from_tokens(t);
    let frac = fraction(value, min, max) as f32;
    let thumb_px = look.fader.thumb_size.round() as i32;
    let travel = (rect.w - thumb_px).max(0) as f32;
    let thumb = place(Rect::new((frac * travel) as i32, 0, thumb_px, rect.h));
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

/// How a rotary knob is drawn beyond what [`KnobFace`] says. The default is a band (EQ) knob in
/// the first channel accent, idle and enabled.
#[derive(Debug, Clone, Copy, Default)]
pub struct KnobLook {
    /// Largest cap and label placement; see [`KnobRole`].
    pub role: KnobRole,
    /// The value arc colour (the strip accent); the theme's first accent when None.
    pub accent: Option<Colour>,
    /// Being dragged: a heavier arc.
    pub dragging: bool,
    /// Nothing to control (an empty sampler pad): faded, no input, no popup.
    pub disabled: bool,
}

/// The caption line under a knob whose label sits below it, px.
fn label_line_height(t: &ThemeTokens) -> f32 {
    (t.type_caption.max(MIN_TEXT_PX) + 2.0).ceil()
}

/// A rotary knob with the default look. See [`knob_face_with`].
pub fn knob_face(
    t: &ThemeTokens,
    rect: Rect,
    face: KnobFace<'_>,
    range: (f64, f64, f64),
    target: Target,
    abs: Rect,
    cx: &mut Context<Shell>,
) -> Div {
    knob_face_with(t, rect, face, range, (target, abs), KnobLook::default(), cx)
}

/// A rotary knob in `rect`: a 270 degree arc with the value from the detent (or minimum), a cap
/// with a pointer, the label below or beside it, and, while `face.active`, the value in a popup.
/// `at` is the target and the rect's window coordinates.
///
/// Only the disc of the knob takes the pointer: the wide trim bar is not one big button. It
/// responds to a press (drag; double-click or ctrl/cmd-click reset), shift (fine), the wheel, and
/// the keys of [`key_step`] when focused. Hovering tells the Shell, which turns `active` on.
pub fn knob_face_with(
    t: &ThemeTokens,
    rect: Rect,
    face: KnobFace<'_>,
    (value, min, max): (f64, f64, f64),
    (target, abs): (Target, Rect),
    knob_look: KnobLook,
    cx: &mut Context<Shell>,
) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let show_label = !face.label.is_empty() && knob_look.role.shows_label();
    // Tall cells (the EQ) put the label under the knob; short ones put it beside.
    let label_below = show_label && rect.h >= LABEL_BELOW_MIN_H;
    let label_h = if label_below {
        label_line_height(t)
    } else {
        0.0
    };
    let mut geom = knob_geometry(rect.w as f32, rect.h as f32, label_h, knob_look.role);
    if show_label && !label_below {
        // Room for the label: the knob moves to the left edge instead of the middle.
        let (_, _, side) = geom.bbox();
        geom.cx = side / 2.0 + KNOB_EDGE_PAD;
    }
    let detent = face.detent.filter(|d| *d > min && *d < max);
    let state = KnobPaintState {
        geom,
        angle: angle_of(value, min, detent, max),
        detent_angle: detent.map(|d| angle_of(d, min, detent, max)),
        bipolar: detent.is_some(),
        killed: in_kill_zone(value, min, detent, matches!(target, Target::Eq(..))),
        hovered: face.active && !knob_look.disabled,
        active: knob_look.dragging,
        disabled: knob_look.disabled,
    };
    let colours = KnobColours::new(&look, t, knob_look.accent.unwrap_or(look.knob.fill));
    let painter = canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            paint_knob(window, bounds.origin, &state, &colours);
        },
    )
    .absolute()
    .size_full();

    let mut outer = place(rect)
        .text_size(px(t.type_caption.max(MIN_TEXT_PX)))
        .text_color(paint(
            look.knob
                .label
                .text
                .with_multiplied_alpha(if knob_look.disabled { 0.4 } else { 1.0 }),
        ))
        .child(painter);

    if show_label {
        let label = if label_below {
            div()
                .absolute()
                .left(px(0.))
                .top(px(rect.h as f32 - label_h))
                .w(px(rect.w as f32))
                .h(px(label_h))
                .flex()
                .items_center()
                .justify_center()
        } else {
            let (_, _, side) = geom.bbox();
            let left = geom.cx + side / 2.0 + LABEL_GAP;
            div()
                .absolute()
                .left(px(left))
                .top(px(0.))
                .w(px((rect.w as f32 - left).max(0.)))
                .h(px(rect.h as f32))
                .flex()
                .items_center()
        };
        outer = outer.child(
            label
                .overflow_hidden()
                .whitespace_nowrap()
                .child(face.label.to_owned()),
        );
    }

    // The pressable part is the knob's bounding square, and a press outside the disc itself is
    // ignored. The popup is the wrapper's other child, so it exists (and takes the pointer) only
    // while shown, and then it takes nothing.
    let (bx, by, side) = geom.bbox();
    let hit_rect = Rect::new(
        bx.floor() as i32,
        by.floor() as i32,
        side.ceil() as i32,
        side.ceil() as i32,
    );
    let hit = place(hit_rect).id(gpui::ElementId::Name(format!("knob-{target:?}").into()));
    let hit = if knob_look.disabled {
        hit
    } else {
        hit.tab_index(0)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    let local_x = f32::from(ev.position.x) - abs.x as f32;
                    let local_y = f32::from(ev.position.y) - abs.y as f32;
                    if !geom.hits(local_x, local_y) {
                        return;
                    }
                    let reset = is_reset_click(
                        ev.click_count,
                        ev.modifiers.control || ev.modifiers.platform,
                    );
                    if reset && this.reset_target(target) {
                        this.end_drag();
                    } else {
                        this.begin_drag_fine(target, abs, ev.position, ev.modifiers.shift);
                    }
                    cx.notify();
                }),
            )
            .on_scroll_wheel(cx.listener(move |this, ev: &ScrollWheelEvent, _, cx| {
                let lines = f64::from(f32::from(ev.delta.pixel_delta(px(WHEEL_LINE_PX)).y))
                    / f64::from(WHEEL_LINE_PX);
                if lines != 0.0 {
                    this.knob_wheel(target, lines, ev.modifiers.shift);
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_key_down(cx.listener(move |this, ev: &KeyDownEvent, _, cx| {
                let keys = &ev.keystroke;
                if keys.modifiers.control || keys.modifiers.platform || keys.modifiers.alt {
                    return;
                }
                if this.knob_key(target, keys.key.as_str(), keys.modifiers.shift) {
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                if *over {
                    this.hovered = Some(target);
                } else if this.hovered == Some(target) {
                    this.hovered = None;
                }
                cx.notify();
            }))
    };
    outer = outer.child(hit);
    if face.active && !knob_look.disabled {
        let text = if face.label.is_empty() {
            face.readout.to_owned()
        } else {
            format!("{} {}", face.label, face.readout)
        };
        outer = outer.child(popup(t, rect.w, geom.popup_top(rect.y as f32), &text));
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

/// One stereo meter column: an optional dB scale, then two segmented bars coloured by dB zone (the
/// theme's `meter_mid_db` / `meter_high_db`), each with a peak-hold segment and a latching clip lamp
/// above it. See `meter_paint`.
/// A click anywhere on the column runs `on_click` (it clears the lamps).
pub fn stereo_meter(
    t: &ThemeTokens,
    rect: Rect,
    labels: bool,
    bars: [BarState; 2],
    cx: &mut Context<Shell>,
    on_click: impl Fn(&mut Shell) + 'static,
) -> Div {
    meter_paint::segmented_meter(t, rect, labels, bars, cx, on_click)
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
