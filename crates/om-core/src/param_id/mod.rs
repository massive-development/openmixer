//! Typed parameter ids. Ids stay strings on the wire (MIDI maps, UI, `ParameterTarget`); this module
//! is the one place that parses them and prints the canonical form.
//!
//! Grammar (segments split on `.`; every segment non-empty). Index segments are 1 to 3 digits.
//! Leading zeros parse but print canonically (`hotcue01` is `hotcue1`).
//! ```text
//! deck.<d>.play | rate | keylock | seek | loop.exit | loop.reloop | sync | sync.master | slip | quantize
//! deck.<d>.hotcue<k>[.clear]          k < 8
//! deck.<d>.loop.quantized.<beats>     beats: digits with at most one dot, > 0 (e.g. 4, 0.5)
//! mixer.ch<c>.trim | fader | filter | pfl | xfader | eq.low | eq.mid | eq.high     c < 4
//! mixer.crossfader | mixer.crossfader.curve | mixer.master | mixer.cue.level | mixer.cue.mix
//! fx.master.bpm | fx.ch<c>.bpm                        rack is `master` or `ch<c>`, c < 4
//! fx.<rack>.fx<u>.enable | mix | bpm | <effect param>  u <= 999; <effect param> is one segment
//! sync.leader | sync.mode | sync.<d>.sync
//! stem.<d>.<s>.volume | mute | filter                s < 4
//! sampler.pad<p>.trigger | volume | gate | pitch      p < 8
//! automix.enable | start | cancel | style
//! <prefix>.<rest>                                     any other id: Ext, routed by prefix
//! ```
//! `<d>` < [`MAX_DECKS`]. Namespaces `deck`, `mixer`, `fx`, `sync`, `stem`, `sampler`, `automix`
//! are reserved: a malformed id in them is an error, never an `Ext`. Plugin prefixes must avoid them.
//!
//! Threads: pure data and parsing; usable from any thread.

mod error;
mod parse;
#[cfg(test)]
mod tests;

pub use error::ParamError;

use std::fmt;

/// Decks and mixer channels the engine can hold (mirrors the mixer's channel limit).
pub const MAX_DECKS: u8 = 4;
/// Mixer channels, and channel FX racks.
pub const MAX_CHANNELS: u8 = 4;
pub const HOTCUES_PER_DECK: u8 = 8;
pub const STEMS_PER_DECK: u8 = 4;
pub const SAMPLER_PADS: u8 = 8;
/// Largest FX unit index the grammar accepts (three digits).
pub const MAX_FX_UNIT: u16 = 999;

/// A parsed parameter id. `Display` prints the canonical id.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamId {
    Deck {
        deck: u8,
        p: DeckParam,
    },
    Mixer(MixerParam),
    Fx {
        rack: FxRack,
        unit: Option<u16>,
        p: FxParam,
    },
    Sync(SyncParam),
    Stem {
        deck: u8,
        stem: u8,
        p: StemParam,
    },
    Sampler {
        pad: u8,
        p: SamplerParam,
    },
    Automix(AutomixParam),
    /// An id outside the engine namespaces, routed to a registered prefix with `rest` as its id.
    Ext {
        prefix: String,
        rest: String,
    },
}

/// The engine namespaces: the first segment of an id that typed targets own. Ids under these roots
/// never reach string-prefix targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Namespace {
    Deck,
    Mixer,
    Fx,
    Sync,
    Stem,
    Sampler,
    Automix,
}

impl Namespace {
    pub const ALL: [Namespace; 7] = [
        Namespace::Deck,
        Namespace::Mixer,
        Namespace::Fx,
        Namespace::Sync,
        Namespace::Stem,
        Namespace::Sampler,
        Namespace::Automix,
    ];

    /// The root segment of this namespace's ids.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deck => "deck",
            Self::Mixer => "mixer",
            Self::Fx => "fx",
            Self::Sync => "sync",
            Self::Stem => "stem",
            Self::Sampler => "sampler",
            Self::Automix => "automix",
        }
    }

    /// The namespace named by `root`, if it is reserved.
    pub fn from_root(root: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|ns| ns.as_str() == root)
    }
}

/// True when the first segment of `prefix` is an engine namespace. Plugins may not register these.
pub fn is_reserved_prefix(prefix: &str) -> bool {
    let root = prefix.split('.').next().unwrap_or_default();
    Namespace::from_root(root).is_some()
}

