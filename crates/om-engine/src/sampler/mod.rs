//! Sampler: 8 pads of 4 voices each, mixed into the master bus before the master inserts (where a
//! limiter sits).
//!
//! Threads. A [`Sample`] is decoded and resampled on a control thread ([`Sample::load`]), never on
//! the audio thread. [`SamplerControl`] (control side, `Send + Sync`) hands samples and
//! trigger/release/stop commands to [`SamplerAudio`] (audio side, a [`crate::ProcessStage`]) through
//! a fixed `rtrb` ring of plain commands. Gain, pitch and mode are latest-wins atomics, so a set is
//! never lost to a full ring. The audio side allocates, locks and drops nothing: a sample the audio
//! thread lets go of (a replaced pad sample, a finished voice's reference) goes back through a
//! second ring and is freed by [`SamplerControl::reclaim`]. When the commands ring is full a
//! trigger, release or stop is dropped and counted; nothing ever blocks.
//!
//! Mix point: [`crate::Mixer`] adds the sampler into the master bus after the channels are summed
//! and before the master gain, the master inserts and the meters. So the master fader scales the
//! sampler, a master insert (limiter) sees it, and the headphone cue mix hears it. The mixer itself
//! has no limiter or clamp; the device layer clamps at the very end.

mod audio;
mod control;
mod registry;
mod sample;
#[cfg(test)]
mod tests;

pub use audio::SamplerAudio;
pub use control::SamplerControl;
pub use om_core::PadMode;
pub use registry::SampleRegistry;
pub use sample::{Sample, SamplerError, MAX_SAMPLE_BYTES, MAX_SAMPLE_SECONDS};

use om_core::atomic::{AtomicF32, AtomicF64};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8};

/// Pads.
pub const PADS: usize = om_core::param_id::SAMPLER_PADS as usize;
/// Voices that can sound at once on one pad. Starting a fifth steals the oldest.
pub const VOICES_PER_PAD: usize = 4;
/// Voice slots per pad: the sounding voices plus those fading out after a steal or a release.
pub(crate) const SLOTS_PER_PAD: usize = VOICES_PER_PAD * 2;
/// Highest pad gain (linear).
pub const MAX_PAD_GAIN: f32 = 1.5;
/// Pitch limit in semitones either way.
pub const MAX_PITCH_SEMITONES: f32 = 24.0;
/// Highest sampler bus gain (linear).
pub const MAX_MASTER_GAIN: f32 = 2.0;
/// Entries in each ring.
pub(crate) const RING_CAPACITY: usize = 64;

// Const assertion: the id grammar and the engine agree on the pad count.
const _: () = assert!(PADS == 8);

/// A command from the control side. Plain data (the `Arc` moves a reference, never copies audio).
pub(crate) enum SamplerCmd {
    Assign {
        pad: u8,
        sample: std::sync::Arc<Sample>,
    },
    Clear {
        pad: u8,
    },
    /// `mode` overrides the pad's mode for this one voice (a MIDI gate press).
    Trigger {
        pad: u8,
        velocity: f32,
        mode: Option<PadMode>,
    },
    Release {
        pad: u8,
    },
    StopAll,
}

/// State shared by both halves.
pub(crate) struct Shared {
    pub gain: [AtomicF32; PADS],
    pub pitch: [AtomicF32; PADS],
    pub mode: [AtomicU8; PADS],
    pub loaded: [AtomicBool; PADS],
    /// Latest-wins "pad is held": set before a trigger is queued, cleared before a release is
    /// queued. The audio side polls it each block, so a Release lost to a full ring still ends the
    /// gate voices.
    pub held: [AtomicBool; PADS],
    pub name_hash: [AtomicU64; PADS],
    /// Sounding voices per pad, written by the audio side after each block.
    pub playing: [AtomicU8; PADS],
    pub voices_active: AtomicU32,
    pub dropped: AtomicU64,
    pub master_gain: AtomicF32,
    /// Engine sample rate as last prepared.
    pub sample_rate: AtomicF64,
}

impl Shared {
    fn new() -> Self {
        Self {
            gain: std::array::from_fn(|_| AtomicF32::new(1.0)),
            pitch: std::array::from_fn(|_| AtomicF32::new(0.0)),
            mode: std::array::from_fn(|_| AtomicU8::new(PadMode::OneShot.as_u8())),
            loaded: std::array::from_fn(|_| AtomicBool::new(false)),
            held: std::array::from_fn(|_| AtomicBool::new(false)),
            name_hash: std::array::from_fn(|_| AtomicU64::new(0)),
            playing: std::array::from_fn(|_| AtomicU8::new(0)),
            voices_active: AtomicU32::new(0),
            dropped: AtomicU64::new(0),
            master_gain: AtomicF32::new(1.0),
            sample_rate: AtomicF64::new(44_100.0),
        }
    }
}
