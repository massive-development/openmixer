//! Global key estimation: clip chroma correlated against 24 rotated key templates (12 major, 12
//! minor). Stateless and thread-safe. [`KeyAnalyzer`] wraps it for the job service (id "key").
//! Port of `src/analysis/KeyEstimator.{h,cpp}`.

use crate::chromagram::{compute_chroma, Chroma, ChromaOptions};
use crate::musical_key::MusicalKey;
use crate::{jlimit, AnalysisContext, Analyzer, AudioClip};
use serde_json::{Map, Value};

// Project templates (not taken from published profiles): tonic strongest, then the tonic-triad
// tones, then the remaining scale tones; minor gives both 6ths and 7ths partial weight.
const MAJOR_TEMPLATE: Chroma = [1.0, 0.1, 0.5, 0.1, 0.8, 0.5, 0.1, 0.85, 0.1, 0.5, 0.1, 0.45];
const MINOR_TEMPLATE: Chroma = [1.0, 0.1, 0.5, 0.8, 0.1, 0.5, 0.1, 0.85, 0.4, 0.2, 0.4, 0.35];
const MIN_PEAK: f64 = 1.0e-6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyResult {
    pub key: MusicalKey,
    /// 0..1: margin of the best template over the runner-up.
    pub confidence: f64,
    /// Normalised to a peak of 1.
    pub chroma: Chroma,
    /// False: silent or atonal, the key is meaningless.
    pub tonal: bool,
}

impl KeyResult {
    fn atonal() -> Self {
        Self {
            key: MusicalKey::new(0, false),
            confidence: 0.0,
            chroma: [0.0; 12],
            tonal: false,
        }
    }

    /// Pretty-printed JSON payload.
    pub fn to_json(&self) -> String {
        let mut obj = Map::new();
        if !self.tonal {
            obj.insert("none".into(), true.into());
        } else {
            obj.insert("tonic".into(), self.key.tonic.into());
            obj.insert(
                "mode".into(),
                if self.key.minor { "minor" } else { "major" }.into(),
            );
            obj.insert("name".into(), self.key.name().into());
            obj.insert("camelot".into(), self.key.camelot().into());
            obj.insert("confidence".into(), self.confidence.into());
            obj.insert(
                "chroma".into(),
                Value::Array(self.chroma.iter().map(|&v| v.into()).collect()),
            );
        }
        serde_json::to_string_pretty(&Value::Object(obj)).unwrap_or_default()
    }

    /// `None` for malformed JSON, a non-object, or a `tonic` outside 0..=11 (or missing).
    pub fn from_json(json: &str) -> Option<Self> {
        let v: Value = serde_json::from_str(json).ok()?;
        if !v.is_object() {
            return None;
        }
        if truthy(v.get("none")) {
            return Some(Self::atonal());
        }

        let tonic = v.get("tonic")?.as_f64().map(|t| t as i32)?;
        if !(0..=11).contains(&tonic) {
            return None;
        }
        let mut chroma = [0.0f64; 12];
        if let Some(list) = v.get("chroma").and_then(Value::as_array) {
            if list.len() == 12 {
                for (dst, src) in chroma.iter_mut().zip(list) {
                    *dst = src.as_f64().unwrap_or(0.0);
                }
            }
        }
        Some(Self {
            key: MusicalKey::new(
                tonic,
                v.get("mode").and_then(Value::as_str) == Some("minor"),
            ),
            confidence: v.get("confidence").and_then(Value::as_f64).unwrap_or(0.0),
            chroma,
            tonal: true,
        })
    }
}

/// Mirrors JUCE's `static_cast<bool>(var)`: true, or a non-zero number.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0),
        _ => false,
    }
}

fn correlation(chroma: &Chroma, template: &Chroma, tonic: usize) -> f64 {
    let mean_a = chroma.iter().sum::<f64>() / 12.0;
    let mean_b = template.iter().sum::<f64>() / 12.0;
    let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
    for (pc, &c) in chroma.iter().enumerate() {
        let a = c - mean_a;
        let b = template[(pc + 12 - tonic) % 12] - mean_b;
        ab += a * b;
        aa += a * a;
        bb += b * b;
    }
    if aa > 0.0 && bb > 0.0 {
        ab / (aa * bb).sqrt()
    } else {
        0.0
    }
}

fn normalised(c: &Chroma) -> Chroma {
    let peak = c.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut out = [0.0f64; 12];
    for (o, &v) in out.iter_mut().zip(c) {
        *o = if peak > 0.0 { v / peak } else { 0.0 };
    }
    out
}

#[derive(Debug, Clone, Copy, Default)]
pub struct KeyEstimator {
    options: ChromaOptions,
}

impl KeyEstimator {
    pub fn new(options: ChromaOptions) -> Self {
        Self { options }
    }

    /// `None` when cancelled or without tonal content.
    pub fn estimate(&self, clip: &AudioClip, context: &dyn AnalysisContext) -> Option<KeyResult> {
        compute_chroma(clip, &self.options, context).and_then(|c| Self::classify(&c))
    }

    /// Best key for a chroma vector (also used by other chroma sources, such as stems).
    pub fn classify(chroma: &Chroma) -> Option<KeyResult> {
        let peak = chroma.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if peak < MIN_PEAK {
            return None;
        }

        let mut best = -2.0f64;
        let mut second = -2.0f64;
        let mut best_key = MusicalKey::new(0, false);
        for tonic in 0..12 {
            for minor in [false, true] {
                let template = if minor {
                    &MINOR_TEMPLATE
                } else {
                    &MAJOR_TEMPLATE
                };
                let score = correlation(chroma, template, tonic);
                if score > best {
                    second = best;
                    best = score;
                    best_key = MusicalKey::new(tonic as i32, minor);
                } else {
                    second = second.max(score);
                }
            }
        }
        Some(KeyResult {
            key: best_key,
            confidence: jlimit(0.0, 1.0, (best - second) * 5.0),
            chroma: normalised(chroma),
            tonal: true,
        })
    }
}

/// [`Analyzer`] wrapper for the job service (id "key").
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyAnalyzer {
    estimator: KeyEstimator,
}

impl Analyzer for KeyAnalyzer {
    fn id(&self) -> &str {
        "key"
    }

    fn version(&self) -> i32 {
        1
    }

    fn analyze(&self, clip: &AudioClip, context: &dyn AnalysisContext) -> Option<String> {
        let result = self.estimator.estimate(clip, context);
        if context.is_cancelled() {
            return None;
        }
        // Atonal or silent is a valid answer.
        Some(result.unwrap_or_else(KeyResult::atonal).to_json())
    }
}

#[cfg(test)]
mod tests;
