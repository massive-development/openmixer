//! `WaveformData` levels packed as RGBA bytes (body, low, mid, high) for one GPU texture upload per
//! track. Each byte is the height the GPUI strip draws for that point, so both windows agree.
//! Runs on a control thread when a track's analysis lands.

use om_analysis::{WaveformData, WaveformLevel, WaveformPoint};
use om_ui::waveform::{band_heights, column, column_shape, BandHeights, Reference};

/// Share of the half-height a full-scale column reaches; the shader scales heights by it.
pub use om_ui::waveform::FILL;
/// A column with any height is drawn at least this far from the centre line, in px.
pub use om_ui::waveform::MIN_HALF_PX;
/// Brightness the played part of the track keeps; the shader dims by it.
pub use om_ui::waveform::PLAYED_BRIGHTNESS;

/// Bytes per packed point: body, low, mid, high.
pub const BYTES_PER_POINT: usize = 4;

/// One level's place in [`PeakBlob::bytes`]: its resolution and point count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeakLevel {
    pub samples_per_point: u32,
    pub len: u32,
}

/// Every level of a track's waveform, finest first, [`BYTES_PER_POINT`] bytes per point.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PeakBlob {
    pub sample_rate: f64,
    pub num_samples: i64,
    /// False for a colourless silhouette: the whole height is in the low byte, drawn grey.
    pub spectral: bool,
    pub levels: Vec<PeakLevel>,
    pub bytes: Vec<u8>,
}

/// Packs each level's points as body, low, mid, high bytes: the GPUI Low-Band body and band
/// heights (0..=1, before `FILL`), normalised by the track's reference, times 255.
pub fn pack_peaks(data: &WaveformData) -> PeakBlob {
    let reference = Reference::of(&data.overview().points);
    let spectral = data.has_spectrum();
    let total: usize = data.levels().iter().map(|l| l.points.len()).sum();
    let mut bytes = Vec::with_capacity(total * BYTES_PER_POINT);
    for level in data.levels() {
        put_level(level, reference, spectral, &mut bytes);
    }
    PeakBlob {
        sample_rate: data.sample_rate(),
        num_samples: data.num_samples(),
        spectral,
        levels: peak_levels(data),
        bytes,
    }
}

/// The level table of [`pack_peaks`] without packing the bytes.
pub fn peak_levels(data: &WaveformData) -> Vec<PeakLevel> {
    data.levels().iter().map(peak_level).collect()
}

// The texture header is u32; levels beyond that range cannot occur for any real track.
fn peak_level(level: &WaveformLevel) -> PeakLevel {
    PeakLevel {
        samples_per_point: u32::try_from(level.samples_per_point).unwrap_or(u32::MAX),
        len: u32::try_from(level.points.len()).unwrap_or(u32::MAX),
    }
}

fn put_level(level: &WaveformLevel, reference: Reference, spectral: bool, out: &mut Vec<u8>) {
    for point in &level.points {
        out.extend(point_bytes(point, reference, spectral));
    }
}

// One point as the GPUI strip folds it into a column, so the heights are its heights.
fn point_bytes(p: &WaveformPoint, reference: Reference, spectral: bool) -> [u8; BYTES_PER_POINT] {
    let c = column(std::slice::from_ref(p));
    let body = column_shape(&c, reference, spectral).body;
    let h: BandHeights = band_heights(&c, reference.peak, spectral);
    [body, h.low, h.mid, h.high].map(height_byte)
}

