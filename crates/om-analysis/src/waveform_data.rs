//! Multi-resolution waveform for overview and zoomed deck displays. Level 0 is the finest; each
//! further level merges [`LEVEL_FACTOR`] points, down to roughly [`OVERVIEW_POINTS`]. Each point
//! holds the full-band peak range and the RMS of three bands (low, mid, high) for colouring.
//! Immutable after building. Port of `src/analysis/WaveformData.{h,cpp}`; the JSON payload is the
//! same format as the C++ cache (base64 of 5 bytes per base-level point).

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde_json::{json, Value};

pub const WAVEFORM_FORMAT_VERSION: i64 = 1;
pub const LEVEL_FACTOR: usize = 4;
pub const OVERVIEW_POINTS: usize = 2048;
const BYTES_PER_POINT: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WaveformPoint {
    /// Full-band sample range, in [-1, 1].
    pub min: f32,
    pub max: f32,
    /// Band RMS, in [0, 1].
    pub low: f32,
    pub mid: f32,
    pub high: f32,
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
}

fn merge_points(points: &[WaveformPoint]) -> WaveformPoint {
    let mut out = WaveformPoint {
        min: 1.0,
        max: -1.0,
        ..WaveformPoint::default()
    };
    let (mut low, mut mid, mut high) = (0.0f64, 0.0f64, 0.0f64);
    for p in points {
        out.min = out.min.min(p.min);
        out.max = out.max.max(p.max);
        low += f64::from(p.low * p.low);
        mid += f64::from(p.mid * p.mid);
        high += f64::from(p.high * p.high);
    }
    let n = points.len() as f64;
    out.low = (low / n).sqrt() as f32;
    out.mid = (mid / n).sqrt() as f32;
    out.high = (high / n).sqrt() as f32;
    out
}

fn coarsen(fine: &WaveformLevel, factor: usize) -> WaveformLevel {
    WaveformLevel {
        samples_per_point: fine.samples_per_point * factor,
        points: fine.points.chunks(factor).map(merge_points).collect(),
    }
}

// Ranges use the full byte; band RMS is stored as sqrt for more resolution at low levels.
fn quantise_signed(v: f32) -> u8 {
    ((v.clamp(-1.0, 1.0) + 1.0) * 127.5).round() as u8
}

fn quantise_level(v: f32) -> u8 {
    (v.clamp(0.0, 1.0).sqrt() * 255.0).round() as u8
}

fn decode_signed(q: u8) -> f32 {
    f32::from(q) / 127.5 - 1.0
}

fn decode_level(q: u8) -> f32 {
    let r = f32::from(q) / 255.0;
    r * r
}

fn encode_points(points: &[WaveformPoint]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(points.len() * BYTES_PER_POINT);
    for p in points {
        bytes.extend_from_slice(&[
            quantise_signed(p.min),
            quantise_signed(p.max),
            quantise_level(p.low),
            quantise_level(p.mid),
            quantise_level(p.high),
        ]);
    }
    bytes
}

fn decode_points(bytes: &[u8]) -> Vec<WaveformPoint> {
    bytes
        .chunks_exact(BYTES_PER_POINT)
        .map(|b| WaveformPoint {
            min: decode_signed(b[0]),
            max: decode_signed(b[1]),
            low: decode_level(b[2]),
            mid: decode_level(b[3]),
            high: decode_level(b[4]),
        })
        .collect()
}

impl WaveformData {
    /// Builds the pyramid from `base`. An empty or zero-resolution base gives invalid data.
    pub fn new(sample_rate: f64, num_samples: i64, base: WaveformLevel) -> Self {
        let mut data = Self {
            sample_rate,
            num_samples,
            levels: Vec::new(),
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
    pub fn from_json(json: &str) -> Option<Self> {
        let v: Value = serde_json::from_str(json).ok()?;
        let version = v.get("version").and_then(Value::as_i64).unwrap_or(0);
        if version != WAVEFORM_FORMAT_VERSION {
            return None;
        }
        let Some(points) = v.get("points") else {
            return Some(Self::default());
        };

        let text = points.as_str().unwrap_or_default();
        let bytes = STANDARD.decode(text).ok()?;
        if bytes.len() % BYTES_PER_POINT != 0 {
            return None;
        }
        let sample_rate = v.get("sampleRate").and_then(Value::as_f64).unwrap_or(0.0);
        let num_samples = v.get("numSamples").and_then(Value::as_i64).unwrap_or(0);
        let samples_per_point = v
            .get("samplesPerPoint")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .max(0) as usize;
        let data = Self::new(
            sample_rate,
            num_samples,
            WaveformLevel {
                samples_per_point,
                points: decode_points(&bytes),
            },
        );
        data.is_valid().then_some(data)
    }
}
