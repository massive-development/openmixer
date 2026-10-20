//! Engine state as plain values, read by id. The engine builds a snapshot on the control thread
//! (`DjEngine::snapshot`) from its atomics and control-side getters; readers (the UI, MIDI feedback)
//! then work on the copy, so they never hold an engine lock.
//!
//! Approximate by design: fields are read one at a time, not as one frame, so a snapshot can mix
//! values from two audio blocks (a playhead and a loop bound, say). Peaks are decaying meter values.
//! Nothing here is an audio-thread value; the snapshot is for display and feedback only.
//!
//! Automix state is not engine state: the engine leaves [`AutomixSnapshot`] at its default and the
//! app overlays the controller's own snapshot (`om-automix`) before readers see it.

use crate::param_id::{StemParam, HOTCUES_PER_DECK, SAMPLER_PADS, STEMS_PER_DECK};
use crate::state_id::{ChannelState, DeckState, MixerState, PadState, StateId};

/// One consistent-enough copy of the engine. See the module docs.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineSnapshot {
    /// Control-timer ticks (`DjEngine::update_control` calls) so far when the snapshot was taken.
    /// It advances on the control timer, not on each engine change, so it measures elapsed control
    /// time. It does not say whether any value moved; compare values for that.
    pub ticks: u64,
    /// Snapshots the engine has built, this one included: 1 for the first. Strictly increasing per
    /// engine, so two snapshots from one engine with different `built` values are separate captures.
    pub built: u64,
    pub decks: Vec<DeckSnapshot>,
    pub mixer: MixerSnapshot,
    pub stems: Vec<[StemSnapshot; STEMS_PER_DECK as usize]>,
    pub sampler: SamplerSnapshot,
    pub automix: AutomixSnapshot,
}

impl EngineSnapshot {
    /// The value of one state id, or `None` when the id names nothing in this engine (an index out
    /// of range, or a channel the mixer does not have). Feeds [`crate::StateReader`].
    pub fn read(&self, id: &StateId) -> Option<f64> {
        match id {
            StateId::Deck { deck, p } => self.decks.get(*deck).map(|d| d.read(p)),
            StateId::Mixer(p) => self.mixer.read(p),
            StateId::Stem { deck, stem, p } => self
                .stems
                .get(*deck)
                .and_then(|d| d.get(*stem))
                .map(|s| s.read(p)),
            StateId::Sampler { pad, p } => self.sampler.pads.get(*pad).map(|s| s.read(p)),
        }
    }
}

/// One deck. Plain copies: no allocation, so the UI can take one per frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DeckSnapshot {
    pub loaded: bool,
    pub playing: bool,
    pub keylock: bool,
    /// A loop is set and enabled.
    pub looping: bool,
    pub slip: bool,
    pub quantize: bool,
    /// The deck follows the sync leader.
    pub sync: bool,
    pub sync_master: bool,
    pub rate: f64,
    pub position_frames: i64,
    pub length_frames: i64,
    pub file_sample_rate: f64,
    /// Loop bounds in source frames; both zero when no loop is set.
    pub loop_start: i64,
    pub loop_end: i64,
    pub hotcues: [Option<i64>; HOTCUES_PER_DECK as usize],
}

impl DeckSnapshot {
    /// The loop bounds are set (end after start), as `Loop::valid` in om-engine.
    pub fn loop_valid(&self) -> bool {
        self.loop_end > self.loop_start
    }

    /// Value of one deck state. Flags read as 1.0 or 0.0; `position` is 0 to 1 (0 when the track
    /// length is unknown). A hotcue index past the bank reads as unset.
    pub fn read(&self, p: &DeckState) -> f64 {
        match p {
            DeckState::Playing => flag(self.playing),
            DeckState::Loaded => flag(self.loaded),
            DeckState::Keylock => flag(self.keylock),
            DeckState::Looping => flag(self.looping),
            DeckState::Cue => flag(self.loaded && !self.playing),
            DeckState::Sync => flag(self.sync),
            DeckState::SyncMaster => flag(self.sync_master),
            DeckState::Slip => flag(self.slip),
            DeckState::Quantize => flag(self.quantize),
            DeckState::Rate => self.rate,
            DeckState::Position => {
                if self.length_frames > 0 {
                    self.position_frames as f64 / self.length_frames as f64
                } else {
                    0.0
                }
            }
            DeckState::Hotcue(index) => flag(self.hotcues.get(*index).copied().flatten().is_some()),
        }
    }
}

