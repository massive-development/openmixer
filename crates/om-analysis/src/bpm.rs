//! Constant-tempo BPM and beat-grid estimation: full-band onset envelope, autocorrelation tempo,
//! joint tempo/phase refinement, then a downbeat from the low (kick) band. Stateless and
//! thread-safe. The job service wiring (id "bpm") is
//! exposed through [`BpmAnalyzer`].

use crate::beat_grid::BeatGridModel;
use crate::onset::{compute_band_onsets, FrequencyBand, OnsetEnvelope, OnsetOptions};
use crate::tempo::{
    estimate_coarse_bpm, find_downbeat_offset, period_frames, refine_beat_phase, BeatPhase,
};
use crate::{jlimit, AnalysisContext, Analyzer, AudioClip, SubProgressContext};
use serde_json::{Map, Value};

const MIN_SECONDS: f64 = 4.0;
const FULL_BAND: FrequencyBand = FrequencyBand {
    low_hz: 30.0,
    high_hz: 16_000.0,
};
const KICK_BAND: FrequencyBand = FrequencyBand {
    low_hz: 30.0,
    high_hz: 200.0,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BpmOptions {
    /// The result is folded by octaves into `[min_bpm, max_bpm)`.
    pub min_bpm: f64,
    pub max_bpm: f64,
    pub beats_per_bar: i32,
}

impl Default for BpmOptions {
    fn default() -> Self {
        Self {
            min_bpm: 70.0,
            max_bpm: 180.0,
            beats_per_bar: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BpmResult {
    pub bpm: f64,
    /// 0..1: how strongly onsets line up with the grid.
    pub confidence: f64,
    pub grid: BeatGridModel,
}

impl BpmResult {
    /// Pretty-printed JSON payload. The grid is omitted when it is invalid.
    pub fn to_json(&self) -> String {
        let mut obj = Map::new();
        obj.insert("bpm".into(), self.bpm.into());
        obj.insert("confidence".into(), self.confidence.into());
        if self.grid.is_valid() {
            obj.insert("grid".into(), self.grid.to_json());
        }
        serde_json::to_string_pretty(&Value::Object(obj)).unwrap_or_default()
    }

    /// `None` for malformed JSON, a missing `bpm`, or a `grid` that fails validation.
    pub fn from_json(json: &str) -> Option<Self> {
        let v: Value = serde_json::from_str(json).ok()?;
        if !v.is_object() {
            return None;
        }
        let bpm = v.get("bpm")?.as_f64()?;
        let confidence = v.get("confidence").and_then(Value::as_f64).unwrap_or(0.0);
        let grid = match v.get("grid") {
            Some(g) => BeatGridModel::from_json(g)?,
            None => BeatGridModel::default(),
        };
        Some(Self {
            bpm,
            confidence,
            grid,
        })
    }
}

fn fold_into_range(mut bpm: f64, lo: f64, hi: f64) -> f64 {
    while bpm > 0.0 && bpm < lo {
        bpm *= 2.0;
    }
    while bpm >= hi {
        bpm *= 0.5;
    }
    bpm
}

fn mean_of(values: &[f32]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().map(|&x| f64::from(x)).sum::<f64>() / values.len() as f64
}

/// Mean on-beat onset against the overall mean: 1 for a clean pulse, 0 when beats are no louder than average.
fn confidence_of(env: &OnsetEnvelope, beats: &BeatPhase) -> f64 {
    if beats.score > 0.0 {
        jlimit(0.0, 1.0, 1.0 - mean_of(&env.values) / beats.score)
    } else {
        0.0
    }
}

/// First downbeat at or after the start of the clip, in source frames.
fn first_downbeat_frame(
    env: &OnsetEnvelope,
    beats: &BeatPhase,
    offset: i32,
    beats_per_bar: i32,
    sample_rate: f64,
) -> f64 {
    let index = beats.phase + f64::from(offset) * period_frames(env, beats.bpm);
    let seconds = env.time_of(index);
    let bar_seconds = f64::from(beats_per_bar) * 60.0 / beats.bpm;
    (seconds - bar_seconds * (seconds / bar_seconds).floor()) * sample_rate
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BpmEstimator {
    options: BpmOptions,
}

impl BpmEstimator {
    pub fn new(options: BpmOptions) -> Self {
        Self { options }
    }

    /// `None` when cancelled, too short (under about 4 s) or without a detectable pulse.
    pub fn estimate(&self, clip: &AudioClip, context: &dyn AnalysisContext) -> Option<BpmResult> {
        if clip.duration_seconds() < MIN_SECONDS {
            return None;
        }

        let onset_step = SubProgressContext::new(context, 0.0, 0.8);
        let bands = compute_band_onsets(
            clip,
            &OnsetOptions::default(),
            &[FULL_BAND, KICK_BAND],
            &onset_step,
        )?;
        let full = &bands[0];

        let (min, max, bar) = (
            self.options.min_bpm,
            self.options.max_bpm,
            self.options.beats_per_bar,
        );
        let coarse = estimate_coarse_bpm(full, min, max);
        let beats = refine_beat_phase(full, fold_into_range(coarse, min, max));
        context.set_progress(0.95);
        if beats.bpm <= 0.0 || context.is_cancelled() {
            return None;
        }

        let offset = find_downbeat_offset(&bands[1], &beats, bar);
        let result = BpmResult {
            bpm: beats.bpm,
            confidence: confidence_of(full, &beats),
            grid: BeatGridModel::constant(
                beats.bpm,
                clip.sample_rate,
                first_downbeat_frame(full, &beats, offset, bar, clip.sample_rate),
                bar,
            ),
        };
        context.set_progress(1.0);
        Some(result)
    }
}

/// [`Analyzer`] wrapper for the job service (id "bpm").
#[derive(Debug, Clone, Copy, Default)]
pub struct BpmAnalyzer {
    estimator: BpmEstimator,
}

impl BpmAnalyzer {
    pub fn new(options: BpmOptions) -> Self {
        Self {
            estimator: BpmEstimator::new(options),
        }
    }
}

impl Analyzer for BpmAnalyzer {
    fn id(&self) -> &str {
        "bpm"
    }

    fn version(&self) -> i32 {
        1
    }

    fn analyze(&self, clip: &AudioClip, context: &dyn AnalysisContext) -> Option<String> {
        let result = self.estimator.estimate(clip, context);
        if context.is_cancelled() {
            return None;
        }
        // No pulse is a valid result: bpm 0, no grid.
        Some(result.unwrap_or_default().to_json())
    }
}

#[cfg(test)]
mod tests;
