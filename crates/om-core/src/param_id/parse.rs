use super::{
    AutomixParam, ChannelParam, DeckParam, EqBand, FxParam, FxRack, MixerParam, ParamError,
    ParamId, SamplerParam, StemParam, SyncParam, HOTCUES_PER_DECK, MAX_CHANNELS, MAX_DECKS,
    MAX_FX_UNIT, SAMPLER_PADS, STEMS_PER_DECK,
};
use std::str::FromStr;

impl FromStr for ParamId {
    type Err = ParamError;

    fn from_str(id: &str) -> Result<Self, ParamError> {
        if id.is_empty() {
            return Err(ParamError::Empty);
        }
        let parts: Vec<&str> = id.split('.').collect();
        if parts.iter().any(|part| part.is_empty()) {
            return Err(malformed(id));
        }
        let rest = &parts[1..];
        match parts[0] {
            "deck" => parse_deck(id, rest),
            "mixer" => parse_mixer(id, rest),
            "fx" => parse_fx(id, rest),
            "sync" => parse_sync(id, rest),
            "stem" => parse_stem(id, rest),
            "sampler" => parse_sampler(id, rest),
            "automix" => parse_automix(id, rest),
            _ => parse_ext(id, &parts),
        }
    }
}

fn malformed(id: &str) -> ParamError {
    ParamError::Malformed(id.to_owned())
}

fn unknown(id: &str) -> ParamError {
    ParamError::Unknown(id.to_owned())
}

/// A 1 to 3 digit index below `max`. Non-digits are an unknown id; oversized values are out of range.
fn index(id: &str, text: &str, what: &'static str, max: u16) -> Result<u16, ParamError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(unknown(id));
    }
    if text.len() > 3 {
        return Err(ParamError::OutOfRange {
            id: id.to_owned(),
            what,
            max: max - 1,
        });
    }
    let value: u16 = text.parse().map_err(|_| unknown(id))?;
    if value >= max {
        return Err(ParamError::OutOfRange {
            id: id.to_owned(),
            what,
            max: max - 1,
        });
    }
    Ok(value)
}

fn index_u8(id: &str, text: &str, what: &'static str, max: u8) -> Result<u8, ParamError> {
    let value = index(id, text, what, u16::from(max))?;
    u8::try_from(value).map_err(|_| unknown(id))
}

fn parse_deck(id: &str, rest: &[&str]) -> Result<ParamId, ParamError> {
    let Some((deck_text, tail)) = rest.split_first() else {
        return Err(malformed(id));
    };
    let deck = index_u8(id, deck_text, "deck", MAX_DECKS)?;
    Ok(ParamId::Deck {
        deck,
        p: parse_deck_param(id, tail)?,
    })
}

fn parse_deck_param(id: &str, tail: &[&str]) -> Result<DeckParam, ParamError> {
    let p = match tail {
        ["play"] => DeckParam::Play,
        ["rate"] => DeckParam::Rate,
        ["keylock"] => DeckParam::Keylock,
        ["seek"] => DeckParam::Seek,
        ["loop", "exit"] => DeckParam::LoopExit,
        ["loop", "reloop"] => DeckParam::LoopReloop,
        ["loop", "quantized", beats @ ..] if !beats.is_empty() => {
            DeckParam::LoopQuantized(parse_beats(id, &beats.join("."))?)
        }
        ["sync"] => DeckParam::Sync,
        ["sync", "master"] => DeckParam::SyncMaster,
        ["slip"] => DeckParam::Slip,
        ["quantize"] => DeckParam::Quantize,
        [hot, action @ ..] if hot.starts_with("hotcue") => parse_hotcue(id, hot, action)?,
        _ => return Err(unknown(id)),
    };
    Ok(p)
}

fn parse_hotcue(id: &str, hot: &str, action: &[&str]) -> Result<DeckParam, ParamError> {
    let index = index_u8(id, &hot["hotcue".len()..], "hotcue", HOTCUES_PER_DECK)?;
    let clear = match action {
        [] => false,
        ["clear"] => true,
        _ => return Err(unknown(id)),
    };
    Ok(DeckParam::Hotcue { index, clear })
}

/// Digits, one dot at most, no leading or trailing dot, and a finite positive value.
fn parse_beats(id: &str, text: &str) -> Result<f64, ParamError> {
    let well_formed = text.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && text.matches('.').count() <= 1
        && !text.starts_with('.')
        && !text.ends_with('.');
    if !well_formed {
        return Err(malformed(id));
    }
    match text.parse::<f64>() {
        Ok(beats) if beats.is_finite() && beats > 0.0 => Ok(beats),
        _ => Err(malformed(id)),
    }
}

fn parse_mixer(id: &str, rest: &[&str]) -> Result<ParamId, ParamError> {
    let p = match rest {
        ["crossfader"] => MixerParam::Crossfader,
        ["crossfader", "curve"] => MixerParam::CrossfaderCurve,
        ["master"] => MixerParam::Master,
        ["cue", "level"] => MixerParam::CueLevel,
        ["cue", "mix"] => MixerParam::CueMix,
        [head, tail @ ..] if head.starts_with("ch") => MixerParam::Channel {
            ch: index_u8(id, &head[2..], "channel", MAX_CHANNELS)?,
            p: parse_channel_param(id, tail)?,
        },
        _ => return Err(unknown(id)),
    };
    Ok(ParamId::Mixer(p))
}

