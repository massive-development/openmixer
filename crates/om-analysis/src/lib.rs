//! Offline analysis: STFT, onset envelopes, tempo and beat-grid estimation, key, structure and
//! waveform data, plus the job service (registry, job runner, SQLite result cache). Analysis runs on worker threads, so it may allocate; it never
//! runs on the audio thread.

#![forbid(unsafe_code)]

mod analyzer;
mod beat_grid;
mod bpm;
mod chromagram;
mod clip;
mod clip_source;
mod context;
mod job;
mod job_runner;
mod key;
mod musical_key;
mod onset;
mod picker;
mod registry;
mod result_cache;
mod segmenter;
mod service;
mod spectrum;
mod sqlite_cache;
mod stft;
mod structure;
mod tempo;
mod waveform;
mod waveform_codec;
mod waveform_data;

#[cfg(test)]
mod test_signals;

pub use analyzer::{Analyzer, INTERFACE_VERSION};
pub use beat_grid::{make_engine_grid, AnalysedBeatGrid, BeatGridModel, TempoMarker};
pub use bpm::{BpmAnalyzer, BpmEstimator, BpmOptions, BpmResult};
pub use chromagram::{compute_chroma, Chroma, ChromaOptions};
pub use clip::AudioClip;
pub use clip_source::{hash_clip, hash_file_content, ClipSource, FileClipSource, MemoryClipSource};
pub use context::{AnalysisContext, NullAnalysisContext, SubProgressContext};
pub use job::{AnalysisJob, JobStatus};
pub use job_runner::run_analysis_job;
pub use key::{KeyAnalyzer, KeyEstimator, KeyResult};
pub use musical_key::{is_harmonically_compatible, MusicalKey};
pub use onset::{compute_band_onsets, FrequencyBand, OnsetEnvelope, OnsetOptions};
pub use picker::{pick_onsets, OnsetPickOptions};
pub use registry::{AnalyzerRegistry, RegistryError};
pub use result_cache::ResultCache;
pub use segmenter::{compute_energy_curve, segment_sections, EnergyCurve, Section, SegmentOptions};
pub use service::{AnalysisService, Listener};
pub use spectrum::{CENTROID_MIN_HZ, CENTROID_OCTAVES, SPREAD_MAX_OCTAVES};
pub use sqlite_cache::{CacheError, SqliteResultCache, SCHEMA_VERSION};
pub use stft::MagnitudeStft;
pub use structure::{StructureAnalyzer, StructureEstimator, StructureResult};
pub use tempo::{
    estimate_coarse_bpm, find_downbeat_offset, fit_to_onsets, period_frames, refine_beat_phase,
    BeatPhase,
};
pub use waveform::{WaveformAnalyzer, WaveformBuilder, WaveformOptions};
pub use waveform_data::{
    WaveformData, WaveformLevel, WaveformPoint, LEGACY_FORMAT_VERSION, SPECTRAL_FORMAT_VERSION,
    WAVEFORM_FORMAT_VERSION,
};

/// Clamps `value` into `[lo, hi]` (never panics on odd bounds).
pub(crate) fn jlimit(lo: f64, hi: f64, value: f64) -> f64 {
    value.max(lo).min(hi)
}
