//! Byte layout of the waveform payload. Format 3 stores `min, max, low, mid, high, loudness`.
//! Format 2 (read only) stored `min, max, loudness, centroid, spread` and one spare byte; its band
//! peaks are modelled from the centroid and spread. Format 1 (read only, for the silhouette) stored
//! `min, max` and three band levels.

use crate::spectrum::{CENTROID_MIN_HZ, CENTROID_OCTAVES, HIGH_EDGE_HZ, LOW_EDGE_HZ};
use crate::WaveformPoint;

pub(crate) const BYTES_PER_POINT: usize = 6;
pub(crate) const LEGACY_BYTES_PER_POINT: usize = 5;

// Ranges use the full byte; levels are stored as sqrt for more resolution at low values.
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

fn decode_unit(q: u8) -> f32 {
    f32::from(q) / 255.0
}

pub(crate) fn encode_points(points: &[WaveformPoint]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(points.len() * BYTES_PER_POINT);
    for p in points {
        bytes.extend_from_slice(&[
            quantise_signed(p.min),
            quantise_signed(p.max),
            quantise_level(p.low),
            quantise_level(p.mid),
            quantise_level(p.high),
            quantise_level(p.loudness),
        ]);
    }
    bytes
}

pub(crate) fn decode_points(bytes: &[u8]) -> Vec<WaveformPoint> {
    bytes
        .chunks_exact(BYTES_PER_POINT)
        .map(|b| WaveformPoint {
            min: decode_signed(b[0]),
            max: decode_signed(b[1]),
            low: decode_level(b[2]),
            mid: decode_level(b[3]),
            high: decode_level(b[4]),
            loudness: decode_level(b[5]),
        })
        .collect()
}

/// Spread is stored as octaves of standard deviation up to this value (format 2).
const V2_SPREAD_MAX_OCTAVES: f32 = 4.0;
/// Narrowest spread, in octaves, the Gaussian model uses (a pure tone is not infinitely sharp).
const V2_MIN_SIGMA_OCTAVES: f32 = 0.25;

/// Standard normal CDF (Abramowitz and Stegun 7.1.26 for erf, error under 2e-7).
fn normal_cdf(z: f32) -> f32 {
    let x = z.abs() / std::f32::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let poly = t
        * (0.254_829_6
            + t * (-0.284_496_74 + t * (1.421_413_7 + t * (-1.453_152_1 + t * 1.061_405_4))));
    let erf = 1.0 - poly * (-x * x).exp();
    if z >= 0.0 {
        0.5 * (1.0 + erf)
    } else {
        0.5 * (1.0 - erf)
    }
}

/// Octaves above [`CENTROID_MIN_HZ`] of a frequency.
fn octave_of(hz: f64) -> f32 {
    (hz / CENTROID_MIN_HZ).log2() as f32
}

/// Energy share (low, mid, high; summing to 1) of a format 2 point with this stored centroid and
/// spread: the spectrum is modelled as a Gaussian on the log-frequency axis and each band gets the
/// mass between its edges. A bimodal spectrum reads as a broad mid, which is why format 3 stores
/// real band peaks.
pub(crate) fn band_fractions(centroid: f32, spread: f32) -> [f32; 3] {
    let mean = centroid.clamp(0.0, 1.0) * CENTROID_OCTAVES;
    let sigma = (spread.clamp(0.0, 1.0) * V2_SPREAD_MAX_OCTAVES).max(V2_MIN_SIGMA_OCTAVES);
    let below_low = normal_cdf((octave_of(LOW_EDGE_HZ) - mean) / sigma);
    let below_mid = normal_cdf((octave_of(HIGH_EDGE_HZ) - mean) / sigma);
    [
        below_low,
        (below_mid - below_low).max(0.0),
        (1.0 - below_mid).max(0.0),
    ]
}

/// Format 2 points with band peaks from [`band_fractions`]: `peak * sqrt(share)`.
pub(crate) fn legacy_v2_points(bytes: &[u8]) -> Vec<WaveformPoint> {
    bytes
        .chunks_exact(BYTES_PER_POINT)
        .map(|b| {
            let (min, max) = (decode_signed(b[0]), decode_signed(b[1]));
            let peak = min.abs().max(max.abs()).min(1.0);
            let [low, mid, high] =
                band_fractions(decode_unit(b[3]), decode_unit(b[4])).map(|s| peak * s.sqrt());
            WaveformPoint {
                min,
                max,
                loudness: decode_level(b[2]),
                low,
                mid,
                high,
            }
        })
        .collect()
}

/// Format 1 points as silhouettes: the three band levels fold into one loudness.
pub(crate) fn legacy_points(bytes: &[u8]) -> Vec<WaveformPoint> {
    bytes
        .chunks_exact(LEGACY_BYTES_PER_POINT)
        .map(|b| {
            let bands = [b[2], b[3], b[4]].map(decode_level);
            WaveformPoint {
                min: decode_signed(b[0]),
                max: decode_signed(b[1]),
                loudness: bands.iter().map(|v| v * v).sum::<f32>().sqrt().min(1.0),
                ..WaveformPoint::default()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn centroid_of(hz: f64) -> f32 {
        octave_of(hz) / CENTROID_OCTAVES
    }

    #[test]
    fn fractions_sum_to_one_and_follow_the_centroid() {
        for hz in [30.0, 100.0, 800.0, 5000.0, 12_000.0] {
            let f = band_fractions(centroid_of(hz), 0.1);
            assert!((f.iter().sum::<f32>() - 1.0).abs() < 1e-4, "{hz}: {f:?}");
        }
        let bass = band_fractions(centroid_of(80.0), 0.05);
        assert!(bass[0] > 0.9, "{bass:?}");
        let mid = band_fractions(centroid_of(900.0), 0.05);
        assert!(mid[1] > 0.9, "{mid:?}");
        let hats = band_fractions(centroid_of(9000.0), 0.05);
        assert!(hats[2] > 0.9, "{hats:?}");
    }

    #[test]
    fn a_wide_spectrum_spreads_over_all_bands() {
        let f = band_fractions(centroid_of(700.0), 0.9);
        assert!(f.iter().all(|&share| share > 0.1), "{f:?}");
    }

    #[test]
    fn normal_cdf_matches_known_values() {
        assert!((normal_cdf(0.0) - 0.5).abs() < 1e-6);
        assert!((normal_cdf(1.0) - 0.841_345).abs() < 1e-4);
        assert!((normal_cdf(-1.96) - 0.025).abs() < 1e-3);
    }
}
