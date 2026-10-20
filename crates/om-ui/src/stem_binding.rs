//! The stem pads of one deck: per-stem mute, volume and filter, a UI-level solo, and an all-on
//! reset. Reads come from the engine's stem snapshot; writes are typed `stem.<d>.<s>.<p>`
//! [`Command::Param`]s (or the string façade when there is no sink), like [`DeckBinding`].
//!
//! Solo is not an engine feature. Pressing solo on a stem mutes the other three and keeps the
//! mutes they had; pressing it again (or the all-on reset) restores them. Mute toggles made while a
//! solo is active update the saved mutes too, so the restore reflects what the user last chose.

use std::sync::{Arc, Mutex};

use om_core::param_id::{ParamError, StemParam, STEMS_PER_DECK};
use om_core::sync::LockExt;
use om_core::{CommandSink, ParamId, ParameterRouter, ParameterTarget, StemSnapshot};
use om_dsp::STEM_COUNT;
use om_engine::DjEngine;

use crate::binding::{deliver, DeckBinding};

/// Stem names as the pads show them, in stem index order (the cache writes the same order).
pub const STEM_NAMES: [&str; STEM_COUNT] = ["Drums", "Bass", "Vocals", "Other"];

/// Upper end of the volume range (linear gain).
pub const MAX_VOLUME: f64 = 1.5;

/// The solo state of one deck: which stem is soloed, and the mutes to restore.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoloState {
    pub stem: Option<usize>,
    pub saved_mutes: [bool; STEM_COUNT],
}

impl Default for SoloState {
    fn default() -> Self {
        Self {
            stem: None,
            saved_mutes: [false; STEM_COUNT],
        }
    }
}

impl SoloState {
    /// The result of pressing solo on `stem` when the mutes are `current`: the new state, and the
    /// mute every stem should have. Pressing the soloed stem again restores the saved mutes; pressing
    /// another stem moves the solo and keeps the mutes saved by the first press.
    pub fn press(&self, stem: usize, current: [bool; STEM_COUNT]) -> (Self, [bool; STEM_COUNT]) {
        if self.stem == Some(stem) {
            let restored = Self {
                stem: None,
                saved_mutes: self.saved_mutes,
            };
            return (restored, self.saved_mutes);
        }
        let saved_mutes = if self.stem.is_some() {
            self.saved_mutes
        } else {
            current
        };
        let mutes = std::array::from_fn(|s| s != stem);
        (
            Self {
                stem: Some(stem),
                saved_mutes,
            },
            mutes,
        )
    }

    /// Records a mute the user chose during a solo, so the restore keeps it.
    pub fn note_mute(&mut self, stem: usize, muted: bool) {
        if self.stem.is_some() {
            if let Some(saved) = self.saved_mutes.get_mut(stem) {
                *saved = muted;
            }
        }
    }
}

/// Routes one deck's stem pads. Cheap to clone; clones share the solo state.
#[derive(Clone)]
pub struct StemBinding {
    pub engine: Arc<DjEngine>,
    /// Typed writes. `None` falls back to `target`.
    pub sink: Option<Arc<dyn CommandSink>>,
    /// String writes, used when there is no sink.
    pub target: Arc<dyn ParameterTarget>,
    pub index: usize,
    solo: Arc<Mutex<SoloState>>,
}

impl StemBinding {
    /// The stem pads of the same deck, through the same writes as `deck`.
    pub fn for_deck(deck: &DeckBinding) -> Self {
        Self {
            engine: Arc::clone(&deck.engine),
            sink: deck.sink.clone(),
            target: Arc::clone(&deck.target),
            index: deck.index,
            solo: Arc::new(Mutex::new(SoloState::default())),
        }
    }

    /// A binding that writes through the string façade (test rigs).
    pub fn new(engine: Arc<DjEngine>, target: Arc<dyn ParameterTarget>, index: usize) -> Self {
        Self {
            engine,
            sink: None,
            target,
            index,
            solo: Arc::new(Mutex::new(SoloState::default())),
        }
    }

    /// A binding that sends typed commands to `router` (the production window).
    pub fn typed(engine: Arc<DjEngine>, router: Arc<ParameterRouter>, index: usize) -> Self {
        Self {
            engine,
            sink: Some(router.clone()),
            target: router,
            index,
            solo: Arc::new(Mutex::new(SoloState::default())),
        }
    }

    /// The four stems as the engine reports them. All idle (`available: false`) when the deck is
    /// not in stem mode or the index is unknown.
    pub fn stems(&self) -> [StemSnapshot; STEM_COUNT] {
        self.engine
            .stem_snapshot(self.index)
            .unwrap_or([StemSnapshot::default(); STEM_COUNT])
    }

    /// True while the deck plays from stems. Pads are enabled only then.
    pub fn has_stems(&self) -> bool {
        self.stems()[0].available
    }

    /// The soloed stem, if any.
    pub fn soloed(&self) -> Option<usize> {
        self.solo.lock_unpoisoned().stem
    }

