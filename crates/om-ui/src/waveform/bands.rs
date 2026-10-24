//! Three fixed frequency bands for the waveform: low, mid and high each have one colour that never
//! changes, so a colour always means the same thing. Heights are compressive (`peak^0.6`) and
//! normalised by the track's loud end so quiet and loud tracks fill the strip alike.
//!
//! The analysis pyramid stores one full-band peak range plus loudness, spectral centroid and
//! spread per point, not per-band peaks. The band split here is therefore an approximation: the
//! point's spectrum is modelled as a Gaussian on the log-frequency axis (centroid and spread), the
//! energy share of each band is the mass of that Gaussian between the band edges, and a band's
//! peak is the full-band peak scaled by the square root of its share. A bimodal spectrum (kick and
//! hats with nothing between) reads as a broad mid; true per-band peaks need a filter bank in
//! `om-analysis` and would replace [`band_fractions`] only.

use super::Column;
use crate::theme::Colour;
use om_analysis::{WaveformPoint, CENTROID_MIN_HZ, CENTROID_OCTAVES, SPREAD_MAX_OCTAVES};

/// Upper edge of the low band.
pub const LOW_MAX_HZ: f64 = 200.0;
/// Upper edge of the mid band (the high band starts here).
pub const MID_MAX_HZ: f64 = 3000.0;

pub const LOW_COLOUR: Colour = Colour::from_argb(0xFF25_63FF);
pub const MID_COLOUR: Colour = Colour::from_argb(0xFFFF_A726);
pub const HIGH_COLOUR: Colour = Colour::from_argb(0xFFF5_F7FA);
/// A waveform whose spectrum is not analysed yet is one neutral grey silhouette.
pub const SILHOUETTE_COLOUR: Colour = Colour::from_argb(0xFF8A_94A6);

/// Display height is `peak^AMPLITUDE_EXPONENT`: quiet detail stays visible next to loud hits.
pub const AMPLITUDE_EXPONENT: f32 = 0.6;
/// Share of the original brightness the already-played part of the track keeps.
pub const PLAYED_BRIGHTNESS: f32 = 0.45;
/// Percentile of the point peaks that maps to full height.
pub const REFERENCE_PERCENTILE: f32 = 95.0;
/// The reference never falls below this, so near-silent tracks are not blown up into noise.
pub const MIN_REFERENCE_PEAK: f32 = 0.1;
/// Narrowest spread, in octaves, the Gaussian model uses (a pure tone is not infinitely sharp).
const MIN_SIGMA_OCTAVES: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    Low,
    Mid,
    High,
}

impl Band {
    /// Back to front: the low band is drawn first and the high band last, on top.
    pub const ALL: [Band; 3] = [Band::Low, Band::Mid, Band::High];

    pub const fn colour(self) -> Colour {
        match self {
            Band::Low => LOW_COLOUR,
            Band::Mid => MID_COLOUR,
            Band::High => HIGH_COLOUR,
        }
    }
}

/// Display heights of the three bands, each 0..=1 of the half-height of the strip.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BandHeights {
    pub low: f32,
    pub mid: f32,
    pub high: f32,
}

impl BandHeights {
    pub fn get(&self, band: Band) -> f32 {
        match band {
            Band::Low => self.low,
            Band::Mid => self.mid,
            Band::High => self.high,
        }
    }
}

/// The colour each band is painted with: the fixed three, or one grey for a silhouette.
pub fn band_colours(spectral: bool) -> [Colour; 3] {
    if spectral {
        Band::ALL.map(Band::colour)
    } else {
        [SILHOUETTE_COLOUR; 3]
    }
}

/// Compressive amplitude: 0..=1 in, 0..=1 out.
pub fn compress(amplitude: f32) -> f32 {
    amplitude.clamp(0.0, 1.0).powf(AMPLITUDE_EXPONENT)
}

/// The peak that maps to full height: the [`REFERENCE_PERCENTILE`] of the points' peaks, at least
/// [`MIN_REFERENCE_PEAK`]. Computed once per track from the overview level.
pub fn reference_peak(points: &[WaveformPoint]) -> f32 {
    if points.is_empty() {
        return 1.0;
    }
    let mut peaks: Vec<f32> = points
        .iter()
        .map(|p| p.min.abs().max(p.max.abs()).min(1.0))
        .collect();
    peaks.sort_by(f32::total_cmp);
    let rank = ((REFERENCE_PERCENTILE / 100.0) * (peaks.len() - 1) as f32).round() as usize;
    peaks[rank.min(peaks.len() - 1)].max(MIN_REFERENCE_PEAK)
}

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

/// Energy share (low, mid, high; summing to 1) of a point with this stored centroid and spread.
pub fn band_fractions(centroid: f32, spread: f32) -> [f32; 3] {
    let mean = centroid.clamp(0.0, 1.0) * CENTROID_OCTAVES;
    let sigma = (spread.clamp(0.0, 1.0) * SPREAD_MAX_OCTAVES).max(MIN_SIGMA_OCTAVES);
    let below_low = normal_cdf((octave_of(LOW_MAX_HZ) - mean) / sigma);
    let below_mid = normal_cdf((octave_of(MID_MAX_HZ) - mean) / sigma);
    [
        below_low,
        (below_mid - below_low).max(0.0),
        (1.0 - below_mid).max(0.0),
    ]
}

