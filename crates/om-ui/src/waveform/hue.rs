//! How much of the colour wheel a track uses: the guard against a waveform collapsing to a few
//! hues. Pure, so tests and the tuning tool share it.

use super::{is_audible, overview_column, wave_hsl};
use crate::theme::ThemeTokens;
use om_analysis::WaveformData;

/// Overview columns the hue span is measured over.
pub const HUE_SPAN_COLUMNS: usize = 1024;

/// The lower and upper percentile of the span: the arc holding the middle 90% of the columns.
pub const HUE_SPAN_PERCENTILES: (f32, f32) = (5.0, 95.0);

/// Circular span, in degrees, of the 5th to 95th percentile of `hues` (degrees, any range): the
/// shortest arc that holds that share of them. Zero for fewer than two hues.
pub fn hue_span(hues: &[f32]) -> f32 {
    let n = hues.len();
    if n < 2 {
        return 0.0;
    }
    let (lo, hi) = HUE_SPAN_PERCENTILES;
    let keep = (((hi - lo) / 100.0 * n as f32).ceil() as usize).clamp(2, n);
    let mut sorted: Vec<f32> = hues.iter().map(|h| h.rem_euclid(360.0)).collect();
    sorted.sort_by(f32::total_cmp);
    (0..n)
        .map(|start| (sorted[(start + keep - 1) % n] - sorted[start]).rem_euclid(360.0))
        .fold(360.0, f32::min)
}

/// Hue, in degrees, of each of `columns` overview columns that is above the loudness floor.
pub fn overview_hues(data: &WaveformData, columns: usize, tokens: &ThemeTokens) -> Vec<f32> {
    let points = &data.overview().points;
    (0..columns)
        .map(|i| overview_column(points, i, columns))
        .filter(|c| is_audible(c, tokens))
        .map(|c| wave_hsl(&c, tokens).hue)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_of_a_tight_group_is_small_and_of_a_full_wheel_is_large() {
        let tight: Vec<f32> = (0..100).map(|i| 100.0 + i as f32 * 0.2).collect();
        assert!(hue_span(&tight) < 20.0);
        let wheel: Vec<f32> = (0..360).map(|i| i as f32).collect();
        assert!((hue_span(&wheel) - 324.0).abs() < 2.0);
        assert_eq!(hue_span(&[]), 0.0);
        assert_eq!(hue_span(&[10.0]), 0.0);
    }

    #[test]
    fn span_wraps_around_zero_and_drops_outliers() {
        let mut hues: Vec<f32> = (0..100).map(|i| (350.0 + i as f32 * 0.2) % 360.0).collect();
        hues.extend([180.0, 181.0, 182.0]);
        assert!(hue_span(&hues) < 30.0, "{}", hue_span(&hues));
    }
}
