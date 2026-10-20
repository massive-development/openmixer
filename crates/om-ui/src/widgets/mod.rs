//! View models of the deck and mixer widgets: state, routing and layout, without drawing. Each
//! model reads the engine through its binding and writes through the control surface.

pub mod channel;
pub mod crossfader;
pub mod cue;
pub mod deck;
pub mod fx_panel;
pub mod hotcues;
pub mod loops;
pub mod master;
pub mod mixer;
pub mod pitch;
pub mod stem_pads;
pub mod transport;

/// Height of a caption strip above a captioned control (px).
pub const CAPTION_HEIGHT: i32 = 16;

/// Rounds a theme metric to whole pixels.
pub fn px(value: f32) -> i32 {
    value.round() as i32
}
