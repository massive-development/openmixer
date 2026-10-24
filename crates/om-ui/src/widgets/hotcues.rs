//! Eight hotcue pads. A click sets an empty cue at the playhead or jumps to a set one; Shift, Alt or
//! Ctrl with the click (or `press_pad` with `clear`) deletes it. Routes `deck.<N>.hotcue<i>` and
//! `...hotcue<i>.clear`.

use crate::binding::DeckBinding;
use crate::geometry::Rect;
use om_core::param_id::DeckParam;
use om_engine::deck::NUM_CUES;

/// Modifier keys held during a pad click.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

impl Modifiers {
    /// Any of the three means "clear".
    pub fn clears(self) -> bool {
        self.shift || self.alt || self.ctrl
    }
}

#[derive(Clone)]
pub struct HotcuePads {
    binding: DeckBinding,
}

impl HotcuePads {
    pub const COUNT: usize = NUM_CUES;

    pub fn new(binding: DeckBinding) -> Self {
        Self { binding }
    }

    /// A pad click with the modifiers held at the time.
    pub fn click(&self, index: usize, modifiers: Modifiers) -> bool {
        self.press_pad(index, modifiers.clears())
    }

    /// Sets or jumps (`clear` false) or deletes (`clear` true) one cue. Out-of-range indices send
    /// nothing and return `false`.
    pub fn press_pad(&self, index: usize, clear: bool) -> bool {
        if index >= Self::COUNT {
            return false;
        }
        let index = index as u8;
        self.binding.send(DeckParam::Hotcue { index, clear }, 1.0);
        true
    }

    /// Whether each pad's cue is set, read from the engine.
    pub fn lit(&self) -> [bool; Self::COUNT] {
        let cues = self.binding.state().hotcues;
        std::array::from_fn(|i| cues[i].is_some())
    }

    /// Pads in two rows of four across `area`, `gap` apart. Each pad is no shorter than the 44 px
    /// touch floor when `area` allows; spare height beyond [`super::deck::CUE_PAD_MAX`] is left
    /// empty under the pads.
    pub fn layout(area: Rect, gap: i32) -> [Rect; Self::COUNT] {
        let cols = (Self::COUNT / 2) as i32;
        let w = (area.w - gap * (cols - 1)) / cols;
        let h = ((area.h - gap) / 2).clamp(0, super::deck::CUE_PAD_MAX);
        std::array::from_fn(|i| {
            let (row, col) = ((i as i32) / cols, (i as i32) % cols);
            Rect::new(area.x + col * (w + gap), area.y + row * (h + gap), w, h)
        })
    }

    /// The colour of pad `index`'s cue (wraps past the palette).
    pub fn colour(index: usize) -> crate::theme::Colour {
        super::deck_header::CUE_PALETTE[index % super::deck_header::CUE_PALETTE.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pads_form_two_rows_of_four_at_least_44_px_square() {
        let pads = HotcuePads::layout(Rect::new(0, 0, 200, 108), 6);
        assert_eq!(pads[0].y, pads[3].y);
        assert!(pads[4].y >= pads[0].bottom());
        assert!(pads.iter().all(|p| p.w >= 44 && p.h >= 44), "{pads:?}");
        for (i, a) in pads.iter().enumerate() {
            for b in &pads[i + 1..] {
                assert!(!a.intersects(b));
            }
        }
    }
}
