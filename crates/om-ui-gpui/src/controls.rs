//! Shared control drawing and pointer wiring: push buttons, sliders, knobs and meters. Positions
//! are relative to the parent card (which is positioned); `abs` gives window coordinates for drags.

use crate::app::Shell;
use crate::drag::{fraction, Target};
use crate::paint::{paint, place, skin};
use gpui::{div, prelude::*, px, Context, Div, MouseButton, MouseDownEvent, MouseUpEvent};
use om_ui::geometry::Rect;
use om_ui::style::LookAndFeel;
use om_ui::theme::{Colour, ThemeTokens};

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
    );
    let el = if lit {
        el
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

/// Makes `el` start a drag on `target` when pressed. `abs` is the control's window rectangle.
pub fn draggable(el: Div, target: Target, abs: Rect, cx: &mut Context<Shell>) -> Div {
    el.on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
            this.begin_drag(target, abs, ev.position);
            cx.notify();
        }),
    )
}

/// Vertical or horizontal slider track with its thumb at the value's fraction. Drawn in `rect`
/// (parent coordinates) and made draggable with `abs`.
pub fn slider(
    t: &ThemeTokens,
    rect: Rect,
    (value, min, max): (f64, f64, f64),
    target: Target,
    abs: Rect,
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
    let track = track.child(skin(thumb, &look.fader.thumb));
    draggable(track, target, abs, cx)
}

/// A knob: label on top, readout below, and a fill showing the value. Drawn in `rect` (parent
/// coordinates) and made draggable with `abs`.
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
    let frac = fraction(value, min, max) as f32;
    let fill_h = ((rect.h as f32 - inset_l) * frac) as i32;
    // The value fill is translucent and drawn first; label and readout are drawn above it in the
    // primary text colour so they stay readable over both the body and the fill.
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
        // Centre mark of a bipolar knob (the zero of the range).
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

/// One stereo meter column (two bars). `levels` are 0..=1 proportions; a lit clip lamp sits on top.
pub fn stereo_meter(t: &ThemeTokens, rect: Rect, levels: [f32; 2], clipping: [bool; 2]) -> Div {
    let look = LookAndFeel::from_tokens(t);
    let m = look.meter;
    let headroom = m.headroom as i32;
    let half = rect.w / 2;
    let mut col = place(rect);
    for (i, (level, clip)) in levels.iter().zip(clipping).enumerate() {
        let x = i as i32 * half;
        let bar_h = ((rect.h - headroom) as f32 * level.clamp(0.0, 1.0)) as i32;
        let side = m.bar_inset as i32;
        let bar = Rect::new(x + side, rect.h - bar_h, (half - 2 * side).max(1), bar_h);
        col = col.child(
            place(Rect::new(
                x,
                headroom,
                half.max(1),
                (rect.h - headroom).max(1),
            ))
            .bg(paint(m.well))
            .rounded(px(m.radius)),
        );
        col = col.child(place(bar).bg(paint(if *level > 0.9 {
            m.high
        } else if *level > 0.6 {
            m.mid
        } else {
            m.low
        })));
        col = col.child(
            place(Rect::new(
                x + side,
                m.inset as i32,
                (half - 2 * side).max(1),
                m.lamp_height as i32,
            ))
            .bg(paint(if clip { m.lamp_on } else { m.lamp_off }))
            .rounded(px(m.radius)),
        );
    }
    col
}

/// A text caption at `rect` (parent coordinates).
pub fn caption(t: &ThemeTokens, rect: Rect, text: &str) -> Div {
    let style = LookAndFeel::from_tokens(t).panel_caption;
    place(rect)
        .text_size(px(style.font_size))
        .text_color(paint(style.text))
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