/// The mixer: channel strips in order, then the crossfader, master and cue.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MixerSnapshot {
    pub channels: Vec<ChannelSnapshot>,
    pub crossfader: f32,
    pub master_gain: f32,
    pub cue_level: f32,
    pub cue_mix: f32,
    pub master_peak_left: f32,
    pub master_peak_right: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ChannelSnapshot {
    pub trim: f32,
    pub fader: f32,
    pub filter: f32,
    /// Low, mid, high, in engine order.
    pub eq: [f32; 3],
    pub pfl: bool,
    pub peak_left: f32,
    pub peak_right: f32,
}

impl MixerSnapshot {
    /// Value of one mixer state; `None` for a channel the mixer does not have.
    pub fn read(&self, p: &MixerState) -> Option<f64> {
        match p {
            MixerState::Channel { ch, p } => {
                let channel = self.channels.get(*ch)?;
                Some(match p {
                    ChannelState::Pfl => flag(channel.pfl),
                    ChannelState::Fader => f64::from(channel.fader),
                    ChannelState::Trim => f64::from(channel.trim),
                    ChannelState::Filter => f64::from(channel.filter),
                    ChannelState::Level => f64::from(channel.peak_left.max(channel.peak_right)),
                })
            }
            MixerState::Crossfader => Some(f64::from(self.crossfader)),
            MixerState::Master => Some(f64::from(self.master_gain)),
            MixerState::MasterLevel => {
                Some(f64::from(self.master_peak_left.max(self.master_peak_right)))
            }
        }
    }
}

/// State of one stem of a deck.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StemSnapshot {
    /// The deck plays from stems (`Deck::load_stems`); otherwise the other fields are idle values.
    pub available: bool,
    pub muted: bool,
    /// Linear gain, 0 to 1.5.
    pub gain: f32,
    /// Sweep filter, -1 low-pass to +1 high-pass, 0 off.
    pub filter: f32,
}

impl Default for StemSnapshot {
    fn default() -> Self {
        Self {
            available: false,
            muted: false,
            gain: 1.0,
            filter: 0.0,
        }
    }
}

impl StemSnapshot {
    /// Value of one stem state, in the units its control id takes: gain, mute as 1.0 or 0.0, filter.
    pub fn read(&self, p: &StemParam) -> f64 {
        match p {
            StemParam::Volume => f64::from(self.gain),
            StemParam::Mute => flag(self.muted),
            StemParam::Filter => f64::from(self.filter),
        }
    }
}

/// How a sampler pad treats its sample once triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PadMode {
    /// Plays to the end; release is ignored.
    #[default]
    OneShot,
    /// Plays while held; release fades it out.
    Gate,
    /// Latches: a press starts the loop and the next press stops it. Release is ignored; stop-all
    /// and clear also end it.
    Loop,
}

impl PadMode {
    /// Stable numeric form (0, 1, 2) used by atomics and `sampler.pad<N>.mode` reads.
    pub fn as_u8(self) -> u8 {
        match self {
            Self::OneShot => 0,
            Self::Gate => 1,
            Self::Loop => 2,
        }
    }

    /// Inverse of [`PadMode::as_u8`]; unknown values fall back to one-shot.
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Gate,
            2 => Self::Loop,
            _ => Self::OneShot,
        }
    }
}

/// One sampler pad.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadSnapshot {
    /// A sample is assigned.
    pub loaded: bool,
    /// At least one voice of the pad is sounding.
    pub playing: bool,
    /// Linear gain, 0 to 1.5.
    pub gain: f32,
    /// Varispeed in semitones, -24 to 24.
    pub pitch: f32,
    pub mode: PadMode,
    /// Hash of the assigned sample's name (0 when empty); lets a UI notice a changed sample.
    pub name_hash: u64,
}

impl Default for PadSnapshot {
    fn default() -> Self {
        Self {
            loaded: false,
            playing: false,
            gain: 1.0,
            pitch: 0.0,
            mode: PadMode::OneShot,
            name_hash: 0,
        }
    }
}

