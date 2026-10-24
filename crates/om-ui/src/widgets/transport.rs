//! Deck transport: play, cue, sync, master, key-lock, quantize, slip. Every button routes a
//! `deck.<N>.<id>` parameter; toggles send the opposite of the engine's current state.

use super::px;
use crate::binding::DeckBinding;
use crate::geometry::Rect;
use crate::theme::{Colour, ThemeTokens};
use om_core::param_id::DeckParam;
use om_core::DeckSnapshot;

/// Transport buttons, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportButton {
    Play,
    Cue,
    Sync,
    Master,
    KeyLock,
    Quantize,
    Slip,
}

impl TransportButton {
    pub const ALL: [TransportButton; 7] = [
        TransportButton::Play,
        TransportButton::Cue,
        TransportButton::Sync,
        TransportButton::Master,
        TransportButton::KeyLock,
        TransportButton::Quantize,
        TransportButton::Slip,
    ];

    /// The buttons of the always-visible transport row, in display order.
    pub const PRIMARY: [TransportButton; 3] = [
        TransportButton::Play,
        TransportButton::Cue,
        TransportButton::Sync,
    ];

    /// The toggles in the More drawer, in display order.
    pub const SECONDARY: [TransportButton; 4] = [
        TransportButton::Master,
        TransportButton::KeyLock,
        TransportButton::Quantize,
        TransportButton::Slip,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            TransportButton::Play => "Play",
            TransportButton::Cue => "CUE",
            TransportButton::Sync => "SYNC",
            TransportButton::Master => "Master",
            TransportButton::KeyLock => "Key lock",
            TransportButton::Quantize => "Quantize",
            TransportButton::Slip => "Slip",
        }
    }

    /// The deck parameter the button sends. `None` for Cue, which is not a parameter: it sends
    /// play and seek instead (see [`TransportPanel::press`]).
    pub const fn route(self) -> Option<DeckParam> {
        match self {
            TransportButton::Play => Some(DeckParam::Play),
            TransportButton::Cue => None,
            TransportButton::Sync => Some(DeckParam::Sync),
            TransportButton::Master => Some(DeckParam::SyncMaster),
            TransportButton::KeyLock => Some(DeckParam::Keylock),
            TransportButton::Quantize => Some(DeckParam::Quantize),
            TransportButton::Slip => Some(DeckParam::Slip),
        }
    }

    /// Cue stops and returns to the start; every other button is a toggle.
    pub const fn is_toggle(self) -> bool {
        !matches!(self, TransportButton::Cue)
    }

    pub const fn tooltip(self) -> &'static str {
        match self {
            TransportButton::Play => "Play / pause",
            TransportButton::Cue => "Stop and return to start",
            TransportButton::Sync => "Follow the sync leader",
            TransportButton::Master => "Make this deck the sync master",
            TransportButton::KeyLock => "Key lock: tempo changes keep the pitch",
            TransportButton::Quantize => "Quantize loop starts to the beat grid",
            TransportButton::Slip => "Slip mode: the track keeps running under loops and scratches",
        }
    }

    /// Lit colour: the deck accent, except Master which uses the master colour.
    pub fn lit_colour(self, theme: &ThemeTokens, deck: usize) -> Colour {
        match self {
            TransportButton::Master => theme.master,
            _ => theme.accent_for(deck as i32),
        }
    }
}

/// Whether a toggle is on in one deck state. Cue is never a toggle.
fn lit(button: TransportButton, s: &DeckSnapshot) -> bool {
    match button {
        TransportButton::Play => s.playing,
        TransportButton::Cue => false,
        TransportButton::Sync => s.sync,
        TransportButton::Master => s.sync_master,
        TransportButton::KeyLock => s.keylock,
        TransportButton::Quantize => s.quantize,
        TransportButton::Slip => s.slip,
    }
}

/// The transport of one deck.
#[derive(Clone)]
pub struct TransportPanel {
    binding: DeckBinding,
}

impl TransportPanel {
    pub fn new(binding: DeckBinding) -> Self {
        Self { binding }
    }

    /// Whether a toggle is currently on, read from the engine.
    pub fn state(&self, button: TransportButton) -> bool {
        lit(button, &self.binding.state())
    }

    /// Presses a button. Toggles send the opposite of their current state. Returns the value sent.
    pub fn press(&self, button: TransportButton) -> f64 {
        let Some(param) = button.route() else {
            // No main-cue point exists in the engine yet: Cue stops and returns to the start.
            self.binding.send(DeckParam::Play, 0.0);
            self.binding.send(DeckParam::Seek, 0.0);
            return 0.0;
        };
        let value = if self.state(button) { 0.0 } else { 1.0 };
        self.binding.send(param, value);
        value
    }

    /// The toggle states to show, in [`TransportButton::ALL`] order. Cue is always off.
    pub fn toggle_states(&self) -> [bool; 7] {
        let state = self.binding.state();
        TransportButton::ALL.map(|b| b.is_toggle() && lit(b, &state))
    }

    /// Whether one button is lit, from a single engine read.
    pub fn is_lit(&self, button: TransportButton) -> bool {
        button.is_toggle() && self.state(button)
    }

    /// Play reads "Pause" while the deck plays.
    pub fn play_label(&self) -> &'static str {
        if self.binding.state().playing {
            "Pause"
        } else {
            "Play"
        }
    }

    /// The primary row: Play takes twice the width of Cue and Sync, `gap` between the cells.
    pub fn layout(area: Rect, gap: i32) -> [Rect; 3] {
        let mut area = area;
        let unit = (area.w - gap * 2) / 4;
        let play = area.remove_left(unit * 2);
        area.remove_left(gap);
        let cue = area.remove_left(unit);
        area.remove_left(gap);
        [play, cue, area]
    }

    /// The More drawer: six cells in two columns and three rows, each 32 px or taller (up to 44
    /// when the area allows). Master, key lock, quantize and slip use the first four; the gpui layer
    /// puts the stems on/off toggle in the fifth.
    pub fn secondary_layout(area: Rect, gap: i32) -> [Rect; 6] {
        let w = (area.w - gap) / 2;
        let h = ((area.h - gap * 2) / 3).clamp(0, 44);
        std::array::from_fn(|i| {
            let (row, col) = ((i / 2) as i32, (i % 2) as i32);
            Rect::new(area.x + col * (w + gap), area.y + row * (h + gap), w, h)
        })
    }
}

/// Rounded pixel height of the transport row for a theme.
pub fn transport_height(theme: &ThemeTokens) -> i32 {
    px(theme.deck_transport_height)
}