    /// The typed id of one stem parameter of this deck. Fails when the index or the stem does not
    /// fit the id ranges.
    pub fn id(&self, stem: usize, p: StemParam) -> Result<ParamId, ParamError> {
        let deck = u8::try_from(self.index).map_err(|_| ParamError::OutOfRange {
            id: format!("stem.{}", self.index),
            what: "deck",
            max: u16::from(om_core::param_id::MAX_DECKS) - 1,
        })?;
        let stem = u8::try_from(stem).unwrap_or(u8::MAX);
        let id = ParamId::Stem { deck, stem, p };
        id.validate()?;
        Ok(id)
    }

    /// Sends one stem parameter. `false` when the id is invalid or the router does not handle it.
    fn send(&self, stem: usize, p: StemParam, value: f64) -> bool {
        match self.id(stem, p) {
            Ok(id) => deliver(self.sink.as_ref(), &self.target, id, value),
            Err(_) => false,
        }
    }

    /// Sets the mute of one stem. During a solo the value is also remembered for the restore.
    pub fn set_mute(&self, stem: usize, muted: bool) -> bool {
        self.solo.lock_unpoisoned().note_mute(stem, muted);
        self.send(stem, StemParam::Mute, flag(muted))
    }

    /// Flips the mute of one stem.
    pub fn toggle_mute(&self, stem: usize) -> bool {
        let muted = self.stems().get(stem).is_some_and(|s| s.muted);
        self.set_mute(stem, !muted)
    }

    /// Sets the gain of one stem, clamped to 0..=1.5.
    pub fn set_volume(&self, stem: usize, gain: f64) -> bool {
        self.send(stem, StemParam::Volume, gain.clamp(0.0, MAX_VOLUME))
    }

    /// Sets the sweep filter of one stem, clamped to -1 (low-pass) .. +1 (high-pass).
    pub fn set_filter(&self, stem: usize, position: f64) -> bool {
        self.send(stem, StemParam::Filter, position.clamp(-1.0, 1.0))
    }

    /// Presses solo on `stem`: it plays alone, the others are muted; pressing it again restores the
    /// mutes from before the solo. Returns `false` when any write failed.
    pub fn solo(&self, stem: usize) -> bool {
        if stem >= STEMS_PER_DECK as usize {
            return false;
        }
        let current = self.stems().map(|s| s.muted);
        let mut state = self.solo.lock_unpoisoned();
        let (next, mutes) = state.press(stem, current);
        *state = next;
        drop(state);
        (0..STEM_COUNT).fold(true, |ok, s| {
            self.send(s, StemParam::Mute, flag(mutes[s])) && ok
        })
    }

    /// The all-on reset: no solo, every stem unmuted at unity gain with the filter off.
    pub fn all_on(&self) -> bool {
        *self.solo.lock_unpoisoned() = SoloState::default();
        (0..STEM_COUNT).fold(true, |ok, s| {
            let a = self.send(s, StemParam::Mute, 0.0);
            let b = self.send(s, StemParam::Volume, 1.0);
            let c = self.send(s, StemParam::Filter, 0.0);
            ok && a && b && c
        })
    }

    /// How loud a stem sounds, 0..=1, for the activity glow: 0 when muted, else gain over the
    /// volume range.
    pub fn level(&self, stem: usize) -> f32 {
        let Some(s) = self.stems().get(stem).copied() else {
            return 0.0;
        };
        if s.muted || !s.available {
            0.0
        } else {
            (s.gain / MAX_VOLUME as f32).clamp(0.0, 1.0)
        }
    }
}

fn flag(on: bool) -> f64 {
    if on {
        1.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solo_mutes_the_others_and_restores_them_on_the_second_press() {
        let current = [false, true, false, false];
        let (state, mutes) = SoloState::default().press(2, current);
        assert_eq!(state.stem, Some(2));
        assert_eq!(mutes, [true, true, false, true]);
        let (state, mutes) = state.press(2, mutes);
        assert_eq!(state.stem, None);
        assert_eq!(mutes, current, "the mutes from before the solo come back");
    }

    #[test]
    fn moving_the_solo_keeps_the_original_mutes() {
        let current = [true, false, false, false];
        let (state, _) = SoloState::default().press(1, current);
        let (state, mutes) = state.press(3, [false, false, true, true]);
        assert_eq!(state.stem, Some(3));
        assert_eq!(mutes, [true, true, true, false]);
        let (state, mutes) = state.press(3, mutes);
        assert_eq!(state.stem, None);
        assert_eq!(mutes, current);
    }

    #[test]
    fn a_mute_chosen_during_a_solo_survives_the_restore() {
        let (mut state, _) = SoloState::default().press(0, [false; STEM_COUNT]);
        state.note_mute(2, true);
        let (state, mutes) = state.press(0, [false, true, true, true]);
        assert_eq!(state.stem, None);
        assert_eq!(mutes, [false, false, true, false]);
    }

    #[test]
    fn note_mute_outside_a_solo_changes_nothing() {
        let mut state = SoloState::default();
        state.note_mute(1, true);
        assert_eq!(state, SoloState::default());
    }
}