fn parse_channel_param(id: &str, tail: &[&str]) -> Result<ChannelParam, ParamError> {
    Ok(match tail {
        ["trim"] => ChannelParam::Trim,
        ["fader"] => ChannelParam::Fader,
        ["filter"] => ChannelParam::Filter,
        ["pfl"] => ChannelParam::Pfl,
        ["xfader"] => ChannelParam::Xfader,
        ["eq", "low"] => ChannelParam::Eq(EqBand::Low),
        ["eq", "mid"] => ChannelParam::Eq(EqBand::Mid),
        ["eq", "high"] => ChannelParam::Eq(EqBand::High),
        _ => return Err(unknown(id)),
    })
}

fn parse_fx(id: &str, rest: &[&str]) -> Result<ParamId, ParamError> {
    let Some((rack_text, tail)) = rest.split_first() else {
        return Err(malformed(id));
    };
    let rack = parse_rack(id, rack_text)?;
    match tail {
        ["bpm"] => Ok(ParamId::Fx {
            rack,
            unit: None,
            p: FxParam::Bpm,
        }),
        [unit_text, p @ ..] => Ok(ParamId::Fx {
            rack,
            unit: Some(parse_unit(id, unit_text)?),
            p: parse_fx_param(id, p)?,
        }),
        [] => Err(malformed(id)),
    }
}

fn parse_rack(id: &str, text: &str) -> Result<FxRack, ParamError> {
    if text == "master" {
        return Ok(FxRack::Master);
    }
    match text.strip_prefix("ch") {
        Some(digits) => Ok(FxRack::Channel(index_u8(
            id,
            digits,
            "channel",
            MAX_CHANNELS,
        )?)),
        None => Err(unknown(id)),
    }
}

fn parse_unit(id: &str, text: &str) -> Result<u16, ParamError> {
    let digits = text.strip_prefix("fx").ok_or_else(|| unknown(id))?;
    index(id, digits, "FX unit", MAX_FX_UNIT + 1)
}

fn parse_fx_param(id: &str, p: &[&str]) -> Result<FxParam, ParamError> {
    Ok(match p {
        ["enable"] => FxParam::Enable,
        ["mix"] => FxParam::Mix,
        ["bpm"] => FxParam::Bpm,
        [name] => FxParam::Effect((*name).to_owned()),
        _ => return Err(unknown(id)),
    })
}

fn parse_sync(id: &str, rest: &[&str]) -> Result<ParamId, ParamError> {
    let p = match rest {
        ["leader"] => SyncParam::Leader,
        ["mode"] => SyncParam::Mode,
        [deck, "sync"] => SyncParam::DeckEnabled(index_u8(id, deck, "deck", MAX_DECKS)?),
        _ => return Err(unknown(id)),
    };
    Ok(ParamId::Sync(p))
}

fn parse_stem(id: &str, rest: &[&str]) -> Result<ParamId, ParamError> {
    match rest {
        [deck, stem, p] => Ok(ParamId::Stem {
            deck: index_u8(id, deck, "deck", MAX_DECKS)?,
            stem: index_u8(id, stem, "stem", STEMS_PER_DECK)?,
            p: match *p {
                "volume" => StemParam::Volume,
                "mute" => StemParam::Mute,
                "filter" => StemParam::Filter,
                _ => return Err(unknown(id)),
            },
        }),
        _ => Err(unknown(id)),
    }
}

fn parse_sampler(id: &str, rest: &[&str]) -> Result<ParamId, ParamError> {
    match rest {
        [pad, p] if pad.starts_with("pad") => Ok(ParamId::Sampler {
            pad: index_u8(id, &pad[3..], "pad", SAMPLER_PADS)?,
            p: match *p {
                "trigger" => SamplerParam::Trigger,
                "volume" => SamplerParam::Volume,
                "gate" => SamplerParam::Gate,
                "pitch" => SamplerParam::Pitch,
                _ => return Err(unknown(id)),
            },
        }),
        _ => Err(unknown(id)),
    }
}

fn parse_automix(id: &str, rest: &[&str]) -> Result<ParamId, ParamError> {
    let p = match rest {
        ["enable"] => AutomixParam::Enable,
        ["start"] => AutomixParam::Start,
        ["cancel"] => AutomixParam::Cancel,
        ["style"] => AutomixParam::Style,
        _ => return Err(unknown(id)),
    };
    Ok(ParamId::Automix(p))
}

/// Any other `prefix.rest` with identifier-shaped segments. Single segments are not extensions.
fn parse_ext(id: &str, parts: &[&str]) -> Result<ParamId, ParamError> {
    let shaped_segments = parts.iter().all(|part| {
        part.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    });
    let prefix = parts[0];
    let starts_well = prefix
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_');
    if parts.len() < 2 || !shaped_segments || !starts_well {
        return Err(malformed(id));
    }
    Ok(ParamId::Ext {
        prefix: prefix.to_owned(),
        rest: parts[1..].join("."),
    })
}
