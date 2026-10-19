//! Shared control drawing and pointer wiring: push buttons, sliders, knobs and meters. Positions
//! are relative to the parent card (which is positioned); `abs` gives window coordinates for drags.

use crate::app::Shell;
use crate::drag::{fraction, Target};
use crate::paint::{paint, place};
use gpui::{div, prelude::*, px, Context, Div, MouseButton, MouseDownEvent, MouseUpEvent};
use om_ui::geometry::Rect;
use om_ui::theme::{Colour, ThemeTokens};

/// A labelled push button. `accent` is its colour when lit.
pub fn button(
    t: &ThemeTokens,
    rect: Rect,
    label: &str,
    lit: bool,
    accent: Colour,
    cx: &mut Context<Shell>,
    on_press: impl Fn(&mut Shell) + 'static,
) -> Div {
    place(rect)
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(t.control_radius))
        .border_1()
        .border_color(paint(t.border))
        .bg(paint(if lit { accent } else { t.surface_raised }))
        .text_color(paint(if lit { t.on_accent } else { t.text_primary }))
        .text_size(px(t.type_small))
        .child(label.to_owned())
        .on_mouse_down(
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
    let frac = fraction(value, min, max) as f32;
    let vertical = rect.h >= rect.w;
    let thumb = if vertical {
        let travel = (rect.h - 12).max(0) as f32;
        place(Rect::new(
            rect.x,
            rect.y + ((1.0 - frac) * travel) as i32,
            rect.w,
            12,
        ))
    } else {
        let travel = (rect.w - 12).max(0) as f32;
        place(Rect::new(
            rect.x + (frac * travel) as i32,
            rect.y,
            12,
            rect.h,
        ))
    };
    let track = place(rect)
        .bg(paint(t.well))
        .border_1()
        .border_color(paint(t.border))
        .rounded(px(t.control_radius));
    let track = track.child(
        thumb
            .bg(paint(t.surface_raised))
            .border_1()
            .border_color(paint(t.highlight))
            .rounded(px(t.control_radius)),
    );
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
    let frac = fraction(value, min, max) as f32;
    let fill_h = ((rect.h - 4) as f32 * frac) as i32;
    let body = place(rect)
        .bg(paint(t.surface_raised))
        .border_1()
        .border_color(paint(t.border))
        .rounded(px(t.control_radius))
        .text_size(px(t.type_caption))
        .text_color(paint(t.text_secondary))
        .child(
            place(Rect::new(0, rect.h - fill_h - 2, rect.w, fill_h))
                .bg(paint(t.accent_for(0)))
                .rounded(px(2.)),
        )
        .child(
            div()
                .absolute()
                .left(px(4.))
                .top(px(2.))
                .child(label.to_owned()),
        )
        .child(
            div()
                .absolute()
                .left(px(4.))
                .bottom(px(2.))
                .text_color(paint(t.text_primary))
                .child(readout.to_owned()),
        );
    draggable(body, target, abs, cx)
}

/// One stereo meter column (two bars). `levels` are 0..=1 proportions; a lit clip lamp sits on top.
pub fn stereo_meter(t: &ThemeTokens, rect: Rect, levels: [f32; 2], clipping: [bool; 2]) -> Div {
    let half = rect.w / 2;
    let mut col = place(rect);
    for (i, (level, clip)) in levels.iter().zip(clipping).enumerate() {
        let x = i as i32 * half;
        let bar_h = ((rect.h - 14) as f32 * level.clamp(0.0, 1.0)) as i32;
        let bar = Rect::new(x + 1, rect.h - bar_h, (half - 2).max(1), bar_h);
        col = col.child(
            place(Rect::new(x, 14, half.max(1), (rect.h - 14).max(1)))
                .bg(paint(t.well))
                .rounded(px(2.)),
        );
        col = col.child(place(bar).bg(paint(if *level > 0.9 {
            t.meter_high
        } else if *level > 0.6 {
            t.meter_mid
        } else {
            t.meter_low
        })));
        col = col.child(
            place(Rect::new(x + 1, 2, (half - 2).max(1), 8))
                .bg(paint(if clip { t.meter_high } else { t.surface_raised }))
                .rounded(px(2.)),
        );
    }
    col
}

/// A text caption at `rect` (parent coordinates).
pub fn caption(t: &ThemeTokens, rect: Rect, text: &str) -> Div {
    place(rect)
        .text_size(px(t.type_caption))
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
