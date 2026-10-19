//! View models of the deck and mixer widgets: state, routing and layout, without drawing. Each
//! model reads the engine through its binding and writes through the control surface.

pub mod deck;
pub mod hotcues;
pub mod loops;
pub mod pitch;
pub mod transport;

/// Height of a caption strip above a captioned control (px).
pub const CAPTION_HEIGHT: i32 = 16;

/// Rounds a theme metric to whole pixels, as the C++ `theme::*()` accessors did.
pub fn px(value: f32) -> i32 {
    value.round() as i32
}
