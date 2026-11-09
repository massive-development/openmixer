//! Conversions from om-ui's models to GPUI values (colours, absolutely placed boxes) and the depth
//! of the faceplate: a 1px edge, a 1px top highlight inside it, the inset shadow of a press, the
//! one small drop under a raised cap, and the keyboard focus ring. Nothing here glows or blurs
//! beyond the drop. How GPUI draws each one: `docs/spikes/DECISIONS.md`, "Faceplate P3".

use gpui::{
    canvas, div, point, prelude::*, px, quad, rgba, transparent_black, BorderStyle, Bounds,
    BoxShadow, Corners, Div, FocusHandle, Pixels, Rgba, Window,
};
use om_ui::geometry::Rect;
use om_ui::style::{ControlStyle, FaceStyle, RingStyle, ShadowStyle};
use om_ui::theme::Colour;

/// om-ui stores ARGB; GPUI's `rgba` takes RRGGBBAA.
pub fn paint(colour: Colour) -> Rgba {
    let argb = colour.argb();
    rgba(((argb & 0x00FF_FFFF) << 8) | (argb >> 24))
}

/// Black at `alpha`: the scrim behind an overlay, so the window shows through.
pub fn scrim(alpha: f32) -> Rgba {
    Rgba {
        a: alpha.clamp(0.0, 1.0),
        ..rgba(0x0000_00ff)
    }
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
pub fn border<E: Styled>(el: E, width: f32) -> E {
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

/// Draws `el` (an element or a hover/press refinement) as a control face: fill, label colour, the
/// 1px edge (dashed when disabled), and the highlight or press shadow inside the edge.
pub fn face<E: Styled>(el: E, style: &FaceStyle) -> E {
    let el = border(el, style.edge_width)
        .border_color(paint(style.edge))
        .bg(paint(style.fill))
        .text_color(paint(style.text))
        .rounded(px(style.radius))
        .shadow(face_shadows(style));
    if style.dashed {
        el.border_dashed()
    } else {
        el
    }
}

/// The inset shadows of a face: the top highlight (`inset 0 1px 0` inside the edge) and the press
/// (`inset 0 1px 2px`). GPUI clips an inset shadow to the element's outer edge and paints the
/// border over it, so both are pulled in by the edge width to land inside the border, as CSS does.
pub fn face_shadows(style: &FaceStyle) -> Vec<BoxShadow> {
    let mut shadows = Vec::with_capacity(2);
    if !style.highlight.is_transparent() {
        shadows.push(inset(style.highlight, 1.0, 0.0, style.edge_width));
    }
    if !style.sink.colour.is_transparent() {
        let s = style.sink;
        shadows.push(inset(s.colour, s.offset_y, s.blur, style.edge_width));
    }
    shadows
}

fn inset(colour: Colour, offset_y: f32, blur: f32, spread: f32) -> BoxShadow {
    BoxShadow::new(px(0.), px(offset_y), paint(colour).into())
        .blur_radius(px(blur))
        .spread_radius(px(spread))
        .inset()
}

/// The small drop under a raised cap (`0 2px 3px` or `0 3px 5px`), as a box shadow.
pub fn drop_shadow(style: &ShadowStyle) -> BoxShadow {
    BoxShadow::new(px(0.), px(style.offset_y), paint(style.colour).into())
        .blur_radius(px(style.blur))
}

/// Paints a raised face into `bounds` for a painted control (a knob cap): the drop, the fill and
/// the 1px etched edge, with corners of `radius` (half the side for a disc).
pub fn paint_raised(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    radius: f32,
    (fill, edge): (Rgba, Rgba),
    drop: &ShadowStyle,
) {
    let corners = Corners::all(px(radius));
    window.paint_drop_shadows(bounds, corners, &[drop_shadow(drop)]);
    window.paint_quad(quad(
        bounds,
        corners,
        fill,
        px(1.),
        edge,
        BorderStyle::Solid,
    ));
}

/// Paints the keyboard focus ring around `bounds`: `ring.width` wide, `ring.offset` outside the
/// control's edge, its corners following the control's `radius` (a circle stays a circle).
pub fn paint_focus_ring(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    radius: f32,
    ring: &RingStyle,
) {
    let reach = ring.offset + ring.width;
    window.paint_quad(quad(
        bounds.dilate(px(reach)),
        px(radius + reach),
        transparent_black(),
        px(ring.width),
        paint(ring.colour),
        BorderStyle::Solid,
    ));
}

/// A layer over its parent that draws the parent's focus ring while `handle` has keyboard focus
/// (focus that came from the keyboard, as CSS `:focus-visible`), or always with no handle (the
/// gallery's forced state). It takes no pointer input. The parent must not clip its overflow.
pub fn focus_layer(ring: RingStyle, radius: f32, handle: Option<FocusHandle>) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            let shown = handle
                .as_ref()
                .is_none_or(|h| h.is_focused(window) && window.last_input_was_keyboard());
            if shown {
                paint_focus_ring(window, bounds, radius, &ring);
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// A point `(x, y)` px from `origin`.
pub fn offset(origin: gpui::Point<Pixels>, (x, y): (f32, f32)) -> gpui::Point<Pixels> {
    point(origin.x + px(x), origin.y + px(y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_ui::style::{Face, LookAndFeel};
    use om_ui::theme::ThemeTokens;

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

    #[test]
    fn the_highlight_and_the_press_sit_inside_the_edge() {
        let t = ThemeTokens::builtin_dark();
        let look = LookAndFeel::from_tokens(&t);
        let rest = face_shadows(&look.face(Face::Rest, t.accent_for(0)));
        assert_eq!(rest.len(), 1);
        let h = &rest[0];
        assert!(h.inset && h.blur_radius == px(0.) && h.offset.y == px(1.));
        assert_eq!(h.spread_radius, px(t.border_width), "pulled in by the edge");
        let pressed = face_shadows(&look.face(Face::Pressed, t.accent_for(0)));
        assert_eq!(pressed.len(), 1);
        assert!(pressed[0].inset && pressed[0].blur_radius == px(2.));
        for f in [Face::On, Face::Playing, Face::Disabled] {
            assert!(face_shadows(&look.face(f, t.accent_for(0))).is_empty());
        }
    }

    #[test]
    fn the_drop_is_not_inset() {
        let look = LookAndFeel::from_tokens(&ThemeTokens::builtin_dark());
        let d = drop_shadow(&look.lift);
        assert!(!d.inset && d.offset.y == px(2.) && d.blur_radius == px(3.));
    }
}
