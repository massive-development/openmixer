//! Eight hotcue pads. A click sets an empty cue at the playhead or jumps to a set one; Shift, Alt or
//! Ctrl with the click (or `press_pad` with `clear`) deletes it. Routes `deck.<N>.hotcue<i>` and
//! `...hotcue<i>.clear`.

use crate::binding::DeckBinding;
use crate::geometry::Rect;
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
        let id = if clear {
            format!("hotcue{index}.clear")
        } else {
            format!("hotcue{index}")
        };
        self.binding.send(&id, 1.0);
        true
    }

    /// Whether each pad's cue is set, read from the engine.
    pub fn lit(&self) -> [bool; Self::COUNT] {
        std::array::from_fn(|i| self.binding.deck().hotcue(i).is_some())
    }

    /// Pads in one row across `area`, `gap` apart.
    pub fn layout(area: Rect, gap: i32) -> [Rect; Self::COUNT] {
        let n = Self::COUNT as i32;
        let w = (area.w - gap * (n - 1)) / n;
        std::array::from_fn(|i| {
            let i = i as i32;
            Rect::new(area.x + i * (w + gap), area.y, w, area.h)
        })
    }
}
