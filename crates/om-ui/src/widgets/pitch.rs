//! Tempo fader with a selectable range (+-8 / 16 / 50 %). Routes `deck.<N>.rate`.

use crate::binding::DeckBinding;
use crate::control::Ranged;
use crate::format;
use om_core::param_id::DeckParam;

/// Selectable ranges as fractions of the original tempo.
pub const RANGES: [f64; 3] = [0.08, 0.16, 0.5];
/// Fader resolution.
pub const STEP: f64 = 0.0005;

pub struct PitchFader {
    binding: DeckBinding,
    range_index: usize,
    fader: Ranged,
}

impl PitchFader {
    /// Starts on the narrowest range. That may pull the tempo back
    /// into range if the engine is already outside it.
    pub fn new(binding: DeckBinding) -> Self {
        let mut pitch = Self {
            binding,
            range_index: 0,
            fader: Ranged::new(1.0 - RANGES[0], 1.0 + RANGES[0], STEP, 1.0),
        };
        pitch.set_range_index(0);
        pitch
    }

    pub fn range(&self) -> f64 {
        RANGES[self.range_index]
    }

    pub fn range_index(&self) -> usize {
        self.range_index
    }

    /// The fader position (a rate, 1.0 = original tempo).
    pub fn fader_value(&self) -> f64 {
        self.fader.value()
    }

    /// Text shown under the fader.
    pub fn readout(&self) -> String {
        format::rate(self.fader.value())
    }

    /// Selects a range (clamped to the list). The current rate is kept when it fits; otherwise it is
    /// pulled back inside and the new rate is sent. Returns the rate sent, if any.
    pub fn set_range_index(&mut self, index: usize) -> Option<f64> {
        self.range_index = index.min(RANGES.len() - 1);
        let r = self.range();
        self.fader.set_range(1.0 - r, 1.0 + r, STEP);
        let engine_rate = self.binding.deck().rate();
        let fitted = engine_rate.clamp(1.0 - r, 1.0 + r);
        self.fader.set(fitted);
        if fitted == engine_rate {
            return None;
        }
        self.binding.send(DeckParam::Rate, fitted);
        Some(fitted)
    }

    /// The user moved the fader: stores the snapped value and sends it. Returns the value sent.
    pub fn user_set(&mut self, value: f64) -> f64 {
        let stored = self.fader.set(value);
        self.binding.send(DeckParam::Rate, stored);
        stored
    }

    /// Follows rate changes made elsewhere (sync nudges, MIDI) without echoing them back. The
    /// fader is left alone while the user drags it.
    pub fn refresh(&mut self, dragging: bool) {
        let rate = self.binding.deck().rate();
        if !dragging && (self.fader.value() - rate).abs() > STEP {
            self.fader.set(rate);
        }
    }
}