/// Band heights of one column. `reference` is the track's [`reference_peak`]. A column without
/// spectrum (`spectral` false) puts its whole height in the low slot, which the caller paints grey
/// (see [`band_colours`]).
pub fn band_heights(c: &Column, reference: f32, spectral: bool) -> BandHeights {
    let peak = (c.peak / reference.max(f32::EPSILON)).min(1.0);
    if !spectral {
        return BandHeights {
            low: compress(peak),
            ..BandHeights::default()
        };
    }
    let [low, mid, high] = band_fractions(c.centroid, c.spread);
    let height = |share: f32| compress(peak * share.sqrt());
    BandHeights {
        low: height(low),
        mid: height(mid),
        high: height(high),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(peak: f32, hz: f64, spread: f32) -> Column {
        Column {
            peak,
            loudness: peak * 0.7,
            centroid: octave_of(hz) / CENTROID_OCTAVES,
            spread,
        }
    }

    #[test]
    fn band_colours_are_the_three_fixed_ones() {
        assert_eq!(Band::Low.colour().argb(), 0xFF25_63FF);
        assert_eq!(Band::Mid.colour().argb(), 0xFFFF_A726);
        assert_eq!(Band::High.colour().argb(), 0xFFF5_F7FA);
        assert_eq!(band_colours(true), [LOW_COLOUR, MID_COLOUR, HIGH_COLOUR]);
        assert_eq!(band_colours(false), [SILHOUETTE_COLOUR; 3]);
        for band in Band::ALL {
            assert_eq!(band.colour().alpha(), 1.0, "opaque: no blending");
        }
    }

    #[test]
    fn compression_lifts_quiet_values_and_keeps_the_ends() {
        assert_eq!(compress(0.0), 0.0);
        assert_eq!(compress(1.0), 1.0);
        assert!((compress(0.25) - 0.25f32.powf(0.6)).abs() < 1e-6);
        assert!(compress(0.1) > 0.1 * 2.0, "quiet detail is lifted");
        assert_eq!(compress(7.0), 1.0, "clamped");
        assert_eq!(compress(-1.0), 0.0);
    }

    #[test]
    fn fractions_sum_to_one_and_follow_the_centroid() {
        for hz in [30.0, 100.0, 800.0, 5000.0, 12_000.0] {
            let f = band_fractions(octave_of(hz) / CENTROID_OCTAVES, 0.1);
            assert!((f.iter().sum::<f32>() - 1.0).abs() < 1e-4, "{hz}: {f:?}");
        }
        let bass = band_fractions(octave_of(80.0) / CENTROID_OCTAVES, 0.05);
        assert!(bass[0] > 0.9, "{bass:?}");
        let mid = band_fractions(octave_of(900.0) / CENTROID_OCTAVES, 0.05);
        assert!(mid[1] > 0.9, "{mid:?}");
        let hats = band_fractions(octave_of(9000.0) / CENTROID_OCTAVES, 0.05);
        assert!(hats[2] > 0.9, "{hats:?}");
    }

    #[test]
    fn a_wide_spectrum_spreads_over_all_bands() {
        let f = band_fractions(octave_of(700.0) / CENTROID_OCTAVES, 0.9);
        assert!(f.iter().all(|&share| share > 0.1), "{f:?}");
    }

    #[test]
    fn normal_cdf_matches_known_values() {
        assert!((normal_cdf(0.0) - 0.5).abs() < 1e-6);
        assert!((normal_cdf(1.0) - 0.841_345).abs() < 1e-4);
        assert!((normal_cdf(-1.96) - 0.025).abs() < 1e-3);
    }

    #[test]
    fn the_dominant_band_sets_the_tallest_bar() {
        let kick = band_heights(&column(0.8, 60.0, 0.05), 1.0, true);
        assert!(kick.low > kick.mid && kick.low > kick.high, "{kick:?}");
        let hat = band_heights(&column(0.8, 9000.0, 0.05), 1.0, true);
        assert!(hat.high > hat.low && hat.high > hat.mid, "{hat:?}");
        assert!(kick.low > 0.8 && kick.low <= 1.0);
    }

    #[test]
    fn heights_are_normalised_by_the_reference_peak() {
        let quiet = column(0.3, 60.0, 0.05);
        let raw = band_heights(&quiet, 1.0, true);
        let boosted = band_heights(&quiet, 0.3, true);
        assert!(boosted.low > raw.low);
        assert!(boosted.low <= 1.0, "never taller than the strip");
    }

    #[test]
    fn a_silhouette_is_one_grey_bar() {
        let h = band_heights(&column(0.5, 60.0, 0.05), 1.0, false);
        assert!(h.low > 0.0);
        assert_eq!((h.mid, h.high), (0.0, 0.0));
    }

    #[test]
    fn reference_peak_is_the_95th_percentile_with_a_floor() {
        let point = |peak: f32| WaveformPoint {
            min: -peak,
            max: peak,
            ..Default::default()
        };
        // 100 points: 0.01 ..= 1.00. The 95th percentile is about 0.95.
        let points: Vec<_> = (1..=100).map(|i| point(i as f32 / 100.0)).collect();
        assert!((reference_peak(&points) - 0.95).abs() < 0.02);
        // One loud transient does not dominate.
        let mut spiky: Vec<_> = (0..100).map(|_| point(0.4)).collect();
        spiky[3] = point(1.0);
        assert!((reference_peak(&spiky) - 0.4).abs() < 1e-6);
        // A near-silent track keeps the floor.
        assert_eq!(reference_peak(&[point(0.001); 10]), MIN_REFERENCE_PEAK);
        assert_eq!(reference_peak(&[]), 1.0);
    }
}
