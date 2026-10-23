//! Byte layout of the waveform payload. Format 2 stores `min, max, loudness, centroid, spread` and
//! one spare byte per point; format 1 (read only, for the silhouette) stored `min, max` and three
//! band levels.

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

fn quantise_unit(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
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
            quantise_level(p.loudness),
            quantise_unit(p.centroid),
            quantise_unit(p.spread),
            0,
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
            loudness: decode_level(b[2]),
            centroid: decode_unit(b[3]),
            spread: decode_unit(b[4]),
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
