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
use crate::meter_paint;
use crate::paint::{paint, place, shadow, skin};
use gpui::{div, prelude::*, px, Context, Div, MouseButton, MouseDownEvent, MouseUpEvent};
use om_ui::geometry::Rect;
use om_ui::style::LookAndFeel;
use om_ui::theme::{Colour, ThemeTokens};

/// Smallest text drawn on a control, in px.
const MIN_TEXT_PX: f32 = 12.0;
/// How far above its control a value popup floats, in px.
const POPUP_LIFT: f32 = 26.0;

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
