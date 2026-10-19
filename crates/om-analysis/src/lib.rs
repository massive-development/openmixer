//! Offline analysis: STFT, onset envelopes, tempo and beat-grid estimation. Port of
//! `src/analysis/` (Todo 15 DSP half). Analysis runs on worker threads, so it may allocate; it
//! never runs on the audio thread. Key, chroma, structure, waveform, the job service and the result
//! cache follow in later steps.

#![forbid(unsafe_code)]

mod analyzer;
mod beat_grid;
mod bpm;
mod clip;
mod context;
mod onset;
mod picker;
mod stft;
mod tempo;

#[cfg(test)]
mod test_signals;

pub use analyzer::Analyzer;
pub use beat_grid::{make_engine_grid, AnalysedBeatGrid, BeatGridModel, TempoMarker};
pub use bpm::{BpmAnalyzer, BpmEstimator, BpmOptions, BpmResult};
pub use clip::AudioClip;
pub use context::{AnalysisContext, NullAnalysisContext, SubProgressContext};
pub use onset::{compute_band_onsets, FrequencyBand, OnsetEnvelope, OnsetOptions};
pub use picker::{pick_onsets, OnsetPickOptions};
pub use stft::MagnitudeStft;
pub use tempo::{
    estimate_coarse_bpm, find_downbeat_offset, fit_to_onsets, period_frames, refine_beat_phase,
    BeatPhase,
};

/// Clamps `value` into `[lo, hi]` with JUCE's `jlimit` semantics (never panics on odd bounds).
pub(crate) fn jlimit(lo: f64, hi: f64, value: f64) -> f64 {
    value.max(lo).min(hi)
}