fn height_byte(height: f32) -> u8 {
    (height.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_ui::waveform::compress;

    // One past om-analysis's private 2048-point overview size, so `new` adds exactly one level.
    const BASE_POINTS: usize = 2049;

    fn point(peak: f32, low: f32, mid: f32, high: f32) -> WaveformPoint {
        WaveformPoint {
            min: -peak,
            max: peak,
            loudness: peak * 0.7,
            low,
            mid,
            high,
        }
    }

    fn data_of(points: Vec<WaveformPoint>) -> WaveformData {
        let len = points.len();
        let base = WaveformLevel {
            samples_per_point: 256,
            points,
        };
        WaveformData::new(44_100.0, (len * 256) as i64, base)
    }

    /// A track at full scale whose first point has `low` in the low band.
    fn fixture_with_low(low: f32) -> WaveformData {
        let mut points = vec![point(1.0, 1.0, 0.5, 0.2); BASE_POINTS];
        points[0] = point(1.0, low, 0.25, 0.0);
        data_of(points)
    }

    fn two_level_fixture() -> WaveformData {
        fixture_with_low(1.0)
    }

    /// A varied track: a ramp of peaks with every band balance.
    fn ramp() -> WaveformData {
        let points = (0..BASE_POINTS)
            .map(|i| {
                let peak = 0.05 + 0.9 * (i % 97) as f32 / 97.0;
                let share = (i % 7) as f32 / 7.0;
                point(peak, peak * share, peak * (1.0 - share), peak * 0.3)
            })
            .collect();
        data_of(points)
    }

    fn gpui_bytes(p: &WaveformPoint, reference: Reference) -> [u8; BYTES_PER_POINT] {
        let c = column(std::slice::from_ref(p));
        let h = band_heights(&c, reference.peak, true);
        let body = column_shape(&c, reference, true).body;
        [body, h.low, h.mid, h.high].map(|v| (v * 255.0).round() as u8)
    }

    // Byte `band` (0 body, 1 low, 2 mid, 3 high) of point `point`.
    fn byte(blob: &PeakBlob, point: usize, band: usize) -> u8 {
        blob.bytes[point * BYTES_PER_POINT + band]
    }

    #[test]
    fn pack_peaks_layout() {
        let data = two_level_fixture();
        let blob = pack_peaks(&data);
        let levels = data.levels();
        assert_eq!(levels.len(), 2);
        assert_eq!(blob.levels, peak_levels(&data));
        for (packed, level) in blob.levels.iter().zip(levels) {
            assert_eq!(packed.samples_per_point as usize, level.samples_per_point);
            assert_eq!(packed.len as usize, level.points.len());
        }
        assert_eq!(blob.levels[0].samples_per_point, 256);
        let total: usize = levels.iter().map(|l| l.points.len()).sum();
        assert_eq!(blob.bytes.len(), total * BYTES_PER_POINT);
        assert_eq!(
            (blob.sample_rate, blob.num_samples, blob.spectral),
            (44_100.0, data.num_samples(), true)
        );
    }

    #[test]
    fn pack_peaks_uses_band_compression() {
        let blob = pack_peaks(&two_level_fixture());
        assert_eq!(byte(&blob, 0, 1), 255);
        assert_eq!(byte(&blob, 0, 2), (compress(0.25) * 255.0).round() as u8);
        assert_eq!(byte(&blob, 0, 3), 0);
    }

    #[test]
    fn the_body_byte_is_the_gpui_low_band_body() {
        let data = ramp();
        let reference = Reference::of(&data.overview().points);
        let shape = column_shape(&column(&data.levels()[0].points[40..41]), reference, true);
        let blob = pack_peaks(&data);
        assert_eq!(
            byte(&blob, 40, 0),
            (shape.low_band().0 * 255.0).round() as u8
        );
        assert!(byte(&blob, 40, 0) > 0);
    }

    #[test]
    fn every_byte_is_the_gpui_band_height() {
        let data = ramp();
        let reference = Reference::of(&data.overview().points);
        let blob = pack_peaks(&data);
        let points = data.levels().iter().flat_map(|l| &l.points);
        for (p, bytes) in points.zip(blob.bytes.chunks_exact(BYTES_PER_POINT)) {
            assert_eq!(bytes, gpui_bytes(p, reference), "{p:?}");
        }
    }

    #[test]
    fn a_quiet_track_fills_the_strip_like_gpui() {
        // Every point peaks at 0.3, so the 95th percentile reference is 0.3.
        let data = data_of(vec![point(0.3, 0.3, 0.075, 0.0); BASE_POINTS]);
        assert!((Reference::of(&data.overview().points).peak - 0.3).abs() < 1e-6);
        let blob = pack_peaks(&data);
        assert_eq!(
            byte(&blob, 0, 1),
            255,
            "the loudest low band is full height"
        );
        assert_eq!(byte(&blob, 0, 2), (compress(0.25) * 255.0).round() as u8);
        assert_eq!(byte(&blob, 0, 3), 0);
    }

    #[test]
    fn one_loud_transient_does_not_shrink_the_rest() {
        let mut points = vec![point(0.4, 0.4, 0.0, 0.0); BASE_POINTS];
        points[3] = point(1.0, 1.0, 0.0, 0.0);
        let blob = pack_peaks(&data_of(points));
        assert_eq!(byte(&blob, 0, 1), 255);
        assert_eq!(byte(&blob, 3, 1), 255, "clamped, as GPUI clamps");
    }

    #[test]
    fn pack_peaks_clamps_out_of_range() {
        let blob = pack_peaks(&fixture_with_low(3.0));
        assert_eq!(byte(&blob, 0, 1), 255);
    }

    #[test]
    fn pack_peaks_empty_levels() {
        let blob = pack_peaks(&WaveformData::default());
        assert!(blob.levels.is_empty() && blob.bytes.is_empty());
    }
}
