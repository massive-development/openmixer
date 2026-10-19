//! Track structure for Automix and cue suggestions: onset times, a loudness curve, labelled sections
//! and suggested mix-in and mix-out points (start and end of the loud part). Stateless and
//! thread-safe. [`StructureAnalyzer`] is the job-service wrapper (id "structure"). Port of
//! `src/analysis/StructureAnalyzer.{h,cpp}`.

use crate::segmenter::{
    compute_energy_curve, segment_sections, EnergyCurve, Section, SegmentOptions,
};
use crate::{
    compute_band_onsets, pick_onsets, AnalysisContext, Analyzer, AudioClip, FrequencyBand,
    OnsetOptions, OnsetPickOptions, SubProgressContext,
};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct StructureResult {
    pub onset_seconds: Vec<f64>,
    pub energy: EnergyCurve,
    pub sections: Vec<Section>,
    pub mix_in_seconds: f64,
    pub mix_out_seconds: f64,
}

/// JUCE-style numeric read: a missing or non-numeric property is zero.
fn num(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn num_list(v: &Value, key: &str) -> Vec<f64> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|list| list.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect())
        .unwrap_or_default()
}

impl StructureResult {
    /// Pretty-printed JSON payload (format version 1).
    pub fn to_json(&self) -> String {
        let sections: Vec<Value> = self
            .sections
            .iter()
            .map(|s| {
                json!({
                    "start": s.start_seconds,
                    "end": s.end_seconds,
                    "energy": s.energy,
                    "label": s.label,
                })
            })
            .collect();
        let value = json!({
            "version": 1,
            "onsets": self.onset_seconds,
            "energyHop": self.energy.hop_seconds,
            "energy": self.energy.values,
            "sections": sections,
            "mixIn": self.mix_in_seconds,
            "mixOut": self.mix_out_seconds,
        });
        serde_json::to_string_pretty(&value).unwrap_or_default()
    }

    /// `None` for malformed JSON, a non-object, or a version other than 1.
    pub fn from_json(json: &str) -> Option<Self> {
        let v: Value = serde_json::from_str(json).ok()?;
        if !v.is_object() || num(&v, "version") as i32 != 1 {
            return None;
        }
        let sections = v
            .get("sections")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .map(|s| Section {
                        start_seconds: num(s, "start"),
                        end_seconds: num(s, "end"),
                        energy: num(s, "energy") as f32,
                        label: s
                            .get("label")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            onset_seconds: num_list(&v, "onsets"),
            energy: EnergyCurve {
                hop_seconds: num(&v, "energyHop"),
                values: num_list(&v, "energy")
                    .into_iter()
                    .map(|x| x as f32)
                    .collect(),
            },
            sections,
            mix_in_seconds: num(&v, "mixIn"),
            mix_out_seconds: num(&v, "mixOut"),
        })
    }
}

/// Mix-in is the start of the first main section, mix-out the end of the last; the whole clip
/// when there is no main section.
fn set_mix_points(r: &mut StructureResult, duration: f64) {
    r.mix_in_seconds = 0.0;
    r.mix_out_seconds = duration;
    let is_main = |s: &&Section| s.label == "main";
    if let Some(first) = r.sections.iter().find(is_main) {
        r.mix_in_seconds = first.start_seconds;
    }
    if let Some(last) = r.sections.iter().rev().find(is_main) {
        r.mix_out_seconds = last.end_seconds;
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StructureEstimator {
    options: SegmentOptions,
}

impl StructureEstimator {
    pub fn new(options: SegmentOptions) -> Self {
        Self { options }
    }

    /// `None` when cancelled.
    pub fn estimate(
        &self,
        clip: &AudioClip,
        context: &dyn AnalysisContext,
    ) -> Option<StructureResult> {
        let onset_step = SubProgressContext::new(context, 0.0, 0.9);
        let onset_options = OnsetOptions {
            fft_order: 10,
            hop: 512,
            mean_window_seconds: 0.1,
        };
        let envelopes = compute_band_onsets(
            clip,
            &onset_options,
            &[FrequencyBand::default()],
            &onset_step,
        )?;

        let energy = compute_energy_curve(clip, 0.5);
        let mut r = StructureResult {
            onset_seconds: pick_onsets(&envelopes[0], &OnsetPickOptions::default()),
            sections: segment_sections(&energy, &self.options),
            energy,
            mix_in_seconds: 0.0,
            mix_out_seconds: 0.0,
        };
        let duration = clip.duration_seconds();
        if let Some(last) = r.sections.last_mut() {
            last.end_seconds = last.end_seconds.min(duration);
        }
        set_mix_points(&mut r, duration);
        context.set_progress(1.0);
        Some(r)
    }
}

/// [`Analyzer`] wrapper for the job service (id "structure").
#[derive(Debug, Clone, Copy, Default)]
pub struct StructureAnalyzer {
    estimator: StructureEstimator,
}

impl Analyzer for StructureAnalyzer {
    fn id(&self) -> &str {
        "structure"
    }

    fn version(&self) -> i32 {
        1
    }

    fn analyze(&self, clip: &AudioClip, context: &dyn AnalysisContext) -> Option<String> {
        let result = self.estimator.estimate(clip, context)?;
        if context.is_cancelled() {
            return None;
        }
        Some(result.to_json())
    }
}

#[cfg(test)]
mod tests;
