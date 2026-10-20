//! Read-only ids: the state a controller map's feedback rules can read. They are not control
//! parameters, so they stay out of [`crate::ParamId`]: `deck.<d>.loaded`, `deck.<d>.position` and
//! `mixer.ch<c>.level` have no setter. Read by [`crate::EngineSnapshot::read`].
//!
//! Grammar (segments split on `.`):
//! ```text
//! deck.<d>.play | loaded | keylock | loop | cue | sync | sync.master | slip | quantize | rate | position
//! deck.<d>.hotcue<k>.set          k: any index; past the bank (8) reads as unset
//! mixer.ch<c>.pfl | fader | trim | filter | level
//! mixer.crossfader | mixer.master | mixer.master.level
//! stem.<d>.<s>.volume | mute | filter     s: 0..3 (drums, bass, vocals, other)
//! ```
//! `<d>` and `<c>` are digits only; whether they name a real deck or channel is decided by the
//! snapshot, which knows the engine's size.

use crate::param_id::{ParamError, StemParam};
use std::str::FromStr;

/// A parsed read-only id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateId {
    Deck { deck: usize, p: DeckState },
    Mixer(MixerState),
    /// The control values of `stem.<d>.<s>.*`, read back.
    Stem {
        deck: usize,
        stem: usize,
        p: StemParam,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeckState {
    Playing,
    Loaded,
    Keylock,
    Looping,
    /// Loaded and stopped.
    Cue,
    Sync,
    SyncMaster,
    Slip,
    Quantize,
    Rate,
    /// Playhead as a fraction of the track.
    Position,
    Hotcue(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MixerState {
    Channel { ch: usize, p: ChannelState },
    Crossfader,
    Master,
    MasterLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelState {
    Pfl,
    Fader,
    Trim,
    Filter,
    /// Decaying peak, louder side.
    Level,
}

impl FromStr for StateId {
    type Err = ParamError;

    fn from_str(id: &str) -> Result<Self, ParamError> {
        if id.is_empty() {
            return Err(ParamError::Empty);
        }
        let parts: Vec<&str> = id.split('.').collect();
        let parsed = match parts.as_slice() {
            ["deck", deck, rest @ ..] => digits(deck)
                .zip(deck_state(rest))
                .map(|(deck, p)| Self::Deck { deck, p }),
            ["mixer", rest @ ..] => mixer_state(rest).map(Self::Mixer),
            ["stem", deck, stem, p] => digits(deck).zip(digits(stem)).and_then(|(deck, stem)| {
                let p = match *p {
                    "volume" => StemParam::Volume,
                    "mute" => StemParam::Mute,
                    "filter" => StemParam::Filter,
                    _ => return None,
                };
                Some(Self::Stem { deck, stem, p })
            }),
            _ => None,
        };
        parsed.ok_or_else(|| ParamError::Unknown(id.to_owned()))
    }
}

/// A non-empty run of ASCII digits.
fn digits(text: &str) -> Option<usize> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn deck_state(rest: &[&str]) -> Option<DeckState> {
    Some(match rest {
        ["play"] => DeckState::Playing,
        ["loaded"] => DeckState::Loaded,
        ["keylock"] => DeckState::Keylock,
        ["loop"] => DeckState::Looping,
        ["cue"] => DeckState::Cue,
        ["sync"] => DeckState::Sync,
        ["sync", "master"] => DeckState::SyncMaster,
        ["slip"] => DeckState::Slip,
        ["quantize"] => DeckState::Quantize,
        ["rate"] => DeckState::Rate,
        ["position"] => DeckState::Position,
        [cue, "set"] => DeckState::Hotcue(digits(cue.strip_prefix("hotcue")?)?),
        _ => return None,
    })
}

fn mixer_state(rest: &[&str]) -> Option<MixerState> {
    Some(match rest {
        ["crossfader"] => MixerState::Crossfader,
        ["master"] => MixerState::Master,
        ["master", "level"] => MixerState::MasterLevel,
        [channel, p] => MixerState::Channel {
            ch: digits(channel.strip_prefix("ch")?)?,
            p: match *p {
                "pfl" => ChannelState::Pfl,
                "fader" => ChannelState::Fader,
                "trim" => ChannelState::Trim,
                "filter" => ChannelState::Filter,
                "level" => ChannelState::Level,
                _ => return None,
            },
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_read_id() {
        assert_eq!(
            "deck.1.play".parse::<StateId>(),
            Ok(StateId::Deck {
                deck: 1,
                p: DeckState::Playing
            })
        );
        assert_eq!(
            "deck.0.sync.master".parse::<StateId>(),
            Ok(StateId::Deck {
                deck: 0,
                p: DeckState::SyncMaster
            })
        );
        assert_eq!(
            "deck.0.hotcue12.set".parse::<StateId>(),
            Ok(StateId::Deck {
                deck: 0,
                p: DeckState::Hotcue(12)
            })
        );
        assert_eq!(
            "mixer.ch3.level".parse::<StateId>(),
            Ok(StateId::Mixer(MixerState::Channel {
                ch: 3,
                p: ChannelState::Level
            }))
        );
        assert_eq!(
            "mixer.master.level".parse::<StateId>(),
            Ok(StateId::Mixer(MixerState::MasterLevel))
        );
    }

    #[test]
    fn parses_stem_read_ids() {
        assert_eq!(
            "stem.1.3.mute".parse::<StateId>(),
            Ok(StateId::Stem {
                deck: 1,
                stem: 3,
                p: StemParam::Mute
            })
        );
        assert!("stem.1.3".parse::<StateId>().is_err());
        assert!("stem.1.x.mute".parse::<StateId>().is_err());
        assert!("stem.1.2.bogus".parse::<StateId>().is_err());
    }

    #[test]
    fn rejects_malformed_and_unknown_ids() {
        for id in [
            "",
            "deck",
            "deck.0",
            "deck.x.play",
            "deck..play",
            "deck.0.bogus",
            "deck.0.play.x",
            "deck.0.hotcue.set",
            "deck.0.hotcuex.set",
            "mixer",
            "mixer.ch0",
            "mixer.ch.pfl",
            "mixer.ch0.bogus",
            "mixer.master.bogus",
            "nope.0.play",
        ] {
            assert!(id.parse::<StateId>().is_err(), "{id}");
        }
    }
}
