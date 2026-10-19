//! Quantized loop controls: eight loop sizes, halve and double, exit and reloop. Sizes route
//! `deck.<N>.loop.quantized.<beats>`; exit and reloop route `loop.exit` and `loop.reloop`.

use crate::binding::DeckBinding;
use crate::geometry::Rect;

/// Loop sizes in beats, in button order.
pub const SIZES: [f64; 8] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0];
pub const SIZE_NAMES: [&str; 8] = ["1/4", "1/2", "1", "2", "4", "8", "16", "32"];
/// Beats; a loop snapped to the grid is within rounding of its size.
pub const MATCH_TOLERANCE: f64 = 0.01;

/// The route id for a loop size. Keeps the decimal point, as in `1.0` and `0.25`.
pub fn loop_id(beats: f64) -> String {
    format!("loop.quantized.{beats:?}")
}

#[derive(Clone)]
pub struct LoopControls {
    binding: DeckBinding,
}

/// Button rectangles of the loop row.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopLayout {
    pub sizes: [Rect; 8],
    pub halve: Rect,
    pub double: Rect,
    pub exit: Rect,
    pub reloop: Rect,
}

impl LoopControls {
    pub fn new(binding: DeckBinding) -> Self {
        Self { binding }
    }

    /// Length of the current loop in beats; 0 when no loop is set or there is no grid.
    pub fn current_length_beats(&self) -> f64 {
        let Some(grid) = self.binding.control().grid() else {
            return 0.0;
        };
        let lp = self.binding.deck().loop_state();
        if !lp.valid() {
            return 0.0;
        }
        grid.beat_at_frame(lp.end) - grid.beat_at_frame(lp.start)
    }

    /// Sets a loop of size `index` from the playhead (or the nearest grid step).
    pub fn press_size(&self, index: usize) -> bool {
        match SIZES.get(index) {
            Some(&beats) => self.binding.send(&loop_id(beats), 1.0),
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
        let beats = self.current_length_beats();
        if beats <= 0.0 {
            return None;
        }
        let next = (beats * factor).clamp(SIZES[0], SIZES[SIZES.len() - 1]);
        self.binding.send(&loop_id(next), 1.0);
        Some(next)
    }

    pub fn exit(&self) -> bool {
        self.binding.send("loop.exit", 1.0)
    }

    pub fn reloop(&self) -> bool {
        self.binding.send("loop.reloop", 1.0)
    }

    /// Whether each size button is lit: a loop is active and its length matches the size.
    pub fn size_lit(&self) -> [bool; 8] {
        let active = self.binding.deck().is_looping();
        let beats = self.current_length_beats();
        std::array::from_fn(|i| active && (beats - SIZES[i]).abs() < MATCH_TOLERANCE)
    }

    /// Whether reloop is lit (a loop is active).
    pub fn reloop_lit(&self) -> bool {
        self.binding.deck().is_looping()
    }

    /// Two rows: the size buttons on top, the actions below. `gap` between buttons and rows.
    pub fn layout(area: Rect, gap: i32) -> LoopLayout {
        let mut area = area;
        let row_h = (area.h - gap) / 2;
        let mut top = area.remove_top(row_h);
        area.remove_top(gap);
        let w = (top.w - gap * 7) / 8;
        let sizes = std::array::from_fn(|_| {
            let r = top.remove_left(w);
            top.remove_left(gap);
            r
        });
        let cell = (area.w - gap * 3) / 4;
        let mut actions = (0..4).map(|_| {
            let r = area.remove_left(cell);
            area.remove_left(gap);
            r
        });
        LoopLayout {
            sizes,
            halve: actions.next().unwrap_or_default(),
            double: actions.next().unwrap_or_default(),
            exit: actions.next().unwrap_or_default(),
            reloop: actions.next().unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_ids_keep_the_decimal_point() {
        assert_eq!(loop_id(0.25), "loop.quantized.0.25");
        assert_eq!(loop_id(1.0), "loop.quantized.1.0");
    }
}
