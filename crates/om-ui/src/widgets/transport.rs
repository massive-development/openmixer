//! Deck transport: play, cue, sync, master, key-lock, quantize, slip. Every button routes a
//! `deck.<N>.<id>` parameter; toggles send the opposite of the engine's current state.

use super::px;
use crate::binding::DeckBinding;
use crate::geometry::Rect;
use crate::theme::{Colour, ThemeTokens};
use om_core::param_id::DeckParam;

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

    pub const fn label(self) -> &'static str {
        match self {
            TransportButton::Play => "Play",
            TransportButton::Cue => "Cue",
            TransportButton::Sync => "Sync",
            TransportButton::Master => "Mstr",
            TransportButton::KeyLock => "Key",
            TransportButton::Quantize => "Qnt",
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
        let b = &self.binding;
        match button {
            TransportButton::Play => b.deck().is_playing(),
            TransportButton::Cue => false,
            TransportButton::Sync => b.sync().is_enabled(b.index),
            TransportButton::Master => b.sync().leader() == Some(b.index),
            TransportButton::KeyLock => b.deck().is_keylock(),
            TransportButton::Quantize => b.control().is_quantize(),
            TransportButton::Slip => b.control().is_slipping(),
        }
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
        TransportButton::ALL.map(|b| b.is_toggle() && self.state(b))
    }

    /// Play reads "Pause" while the deck plays.
    pub fn play_label(&self) -> &'static str {
        if self.binding.deck().is_playing() {
            "Pause"
        } else {
            "Play"
        }
    }

    /// Seven equal cells across `area`, with half an inner gap between them.
    pub fn layout(area: Rect, inner_gap: i32) -> [Rect; 7] {
        let mut area = area;
        let half = inner_gap / 2;
        let cell = (area.w - half * 6) / 7;
        TransportButton::ALL.map(|_| {
            let r = area.remove_left(cell);
            area.remove_left(half);
            r
        })
    }
}

/// Rounded pixel height of the transport row for a theme.
pub fn transport_height(theme: &ThemeTokens) -> i32 {
    px(theme.deck_transport_height)
}
