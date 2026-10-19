//! Offline analysis: STFT, onset envelopes, tempo and beat-grid estimation, key, structure and
//! waveform data. Port of `src/analysis/` (Todos 15-18, analysis side). Analysis runs on worker
//! threads, so it may allocate; it never runs on the audio thread. The job service, the analyser
//! registry and the result cache follow in a later step.

#![forbid(unsafe_code)]

mod analyzer;
mod beat_grid;
mod bpm;
mod chromagram;
mod clip;
mod context;
mod key;
mod musical_key;
mod onset;
mod picker;
mod segmenter;
mod stft;
mod structure;
mod tempo;
mod waveform;
mod waveform_data;

#[cfg(test)]
mod test_signals;

pub use analyzer::Analyzer;
pub use beat_grid::{make_engine_grid, AnalysedBeatGrid, BeatGridModel, TempoMarker};
pub use bpm::{BpmAnalyzer, BpmEstimator, BpmOptions, BpmResult};
pub use chromagram::{compute_chroma, Chroma, ChromaOptions};
pub use clip::AudioClip;
pub use context::{AnalysisContext, NullAnalysisContext, SubProgressContext};
pub use key::{KeyAnalyzer, KeyEstimator, KeyResult};
pub use musical_key::{is_harmonically_compatible, MusicalKey};
pub use onset::{compute_band_onsets, FrequencyBand, OnsetEnvelope, OnsetOptions};
pub use picker::{pick_onsets, OnsetPickOptions};
pub use segmenter::{compute_energy_curve, segment_sections, EnergyCurve, Section, SegmentOptions};
pub use stft::MagnitudeStft;
pub use structure::{StructureAnalyzer, StructureEstimator, StructureResult};
pub use tempo::{
    estimate_coarse_bpm, find_downbeat_offset, fit_to_onsets, period_frames, refine_beat_phase,
    BeatPhase,
};
pub use waveform::{WaveformAnalyzer, WaveformBuilder, WaveformOptions};
pub use waveform_data::{WaveformData, WaveformLevel, WaveformPoint};

/// Clamps `value` into `[lo, hi]` with JUCE's `jlimit` semantics (never panics on odd bounds).
pub(crate) fn jlimit(lo: f64, hi: f64, value: f64) -> f64 {
    value.max(lo).min(hi)
}
