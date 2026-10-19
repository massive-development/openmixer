//! Conversions from om-ui's models to GPUI values: colours and absolutely placed boxes.

use gpui::{div, prelude::*, px, rgba, Div, Rgba};
use om_ui::geometry::Rect;
use om_ui::style::ControlStyle;
use om_ui::theme::Colour;

/// om-ui stores ARGB; GPUI's `rgba` takes RRGGBBAA.
pub fn paint(colour: Colour) -> Rgba {
    let argb = colour.argb();
    rgba(((argb & 0x00FF_FFFF) << 8) | (argb >> 24))
}

/// An empty box placed at `rect` inside its positioned parent.
pub fn place(rect: Rect) -> Div {
    div()
        .absolute()
        .left(px(rect.x as f32))
        .top(px(rect.y as f32))
        .w(px(rect.w as f32))
        .h(px(rect.h as f32))
}

/// Gives `el` a border `width` px wide. gpui only has fixed widths (0 to 12 px), so a token's width
/// is rounded to the nearest whole pixel and clamped to that range.
pub fn border(el: Div, width: f32) -> Div {
    match width.round().clamp(0.0, 12.0) as u32 {
        0 => el.border_0(),
        1 => el.border_1(),
        2 => el.border_2(),
        3 => el.border_3(),
        4 => el.border_4(),
        5 => el.border_5(),
        6 => el.border_6(),
        7 => el.border_7(),
        8 => el.border_8(),
        9 => el.border_9(),
        10 => el.border_10(),
        11 => el.border_11(),
        _ => el.border_12(),
    }
}

/// Draws `el` in a control style: fill, text colour, border, radius and font size.
pub fn skin(el: Div, style: &ControlStyle) -> Div {
    border(el, style.border_width)
        .border_color(paint(style.border))
        .bg(paint(style.fill))
        .text_color(paint(style.text))
        .text_size(px(style.font_size))
        .rounded(px(style.radius))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argb_becomes_rgba() {
        // Opaque red: ARGB 0xFFFF0000 -> RGBA 0xFF0000FF.
        let red = paint(Colour::from_argb(0xFFFF_0000));
        assert!((red.r - 1.0).abs() < 1e-6);
        assert!(red.g.abs() < 1e-6 && red.b.abs() < 1e-6);
        assert!((red.a - 1.0).abs() < 1e-6);
        // Half-transparent green: alpha 0x80 must land in the low byte.
        let green = paint(Colour::from_argb(0x8000_FF00));
        assert!((green.g - 1.0).abs() < 1e-6);
        assert!((green.a - 128.0 / 255.0).abs() < 1e-3);
    }
}
