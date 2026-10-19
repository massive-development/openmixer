//! Key-lock time-stretch behind a small real-time interface. The engine is chosen by the
//! [`Stretcher`] trait; [`RubberBandStretcher`] is the Rubber Band R3 implementation used by the
//! decks. Port of the time-stretch contract in `src/engine/TimeStretch.h`.
//!
//! Threads: [`Stretcher::prepare`] allocates and belongs to the control thread. Every other method
//! is audio-thread safe: no allocation, locks or I/O in the implementation.

mod ffi;
mod rubberband;

pub use rubberband::RubberBandStretcher;

use thiserror::Error;

/// Largest channel count accepted by [`Stretcher::prepare`].
pub const MAX_CHANNELS: usize = 8;

/// Errors from [`Stretcher::prepare`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum StretchError {
    #[error("channel count {0} is outside 1..={MAX_CHANNELS}")]
    ChannelCount(usize),
    #[error("sample rate {0} Hz is outside 8000..=192000 Hz")]
    SampleRate(u32),
    #[error("the stretch engine could not be created")]
    Create,
}

/// Real-time time-stretch with independent tempo and pitch control.
///
/// `time_ratio` is output length over input length (tempo is its inverse) and `pitch_scale` is
/// output pitch over input pitch. Output is pulled with [`Stretcher::available`] and
/// [`Stretcher::retrieve`] after feeding input with [`Stretcher::process`]. The engine delays its
/// output, so a caller that needs time alignment feeds [`Stretcher::start_pad`] silent frames first
/// and discards [`Stretcher::start_delay`] output frames.
pub trait Stretcher: Send {
    /// Creates the engine for `sample_rate`, `channels` and up to `max_block` frames per call to
    /// [`Stretcher::process`] or [`Stretcher::process_silence`]. Allocates. Replaces any earlier
    /// preparation. On error the stretcher is left unprepared.
    fn prepare(
        &mut self,
        sample_rate: u32,
        channels: usize,
        max_block: usize,
    ) -> Result<(), StretchError>;

    /// True once [`Stretcher::prepare`] has succeeded.
    fn is_prepared(&self) -> bool;

    /// Drops buffered audio and phase state, keeping the current ratios. Starts a fresh stream.
    fn reset(&mut self);

    /// Sets the time and pitch ratios. Cheap, and may be called before every block.
    fn set_ratios(&mut self, time_ratio: f64, pitch_scale: f64);

    /// The time ratio last set.
    fn time_ratio(&self) -> f64;

    /// The pitch scale last set.
    fn pitch_scale(&self) -> f64;

    /// Silent input frames the engine prefers before real input.
    fn start_pad(&self) -> usize;

    /// Output frames to discard after the start pad has been fed.
    fn start_delay(&self) -> usize;

    /// Processing latency the engine reports, in output frames.
    fn latency(&self) -> usize;

    /// Largest frame count one [`Stretcher::process`] or [`Stretcher::process_silence`] call takes.
    fn max_feed(&self) -> usize;

    /// Input frames the engine needs before it produces more output, clamped to [`Stretcher::max_feed`].
    fn samples_required(&self) -> usize;

    /// Feeds planar input: one slice per channel, all of equal length. Inputs longer than
    /// [`Stretcher::max_feed`] are split across engine calls.
    fn process(&mut self, input: &[&[f32]]);

    /// Feeds `frames` frames of silence, clamped to [`Stretcher::max_feed`].
    fn process_silence(&mut self, frames: usize);

    /// Output frames ready to retrieve.
    fn available(&self) -> usize;

    /// Copies up to [`Stretcher::available`] frames into the planar `output` (one slice per channel),
    /// limited by the shortest slice. Returns the number of frames written.
    fn retrieve(&mut self, output: &mut [&mut [f32]]) -> usize;
}
