//! Quantized loop controls: eight loop sizes, halve and double, exit and reloop. Sizes route
//! `deck.<N>.loop.quantized.<beats>`; exit and reloop route `loop.exit` and `loop.reloop`.

use crate::binding::DeckBinding;
use crate::geometry::Rect;
use om_core::param_id::DeckParam;
use om_core::DeckSnapshot;
use om_engine::grid::BeatGrid;

/// Loop sizes in beats, in button order.
pub const SIZES: [f64; 8] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0];
pub const SIZE_NAMES: [&str; 8] = ["1/4", "1/2", "1", "2", "4", "8", "16", "32"];
/// Beats; a loop snapped to the grid is within rounding of its size.
pub const MATCH_TOLERANCE: f64 = 0.01;

/// The route id for a loop size. Keeps the decimal point, as in `1.0` and `0.25`.
pub fn loop_param(beats: f64) -> DeckParam {
    DeckParam::LoopQuantized(beats)
}

#[derive(Clone)]
pub struct LoopControls {
    binding: DeckBinding,
}

/// What the loop row shows for one frame. Every field comes from the same deck snapshot, so the lit
/// buttons and the length they match cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoopFrame {
    /// Each size button: a loop is active and its length matches the size.
    pub size_lit: [bool; 8],
    /// Reloop is lit while a loop is active.
    pub reloop_lit: bool,
    /// Length of the current loop in beats; 0 when no loop is set or there is no grid.
    pub length_beats: f64,
}

impl LoopFrame {
    /// Text of the size button: the loop's length, or the length a press would start while idle.
    pub fn label(&self) -> String {
        if self.length_beats > 0.0 {
            crate::widgets::deck_header::loop_label(self.length_beats)
        } else {
            SIZE_NAMES[DEFAULT_SIZE_INDEX].to_owned()
        }
    }
}

/// Button rectangles of the loop row: halve, the size readout (also the loop on/off button),
/// double.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoopLayout {
    pub halve: Rect,
    pub size: Rect,
    pub double: Rect,
}

/// Index in [`SIZES`] of the loop the size button starts when none is set: 4 beats.
pub const DEFAULT_SIZE_INDEX: usize = 4;

impl LoopFrame {
    /// The frame for one deck snapshot, read against the deck's grid. Pure: no engine access.
    pub fn from_state(state: &DeckSnapshot, grid: Option<&dyn BeatGrid>) -> Self {
        let length_beats = match grid {
            Some(grid) if state.loop_valid() => {
                grid.beat_at_frame(state.loop_end) - grid.beat_at_frame(state.loop_start)
            }
            _ => 0.0,
        };
        Self {
            size_lit: std::array::from_fn(|i| {
                state.looping && (length_beats - SIZES[i]).abs() < MATCH_TOLERANCE
            }),
            reloop_lit: state.looping,
            length_beats,
        }
    }
}

impl LoopControls {
    pub fn new(binding: DeckBinding) -> Self {
        Self { binding }
    }

    /// The row's display state for this frame: one deck snapshot, every value derived from it.
    pub fn frame(&self) -> LoopFrame {
        let state = self.binding.state();
        let grid = self.binding.control().grid();
        LoopFrame::from_state(&state, grid.as_deref())
    }

    /// Sets a loop of size `index` from the playhead (or the nearest grid step).
    pub fn press_size(&self, index: usize) -> bool {
        match SIZES.get(index) {
            Some(&beats) => self.binding.send(loop_param(beats), 1.0),
            None => false,
        }
    }

    /// Halves the current loop. Returns the size sent, or `None` when there is no loop to scale.
    pub fn halve(&self) -> Option<f64> {
        self.resize(0.5)
    }

    /// Doubles the current loop. Returns the size sent, or `None` when there is no loop to scale.
    pub fn double(&self) -> Option<f64> {
        self.resize(2.0)
    }

    fn resize(&self, factor: f64) -> Option<f64> {
        let beats = self.frame().length_beats;
        if beats <= 0.0 {
            return None;
        }
        let next = (beats * factor).clamp(SIZES[0], SIZES[SIZES.len() - 1]);
        self.binding.send(loop_param(next), 1.0);
        Some(next)
    }

    pub fn exit(&self) -> bool {
        self.binding.send(DeckParam::LoopExit, 1.0)
    }

    pub fn reloop(&self) -> bool {
        self.binding.send(DeckParam::LoopReloop, 1.0)
    }

    /// One row: halve and double (40 px each, over the 32 px floor) around the size button, which
    /// takes the rest.
    pub fn layout(area: Rect, gap: i32) -> LoopLayout {
        let mut area = area;
        let side = 40.min(area.w / 3);
        let halve = area.remove_left(side);
        area.remove_left(gap);
        let double = area.remove_right(side);
        area.remove_right(gap);
        LoopLayout {
            halve,
            size: area,
            double,
        }
    }

    /// The size button: exits an active loop, re-enters the last one, or starts a loop of
    /// `SIZES[DEFAULT_SIZE_INDEX]` beats when none is set. Returns whether it sent.
    pub fn toggle(&self) -> bool {
        let state = self.binding.state();
        if state.looping {
            self.exit()
        } else if state.loop_valid() {
            self.reloop()
        } else {
            self.press_size(DEFAULT_SIZE_INDEX)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_sizes_round_trip_through_the_router_grammar() {
        for beats in [0.25, 0.5, 1.0, 2.0, 4.0] {
            let id = om_core::ParamId::Deck {
                deck: 0,
                p: loop_param(beats),
            };
            let text = id.to_string();
            assert_eq!(text.parse::<om_core::ParamId>(), Ok(id), "{text}");
        }
        assert_eq!(loop_param(0.25).to_string(), "loop.quantized.0.25");
    }

    #[test]
    fn the_loop_row_is_halve_size_double_with_the_size_in_the_middle() {
        let l = LoopControls::layout(Rect::new(0, 0, 200, 36), 6);
        assert!(l.halve.right() < l.size.x && l.size.right() < l.double.x);
        assert!(l.halve.w >= 32 && l.double.w >= 32 && l.size.w >= 64);
        assert_eq!(l.size.h, 36);
    }

    #[test]
    fn the_size_label_follows_the_loop_or_offers_the_default() {
        let idle = LoopFrame {
            size_lit: [false; 8],
            reloop_lit: false,
            length_beats: 0.0,
        };
        assert_eq!(idle.label(), "4");
        assert_eq!(
            LoopFrame {
                length_beats: 0.25,
                ..idle
            }
            .label(),
            "1/4"
        );
    }
}
