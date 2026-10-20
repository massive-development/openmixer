//! The value types of a planned transition: [`Step`], [`TransitionPlan`], [`PlanOptions`] and the
//! typed [`PlanError`] that says why no plan could be made.
//!
//! Beats in a plan are beats of the **outgoing** deck's grid: the controller reads that deck's
//! position from the engine snapshot and asks the plan for the values at that beat, so a plan is a
//! pure function of one number. Asking twice gives the same values.
//!
//! Threads: pure data; any thread.

use crate::stem_fade::{Curve, StemFadePlan};
use om_core::{EngineSnapshot, ParamId};

pub use om_core::AutomixStyle as Style;

/// Lowest EQ band gain the mixer accepts, in dB (a full kill). Mirrors `ThreeBandEq::MIN_DB`.
pub const EQ_KILL_DB: f64 = -48.0;
/// Transition lengths the planner picks from, in beats (two, four and eight bars of 4/4).
pub const LENGTHS: [f64; 3] = [8.0, 16.0, 32.0];

/// One move of one control: from `from` to `to` over `length_beats` starting at `at_beat`.
/// `length_beats <= 0` is a step that reads `to` from `at_beat` on.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub at_beat: f64,
    pub length_beats: f64,
    pub id: ParamId,
    pub from: f64,
    pub to: f64,
    pub curve: Curve,
}

impl Step {
    pub fn new(
        at_beat: f64,
        length_beats: f64,
        id: ParamId,
        from: f64,
        to: f64,
        curve: Curve,
    ) -> Self {
        Self {
            at_beat,
            length_beats,
            id,
            from,
            to,
            curve,
        }
    }

    /// An instant change at `at_beat`.
    pub fn set(at_beat: f64, id: ParamId, from: f64, to: f64) -> Self {
        Self::new(at_beat, 0.0, id, from, to, Curve::Linear)
    }

    pub fn end_beat(&self) -> f64 {
        self.at_beat + self.length_beats.max(0.0)
    }

    /// The value at `beat`: `from` before the step, `to` from its end on, the curve between. Both
    /// ends are exact.
    pub fn value_at(&self, beat: f64) -> f64 {
        if beat.is_nan() || beat < self.at_beat {
            return self.from;
        }
        if self.length_beats <= 0.0 || beat >= self.end_beat() {
            return self.to;
        }
        self.curve.value(
            self.from,
            self.to,
            (beat - self.at_beat) / self.length_beats,
        )
    }
}

/// How the incoming deck is brought to the outgoing deck's tempo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoPlan {
    /// The deck whose beat the other follows (the outgoing one).
    pub leader_deck: u8,
    /// The deck that follows through sync (the incoming one).
    pub follower_deck: u8,
    /// Playback rate the follower needs to match the leader (1.0 = its own tempo).
    pub follower_rate: f64,
    /// `|follower_rate - 1|`: how far the follower is stretched.
    pub bpm_distance: f64,
}

/// A transition, ready to run. Immutable once made.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionPlan {
    /// The style that was planned (never `Auto`; a stem style that fell back reads `Crossfade`).
    pub style: Style,
    pub out_deck: u8,
    pub in_deck: u8,
    /// Bar-aligned beat on the outgoing grid where the incoming deck starts.
    pub start_beat: f64,
    /// 0 for a cut, otherwise 8, 16 or 32.
    pub length_beats: f64,
    /// Where the incoming deck is cued, in beats on its own grid.
    pub in_cue_beat: f64,
    /// Ordered by `at_beat`; every step lies within `[start_beat, start_beat + length_beats]`.
    pub steps: Vec<Step>,
    /// Stem moves (bass swap, bridge); run by `StemFadeRunner`.
    pub stem_plan: Option<StemFadePlan>,
    pub tempo_plan: TempoPlan,
    /// Applied once at the end: the outgoing deck stops, the incoming one leads sync, and the
    /// controls the plan moved on the outgoing side go back.
    pub cleanup: Vec<(ParamId, f64)>,
    /// What a cancel puts back: EQ, filter and FX that the plan moved, to the value they had at
    /// plan time. Never the crossfader, a fader, play or seek (the user owns those).
    pub restore_on_cancel: Vec<(ParamId, f64)>,
}