impl PadSnapshot {
    /// Value of one pad state: flags as 1.0 or 0.0, gain, semitones, mode as 0/1/2.
    pub fn read(&self, p: &PadState) -> f64 {
        match p {
            PadState::Loaded => flag(self.loaded),
            PadState::Playing => flag(self.playing),
            PadState::Volume => f64::from(self.gain),
            PadState::Pitch => f64::from(self.pitch),
            PadState::Mode => f64::from(self.mode.as_u8()),
        }
    }
}

/// The sampler: eight pads plus counters. Filled from atomics, no audio lock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SamplerSnapshot {
    pub pads: [PadSnapshot; SAMPLER_PADS as usize],
    /// Sounding voices over all pads.
    pub voices_active: u32,
    /// Commands the sampler dropped because its ring was full.
    pub dropped_cmds: u64,
}

impl Default for SamplerSnapshot {
    fn default() -> Self {
        Self {
            pads: [PadSnapshot::default(); SAMPLER_PADS as usize],
            voices_active: 0,
            dropped_cmds: 0,
        }
    }
}

/// Where the automix controller is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AutomixState {
    /// Nothing planned.
    #[default]
    Idle,
    /// The next track is being loaded on the idle deck.
    Loading,
    /// The next track is loaded and cued; a plan waits for its start beat.
    Armed,
    /// The plan is running.
    Transitioning,
    /// The plan ended; end-of-transition moves are being applied.
    Cleanup,
}

/// How a transition mixes the outgoing deck into the incoming one. `Auto` lets the planner pick.
/// The numeric form is the value of the `automix.style` id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AutomixStyle {
    #[default]
    Auto,
    Crossfade,
    BassSwap,
    EchoOut,
    Cut,
    /// Drums-only bridge on the outgoing deck (stems).
    StemBridge,
}

impl AutomixStyle {
    pub const ALL: [AutomixStyle; 6] = [
        Self::Auto,
        Self::Crossfade,
        Self::BassSwap,
        Self::EchoOut,
        Self::Cut,
        Self::StemBridge,
    ];

    /// Stable numeric form (the `automix.style` value).
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Auto => 0,
            Self::Crossfade => 1,
            Self::BassSwap => 2,
            Self::EchoOut => 3,
            Self::Cut => 4,
            Self::StemBridge => 5,
        }
    }

    /// Inverse of [`AutomixStyle::as_u8`]; unknown values fall back to `Auto`.
    pub fn from_u8(v: u8) -> Self {
        Self::ALL
            .into_iter()
            .find(|s| s.as_u8() == v)
            .unwrap_or_default()
    }

    /// The style for an `automix.style` value: rounded, out-of-range reads as `Auto`.
    pub fn from_value(value: f64) -> Self {
        if value.is_finite() && (0.0..=255.0).contains(&value.round()) {
            Self::from_u8(value.round() as u8)
        } else {
            Self::Auto
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Crossfade => "Crossfade",
            Self::BassSwap => "Bass swap",
            Self::EchoOut => "Echo out",
            Self::Cut => "Cut",
            Self::StemBridge => "Stem bridge",
        }
    }
}

/// The automix controller as readers see it (filled by `om-automix`, not by the engine).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AutomixSnapshot {
    /// `automix.enable`.
    pub enabled: bool,
    pub state: AutomixState,
    /// The style asked for (`Auto` stays `Auto`).
    pub style: AutomixStyle,
    /// The style of the plan in flight or armed, when there is one.
    pub plan_style: Option<AutomixStyle>,
    /// 0 to 1 through the running transition; 0 when none runs.
    pub progress: f64,
    /// Title of the track loaded or loading as the next one; empty when none.
    pub next_track_title: String,
    /// Tempo of that track, when it is analysed.
    pub next_track_bpm: Option<f64>,
    /// Camelot key of that track; empty when unknown.
    pub next_track_key: String,
    /// Beats of the outgoing deck left until its end; `None` when no deck leads.
    pub remaining_beats: Option<f64>,
    /// The last thing that went wrong or why a plan was dropped; empty when nothing did.
    pub last_error: String,
}

fn flag(on: bool) -> f64 {
    if on {
        1.0
    } else {
        0.0
    }
}
