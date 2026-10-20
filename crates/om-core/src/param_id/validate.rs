//! Validation of hand-built ids. The parser only builds valid ids; a value built by code can name
//! an index or a shape the grammar cannot print back. [`ParamId::validate`] is the check.

use super::{
    DeckParam, FxParam, FxRack, MixerParam, ParamError, ParamId, SyncParam, HOTCUES_PER_DECK,
    MAX_CHANNELS, MAX_DECKS, MAX_FX_UNIT, MAX_LOOP_BEATS, SAMPLER_PADS, STEMS_PER_DECK,
};

impl ParamId {
    /// `Ok` when `Display` prints this id in a form that parses back to the same value. Ids from
    /// `FromStr` always pass; hand-built ids may not. Limits are the grammar's (`MAX_DECKS`, and so
    /// on).
    pub fn validate(&self) -> Result<(), ParamError> {
        match self {
            Self::Deck { deck, p } => {
                index(self, u16::from(*deck), "deck", MAX_DECKS)?;
                validate_deck_param(self, p)
            }
            Self::Mixer(p) => validate_mixer_param(self, p),
            Self::Fx { rack, unit, p } => validate_fx(self, *rack, *unit, p),
            Self::Sync(SyncParam::DeckEnabled(deck)) => {
                index(self, u16::from(*deck), "deck", MAX_DECKS)
            }
            Self::Sync(_) | Self::Automix(_) | Self::Record(_) => Ok(()),
            Self::Stem { deck, stem, .. } => {
                index(self, u16::from(*deck), "deck", MAX_DECKS)?;
                index(self, u16::from(*stem), "stem", STEMS_PER_DECK)
            }
            Self::Sampler { pad, .. } => index(self, u16::from(*pad), "pad", SAMPLER_PADS),
            Self::Ext { prefix, rest } => validate_ext(self, prefix, rest),
        }
    }
}

/// Fails when `value` is not below `max`.
fn index(id: &ParamId, value: u16, what: &'static str, max: u8) -> Result<(), ParamError> {
    if value < u16::from(max) {
        Ok(())
    } else {
        Err(out_of_range(id, what, u16::from(max) - 1))
    }
}

fn out_of_range(id: &ParamId, what: &'static str, max: u16) -> ParamError {
    ParamError::OutOfRange {
        id: id.to_string(),
        what,
        max,
    }
}

fn malformed(id: &ParamId) -> ParamError {
    ParamError::Malformed(id.to_string())
}

fn validate_deck_param(id: &ParamId, p: &DeckParam) -> Result<(), ParamError> {
    match p {
        DeckParam::Hotcue { index: cue, .. } => {
            index(id, u16::from(*cue), "hotcue", HOTCUES_PER_DECK)
        }
        // The same rule the parser applies to `loop.quantized.<beats>`.
        DeckParam::LoopQuantized(beats) => {
            if beats.is_finite() && *beats > 0.0 && *beats <= MAX_LOOP_BEATS {
                Ok(())
            } else {
                Err(malformed(id))
            }
        }
        _ => Ok(()),
    }
}

fn validate_mixer_param(id: &ParamId, p: &MixerParam) -> Result<(), ParamError> {
    match p {
        MixerParam::Channel { ch, .. } => index(id, u16::from(*ch), "channel", MAX_CHANNELS),
        _ => Ok(()),
    }
}

fn validate_fx(
    id: &ParamId,
    rack: FxRack,
    unit: Option<u16>,
    p: &FxParam,
) -> Result<(), ParamError> {
    if let FxRack::Channel(ch) = rack {
        index(id, u16::from(ch), "channel", MAX_CHANNELS)?;
    }
    match unit {
        // With no unit only the rack tempo is an id; `fx.<rack>.enable` would parse as nothing.
        None if *p != FxParam::Bpm => Err(malformed(id)),
        None => Ok(()),
        Some(unit) if unit > MAX_FX_UNIT => Err(out_of_range(id, "FX unit", MAX_FX_UNIT)),
        Some(_) => match p {
            FxParam::Effect(name) => validate_segment(id, name, &["enable", "mix", "bpm"]),
            _ => Ok(()),
        },
    }
}

/// An effect name is one non-empty segment without dots, and not a word the grammar reads as
/// something else.
fn validate_segment(id: &ParamId, name: &str, reserved: &[&str]) -> Result<(), ParamError> {
    if name.is_empty() || name.contains('.') || reserved.contains(&name) {
        Err(malformed(id))
    } else {
        Ok(())
    }
}

/// A plugin id: a prefix the engine does not own, and identifier-shaped segments after it.
fn validate_ext(id: &ParamId, prefix: &str, rest: &str) -> Result<(), ParamError> {
    let shaped = |text: &str| {
        !text.is_empty()
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    };
    let starts_well = prefix
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_');
    if super::is_reserved_prefix(prefix) {
        return Err(ParamError::Reserved(prefix.to_owned()));
    }
    if !starts_well || !shaped(prefix) || rest.split('.').any(|seg| !shaped(seg)) {
        return Err(malformed(id));
    }
    Ok(())
}