impl TransitionPlan {
    pub fn end_beat(&self) -> f64 {
        self.start_beat + self.length_beats
    }

    /// The distinct ids the steps drive, in first-use order.
    pub fn step_ids(&self) -> Vec<&ParamId> {
        let mut ids: Vec<&ParamId> = Vec::new();
        for step in &self.steps {
            if !ids.contains(&&step.id) {
                ids.push(&step.id);
            }
        }
        ids
    }

    /// The value every step id should have at `beat`: for each id, the latest step that has begun.
    /// An id no step has begun on is left out. Pure, so repeating it is harmless.
    pub fn values_at(&self, beat: f64) -> Vec<(ParamId, f64)> {
        self.step_ids()
            .into_iter()
            .filter_map(|id| {
                self.steps
                    .iter()
                    .rfind(|s| &s.id == id && beat >= s.at_beat)
                    .map(|s| (id.clone(), s.value_at(beat)))
            })
            .collect()
    }
}

/// Why no plan was made.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum PlanError {
    #[error("deck {0} has no beat grid")]
    NoGrid(u8),
    #[error("deck {0} has no usable tempo")]
    InvalidBpm(u8),
    #[error("tempo differs by {distance:.1}% (limit {limit:.1}%)")]
    BpmRatio { distance: f64, limit: f64 },
    #[error("outgoing track has {remaining:.0} beats left (needs {min:.0})")]
    OutgoingTooShort { remaining: f64, min: f64 },
    #[error("incoming track has {remaining:.0} beats after its cue (needs {needed:.0})")]
    IncomingTooShort { remaining: f64, needed: f64 },
    #[error("no room for the transition before the end of the outgoing track")]
    NoRoom,
    #[error("outgoing and incoming are the same deck")]
    SameDeck,
}

/// Knobs of one planning call. `Default` gives the app's settings.
#[derive(Debug, Clone)]
pub struct PlanOptions<'a> {
    /// Largest tempo gap (as a fraction, 0.06 = 6%) a beat-matched style accepts.
    pub max_bpm_ratio: f64,
    /// Shortest transition, in beats. Needs at least this much left on the outgoing track.
    pub min_length_beats: f64,
    /// A length to use instead of the planner's pick; snapped to 8, 16 or 32.
    pub length_beats: Option<f64>,
    /// Where the outgoing deck is now, in its beats. The start is never earlier than
    /// `out_position_beat + lead_beats`.
    pub out_position_beat: f64,
    pub lead_beats: f64,
    /// Start at the first bar the lead allows instead of at the track's mix-out point.
    pub start_asap: bool,
    /// The outgoing deck's playback rate now (for the tempo plan).
    pub out_rate: f64,
    /// The outgoing deck sits on the crossfader's left (position 0) side.
    pub out_on_left: bool,
    /// FX unit index of the echo on the outgoing channel's rack.
    pub echo_unit: u16,
    /// The engine now. Start values (crossfader, EQ, filter, stem gains and mutes) are read from
    /// it so a cancel puts back what the user had; without it the neutral values are used.
    pub snapshot: Option<&'a EngineSnapshot>,
}

impl Default for PlanOptions<'_> {
    fn default() -> Self {
        Self {
            max_bpm_ratio: 0.06,
            min_length_beats: 8.0,
            length_beats: None,
            out_position_beat: 0.0,
            lead_beats: 4.0,
            start_asap: false,
            out_rate: 1.0,
            out_on_left: true,
            echo_unit: 0,
            snapshot: None,
        }
    }
}