/// The part of an FX id after its rack: `bpm` (rack tempo) or `fx<N>.<param>` (unit `N`).
#[derive(Debug, Clone, PartialEq)]
pub struct FxRackParam {
    pub unit: Option<u16>,
    pub p: FxParam,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeckParam {
    Play,
    Rate,
    Keylock,
    Seek,
    LoopExit,
    LoopReloop,
    /// Loop length in beats, from the nearest grid step.
    LoopQuantized(f64),
    Hotcue {
        index: u8,
        clear: bool,
    },
    Sync,
    SyncMaster,
    Slip,
    Quantize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerParam {
    Channel { ch: u8, p: ChannelParam },
    Crossfader,
    CrossfaderCurve,
    Master,
    CueLevel,
    CueMix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelParam {
    Trim,
    Fader,
    Filter,
    Pfl,
    Xfader,
    Eq(EqBand),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqBand {
    Low,
    Mid,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxRack {
    Master,
    Channel(u8),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FxParam {
    Enable,
    Mix,
    Bpm,
    /// An effect-specific parameter, named as the effect declares it.
    Effect(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncParam {
    Leader,
    Mode,
    /// `sync.<d>.sync`: the deck follows the leader.
    DeckEnabled(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StemParam {
    Volume,
    Mute,
    Filter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplerParam {
    Trigger,
    Volume,
    Gate,
    Pitch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomixParam {
    Enable,
    Start,
    Cancel,
    Style,
}

impl ParamId {
    /// The engine namespace this id belongs to, or `None` for plugin (`Ext`) ids.
    pub fn namespace(&self) -> Option<Namespace> {
        match self {
            Self::Deck { .. } => Some(Namespace::Deck),
            Self::Mixer(_) => Some(Namespace::Mixer),
            Self::Fx { .. } => Some(Namespace::Fx),
            Self::Sync(_) => Some(Namespace::Sync),
            Self::Stem { .. } => Some(Namespace::Stem),
            Self::Sampler { .. } => Some(Namespace::Sampler),
            Self::Automix(_) => Some(Namespace::Automix),
            Self::Ext { .. } => None,
        }
    }
}

impl fmt::Display for ParamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Deck { deck, p } => write!(f, "deck.{deck}.{p}"),
            Self::Mixer(p) => write!(f, "mixer.{p}"),
            Self::Fx { rack, unit, p } => match unit {
                Some(unit) => write!(f, "fx.{rack}.fx{unit}.{p}"),
                None => write!(f, "fx.{rack}.{p}"),
            },
            Self::Sync(p) => write!(f, "sync.{p}"),
            Self::Stem { deck, stem, p } => write!(f, "stem.{deck}.{stem}.{p}"),
            Self::Sampler { pad, p } => write!(f, "sampler.pad{pad}.{p}"),
            Self::Automix(p) => write!(f, "automix.{p}"),
            Self::Ext { prefix, rest } => write!(f, "{prefix}.{rest}"),
        }
    }
}

impl fmt::Display for DeckParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Play => f.write_str("play"),
            Self::Rate => f.write_str("rate"),
            Self::Keylock => f.write_str("keylock"),
            Self::Seek => f.write_str("seek"),
            Self::LoopExit => f.write_str("loop.exit"),
            Self::LoopReloop => f.write_str("loop.reloop"),
            Self::LoopQuantized(beats) => write!(f, "loop.quantized.{beats}"),
            Self::Hotcue {
                index,
                clear: false,
            } => write!(f, "hotcue{index}"),
            Self::Hotcue { index, clear: true } => write!(f, "hotcue{index}.clear"),
            Self::Sync => f.write_str("sync"),
            Self::SyncMaster => f.write_str("sync.master"),
            Self::Slip => f.write_str("slip"),
            Self::Quantize => f.write_str("quantize"),
        }
    }
}

impl fmt::Display for MixerParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Channel { ch, p } => write!(f, "ch{ch}.{p}"),
            Self::Crossfader => f.write_str("crossfader"),
            Self::CrossfaderCurve => f.write_str("crossfader.curve"),
            Self::Master => f.write_str("master"),
            Self::CueLevel => f.write_str("cue.level"),
            Self::CueMix => f.write_str("cue.mix"),
        }
    }
}

impl fmt::Display for ChannelParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Trim => f.write_str("trim"),
            Self::Fader => f.write_str("fader"),
            Self::Filter => f.write_str("filter"),
            Self::Pfl => f.write_str("pfl"),
            Self::Xfader => f.write_str("xfader"),
            Self::Eq(band) => write!(f, "eq.{band}"),
        }
    }
}

impl fmt::Display for EqBand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Low => "low",
            Self::Mid => "mid",
            Self::High => "high",
        })
    }
}

impl fmt::Display for FxRack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Master => f.write_str("master"),
            Self::Channel(ch) => write!(f, "ch{ch}"),
        }
    }
}

impl fmt::Display for FxParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Enable => f.write_str("enable"),
            Self::Mix => f.write_str("mix"),
            Self::Bpm => f.write_str("bpm"),
            Self::Effect(name) => f.write_str(name),
        }
    }
}

impl fmt::Display for SyncParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Leader => f.write_str("leader"),
            Self::Mode => f.write_str("mode"),
            Self::DeckEnabled(deck) => write!(f, "{deck}.sync"),
        }
    }
}

impl fmt::Display for StemParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Volume => "volume",
            Self::Mute => "mute",
            Self::Filter => "filter",
        })
    }
}

impl fmt::Display for SamplerParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Trigger => "trigger",
            Self::Volume => "volume",
            Self::Gate => "gate",
            Self::Pitch => "pitch",
        })
    }
}

impl fmt::Display for AutomixParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Enable => "enable",
            Self::Start => "start",
            Self::Cancel => "cancel",
            Self::Style => "style",
        })
    }
}
