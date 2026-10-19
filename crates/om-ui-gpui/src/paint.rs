//! Conversions from om-ui's models to GPUI values: colours and absolutely placed boxes.

use gpui::{div, prelude::*, px, rgba, Div, Rgba};
use om_ui::geometry::Rect;
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
