//! Multi-resolution waveform for overview and zoomed deck displays. Level 0 is the finest; each
//! further level merges [`LEVEL_FACTOR`] points, down to roughly [`OVERVIEW_POINTS`]. Each point
//! holds the full-band peak range plus loudness and the peak of the low, mid and high band.
//! Immutable after building. The JSON payload is base64 of 6 bytes per base-level point.

use crate::waveform_codec::{
    decode_points, encode_points, legacy_points, legacy_v2_points, BYTES_PER_POINT,
    LEGACY_BYTES_PER_POINT,
};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde_json::{json, Value};

pub const WAVEFORM_FORMAT_VERSION: i64 = 3;
/// The centroid and spread format: read only, mapped to band peaks through a model.
pub const SPECTRAL_FORMAT_VERSION: i64 = 2;
/// The first format: three band levels per point and no spectrum.
pub const LEGACY_FORMAT_VERSION: i64 = 1;
pub const LEVEL_FACTOR: usize = 4;
pub const OVERVIEW_POINTS: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WaveformPoint {
    /// Full-band sample range, in [-1, 1].
    pub min: f32,
    pub max: f32,
    /// RMS over the point's analysis window, in [0, 1].
    pub loudness: f32,
    /// Peak of the low (below about 200 Hz), mid and high (above about 3 kHz) band, each at most
    /// the full-band peak: `peak * sqrt(energy share of the band)`.
    pub low: f32,
    pub mid: f32,
    pub high: f32,
}

impl WaveformPoint {
    /// The loudest sample of the point.
    pub fn peak(&self) -> f32 {
        self.min.abs().max(self.max.abs()).min(1.0)
    }

