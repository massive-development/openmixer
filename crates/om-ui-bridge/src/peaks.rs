//! `WaveformData` levels packed as RGB bytes (low, mid, high) for one GPU texture upload per track.
//! Runs on a control thread when a track's analysis lands.

use om_analysis::{WaveformData, WaveformLevel, WaveformPoint};
use om_ui::waveform::compress;

/// One level's place in [`PeakBlob::bytes`]: its resolution and point count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeakLevel {
    pub samples_per_point: u32,
    pub len: u32,
}

/// Every level of a track's waveform, finest first, three bytes per point.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PeakBlob {
    pub sample_rate: f64,
    pub num_samples: i64,
    pub levels: Vec<PeakLevel>,
    pub bytes: Vec<u8>,
}

/// Packs each level's points as low, mid, high bytes, compressed as the GPUI waveform draws them.
pub fn pack_peaks(data: &WaveformData) -> PeakBlob {
    let levels = data.levels();
    let total: usize = levels.iter().map(|l| l.points.len()).sum();
    let mut bytes = Vec::with_capacity(total * 3);
    levels.iter().for_each(|l| put_level(l, &mut bytes));
    PeakBlob {
        sample_rate: data.sample_rate(),
        num_samples: data.num_samples(),
        levels: levels.iter().map(peak_level).collect(),
        bytes,
    }
}

// The texture header is u32; levels beyond that range cannot occur for any real track.
fn peak_level(level: &WaveformLevel) -> PeakLevel {
    PeakLevel {
        samples_per_point: u32::try_from(level.samples_per_point).unwrap_or(u32::MAX),
        len: u32::try_from(level.points.len()).unwrap_or(u32::MAX),
    }
}

fn put_level(level: &WaveformLevel, out: &mut Vec<u8>) {
    level.points.iter().for_each(|p| out.extend(point_rgb(p)));
}

fn point_rgb(p: &WaveformPoint) -> [u8; 3] {
    [band_byte(p.low), band_byte(p.mid), band_byte(p.high)]
}

fn band_byte(v: f32) -> u8 {
    (compress(v).clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    // One past om-analysis's private 2048-point overview size, so `new` adds exactly one level.
    const BASE_POINTS: usize = 2049;

    fn fixture_with_low(low: f32) -> WaveformData {
        let first = WaveformPoint {
            low,
            mid: 0.25,
            high: 0.0,
            ..WaveformPoint::default()
        };
        let mut points = vec![WaveformPoint::default(); BASE_POINTS];
        points[0] = first;
        let base = WaveformLevel {
            samples_per_point: 256,
            points,
        };
        WaveformData::new(44_100.0, (BASE_POINTS * 256) as i64, base)
    }

    fn two_level_fixture() -> WaveformData {
        fixture_with_low(1.0)
    }

    #[test]
    fn pack_peaks_layout() {
        let data = two_level_fixture();
        let blob = pack_peaks(&data);
        let levels = data.levels();
        assert_eq!(levels.len(), 2);
        assert_eq!(blob.levels.len(), 2);
        for (packed, level) in blob.levels.iter().zip(levels) {
            assert_eq!(packed.samples_per_point as usize, level.samples_per_point);
            assert_eq!(packed.len as usize, level.points.len());
        }
        assert_eq!(blob.levels[0].samples_per_point, 256);
        let total: usize = levels.iter().map(|l| l.points.len()).sum();
        assert_eq!(blob.bytes.len(), total * 3);
        assert_eq!(
            (blob.sample_rate, blob.num_samples),
            (44_100.0, data.num_samples())
        );
    }

    #[test]
    fn pack_peaks_uses_band_compression() {
        let blob = pack_peaks(&two_level_fixture());
        assert_eq!(blob.bytes[0], 255);
        assert_eq!(blob.bytes[1], (compress(0.25) * 255.0).round() as u8);
        assert_eq!(blob.bytes[2], 0);
    }

    #[test]
    fn pack_peaks_clamps_out_of_range() {
        let blob = pack_peaks(&fixture_with_low(3.0));
        assert_eq!(blob.bytes[0], 255);
    }

    #[test]
    fn pack_peaks_empty_levels() {
        let blob = pack_peaks(&WaveformData::default());
        assert!(blob.levels.is_empty() && blob.bytes.is_empty());
    }
}
