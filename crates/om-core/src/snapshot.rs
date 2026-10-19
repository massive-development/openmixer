//! Engine state as plain values, read by id. The engine builds a snapshot on the control thread
//! (`DjEngine::snapshot`) from its atomics and control-side getters; readers (the UI, MIDI feedback)
//! then work on the copy, so they never hold an engine lock.
//!
//! Approximate by design: fields are read one at a time, not as one frame, so a snapshot can mix
//! values from two audio blocks (a playhead and a loop bound, say). Peaks are decaying meter values.
//! Nothing here is an audio-thread value; the snapshot is for display and feedback only.
//!
//! Stems, sampler and automix have no engine state yet: their snapshot types are empty placeholders
//! until those features land (Todos 19, 30, 38, 40).

use crate::param_id::{HOTCUES_PER_DECK, STEMS_PER_DECK};
use crate::state_id::{ChannelState, DeckState, MixerState, StateId};

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

/// Stem state for one stem of a deck. Placeholder until stems land (Todo 19).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StemSnapshot;

/// Sampler pad state. Placeholder until the sampler lands (Todo 40).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SamplerSnapshot;

/// Automix state. Placeholder until automix lands (Todo 38).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AutomixSnapshot;

fn flag(on: bool) -> f64 {
    if on {
        1.0
    } else {
        0.0
    }
}