    /// Merges equal-length points: widest range, band peaks held at their maximum (peak hold, as
    /// the full-band range does), loudness as RMS. A pyramid level and a strip column use this
    /// same rule.
    pub fn merge(points: &[WaveformPoint]) -> Self {
        let mut merged = Self {
            min: 1.0,
            max: -1.0,
            ..Self::default()
        };
        let mut energy = 0.0f32;
        for p in points {
            merged.min = merged.min.min(p.min);
            merged.max = merged.max.max(p.max);
            merged.low = merged.low.max(p.low);
            merged.mid = merged.mid.max(p.mid);
            merged.high = merged.high.max(p.high);
            energy += p.loudness * p.loudness;
        }
        if points.is_empty() {
            merged.min = 0.0;
            merged.max = 0.0;
        } else {
            merged.loudness = (energy / points.len() as f32).sqrt().min(1.0);
        }
        merged
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WaveformLevel {
    pub samples_per_point: usize,
    pub points: Vec<WaveformPoint>,
}

static EMPTY_LEVEL: WaveformLevel = WaveformLevel {
    samples_per_point: 0,
    points: Vec::new(),
};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WaveformData {
    sample_rate: f64,
    num_samples: i64,
    levels: Vec<WaveformLevel>,
    spectrum: bool,
}

fn coarsen(fine: &WaveformLevel, factor: usize) -> WaveformLevel {
    WaveformLevel {
        samples_per_point: fine.samples_per_point * factor,
        points: fine
            .points
            .chunks(factor)
            .map(WaveformPoint::merge)
            .collect(),
    }
}

impl WaveformData {
    /// Builds the pyramid from `base`. An empty or zero-resolution base gives invalid data.
    pub fn new(sample_rate: f64, num_samples: i64, base: WaveformLevel) -> Self {
        let mut data = Self {
            sample_rate,
            num_samples,
            levels: Vec::new(),
            spectrum: true,
        };
        if base.samples_per_point == 0 || base.points.is_empty() {
            return data;
        }
        data.levels.push(base);
        while data.levels[data.levels.len() - 1].points.len() > OVERVIEW_POINTS {
            let next = coarsen(&data.levels[data.levels.len() - 1], LEVEL_FACTOR);
            data.levels.push(next);
        }
        data
    }

    pub fn is_valid(&self) -> bool {
        self.levels.first().is_some_and(|l| !l.points.is_empty())
    }

    /// False for a silhouette read from a format 1 entry: ranges only, no usable colour data.
    pub fn has_spectrum(&self) -> bool {
        self.spectrum
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    pub fn num_samples(&self) -> i64 {
        self.num_samples
    }

    pub fn levels(&self) -> &[WaveformLevel] {
        &self.levels
    }

    /// Coarsest level that still has at least one point per pixel at this zoom.
    pub fn level_for(&self, samples_per_pixel: f64) -> &WaveformLevel {
        self.levels
            .iter()
            .rev()
            .find(|l| l.samples_per_point as f64 <= samples_per_pixel)
            .or_else(|| self.levels.first())
            .unwrap_or(&EMPTY_LEVEL)
    }

    /// The coarsest level; empty when there is no data.
    pub fn overview(&self) -> &WaveformLevel {
        self.levels.last().unwrap_or(&EMPTY_LEVEL)
    }

    /// Compact payload: the base level quantised to 8 bits per value, base64 in JSON.
    pub fn to_json(&self) -> String {
        let mut obj = json!({
            "version": WAVEFORM_FORMAT_VERSION,
            "sampleRate": self.sample_rate,
            "numSamples": self.num_samples,
        });
        if let (true, Some(base)) = (self.is_valid(), self.levels.first()) {
            obj["samplesPerPoint"] = json!(base.samples_per_point);
            obj["points"] = json!(STANDARD.encode(encode_points(&base.points)));
        }
        serde_json::to_string_pretty(&obj).unwrap_or_default()
    }

    /// `None` for a wrong version, malformed base64, a payload that is not a whole number of
    /// points, or a base level that is empty. A payload with no `points` is valid empty data.
    /// Format 1 payloads are not read here; see [`Self::silhouette_from_v1_json`].
    pub fn from_json(json: &str) -> Option<Self> {
        Self::parse(
            json,
            WAVEFORM_FORMAT_VERSION,
            BYTES_PER_POINT,
            decode_points,
        )
    }

    /// A format 2 payload (spectral centroid and spread) with band peaks estimated from a
    /// Gaussian model of the spectrum, for display while the format 3 analysis rebuilds. `None`
    /// for any other version or a damaged payload.
    pub fn from_v2_json(json: &str) -> Option<Self> {
        Self::parse(
            json,
            SPECTRAL_FORMAT_VERSION,
            BYTES_PER_POINT,
            legacy_v2_points,
        )
    }

    /// The ranges of a format 1 payload as a colourless silhouette, for display while the
    /// spectral analysis rebuilds. `None` for any other version or a damaged payload.
    pub fn silhouette_from_v1_json(json: &str) -> Option<Self> {
        let mut data = Self::parse(
            json,
            LEGACY_FORMAT_VERSION,
            LEGACY_BYTES_PER_POINT,
            legacy_points,
        )?;
        data.spectrum = false;
        Some(data)
    }

    fn parse(
        json: &str,
        version: i64,
        stride: usize,
        decode: fn(&[u8]) -> Vec<WaveformPoint>,
    ) -> Option<Self> {
        let v: Value = serde_json::from_str(json).ok()?;
        if v.get("version").and_then(Value::as_i64).unwrap_or(0) != version {
            return None;
        }
        let Some(points) = v.get("points") else {
            return Some(Self::default());
        };
        let bytes = STANDARD.decode(points.as_str().unwrap_or_default()).ok()?;
        if bytes.len() % stride != 0 {
            return None;
        }
        let sample_rate = v.get("sampleRate").and_then(Value::as_f64).unwrap_or(0.0);
        let num_samples = v.get("numSamples").and_then(Value::as_i64).unwrap_or(0);
        let samples_per_point = v
            .get("samplesPerPoint")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .max(0) as usize;
        let base = WaveformLevel {
            samples_per_point,
            points: decode(&bytes),
        };
        let data = Self::new(sample_rate, num_samples, base);
        data.is_valid().then_some(data)
    }
}
