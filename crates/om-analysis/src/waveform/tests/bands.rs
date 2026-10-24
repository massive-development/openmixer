//! Band peaks from the FFT: which band a tone lands in, the bimodal case the old Gaussian model
//! got wrong, the bound by the full-band peak and a smooth crossover.

use super::*;
use crate::test_signals::{add_tone, silence, RATE};
use std::f64::consts::PI;

/// Share of a point's energy in each band, from the stored band peaks (their squares sum to the
/// peak squared by construction).
fn shares(p: &WaveformPoint) -> [f32; 3] {
    let sq = [p.low, p.mid, p.high].map(|v| v * v);
    let total: f32 = sq.iter().sum();
    if total <= 0.0 {
        return [0.0; 3];
    }
    sq.map(|v| v / total)
}

fn peak(p: &WaveformPoint) -> f32 {
    p.min.abs().max(p.max.abs())
}

/// Mean band peaks over a window in the middle of a steady signal, with the mean full-band
/// peak in that window.
fn steady(clip: &AudioClip) -> (WaveformPoint, f32) {
    let data = build(clip);
    let points = &data.levels()[0].points;
    let n = points.len();
    let window = &points[n / 2..n / 2 + 64];
    let best = window.iter().map(peak).sum::<f32>() / 64.0;
    let sum = |f: fn(&WaveformPoint) -> f32| window.iter().map(f).sum::<f32>() / 64.0;
    let mean = WaveformPoint {
        min: 0.0,
        max: 0.0,
        loudness: sum(|p| p.loudness),
        low: sum(|p| p.low),
        mid: sum(|p| p.mid),
        high: sum(|p| p.high),
    };
    (mean, best)
}

#[test]
fn a_tone_lands_in_its_band() {
    for (hz, band) in [(60.0, 0), (900.0, 1), (9000.0, 2)] {
        let (p, best) = steady(&tone(hz, 0.5, 4.0));
        let bands = [p.low, p.mid, p.high];
        // Band peaks are peak * sqrt(share): the tone's own band carries (almost) the whole peak.
        assert!(bands[band] > 0.9 * best, "{hz} Hz: {bands:?} (peak {best})");
        for other in (0..3).filter(|&b| b != band) {
            assert!(
                bands[other] < 0.1 * best,
                "{hz} Hz leaks into band {other}: {bands:?} (peak {best})"
            );
        }
    }
}

#[test]
fn bass_plus_hats_leaves_the_mid_empty() {
    // The case the Gaussian model got wrong: two distant tones read as a broad mid.
    let mut clip = tone(60.0, 0.3, 4.0);
    add_tone(&mut clip, 9000.0, 0.3);
    let (p, _) = steady(&clip);
    let ratio = p.low / p.high;
    assert!(
        ratio > 0.7 && ratio < 1.42,
        "low and high within 3 dB: {p:?} ({ratio})"
    );
    assert!(p.mid < 0.2 * p.low, "mid {} vs low {}", p.mid, p.low);
}

#[test]
fn band_peaks_never_exceed_the_peak_and_shares_sum_to_one() {
    for style in [MixStyle::Full, MixStyle::HeavyBass, MixStyle::VocalLed] {
        let data = build(&mix_fixture_clip(style, 8.0));
        let mut loud = 0;
        for p in &data.levels()[0].points {
            let top = peak(p) * (1.0 + 1e-4);
            assert!(p.low <= top && p.mid <= top && p.high <= top, "{p:?}");
            if peak(p) > 0.02 {
                loud += 1;
                let sum: f32 = shares(p).iter().sum();
                assert!((sum - 1.0).abs() < 1e-3, "{p:?} sums to {sum}");
                let energy: f32 = [p.low, p.mid, p.high].iter().map(|v| v * v).sum();
                assert!(
                    (energy - peak(p) * peak(p)).abs() < 1e-3,
                    "{p:?}: {energy} vs {}",
                    peak(p) * peak(p)
                );
            }
        }
        assert!(loud > 100, "{style:?}");
    }
}

#[test]
fn the_crossover_is_smooth_across_200_hz() {
    // 150 -> 300 Hz over 2 s: the share moves from low to mid without a step.
    let seconds = 2.0;
    let samples: Vec<f32> = (0..(seconds * RATE) as usize)
        .map(|i| {
            let t = i as f64 / RATE;
            // Linear chirp: phase = 2 pi (f0 t + k t^2 / 2).
            let k = (300.0 - 150.0) / seconds;
            (0.5 * (2.0 * PI * (150.0 * t + 0.5 * k * t * t)).sin()) as f32
        })
        .collect();
    let data = build(&AudioClip::new(samples, RATE));
    // Compare the points that have a clear peak: a low tone has quiet points at its zero crossing.
    let s: Vec<[f32; 3]> = data.levels()[0].points.iter().map(shares).collect();
    let inner = &s[64..s.len() - 64];
    let biggest = inner
        .windows(2)
        .map(|w| (w[1][0] - w[0][0]).abs().max((w[1][1] - w[0][1]).abs()))
        .fold(0.0, f32::max);
    assert!(biggest < 0.15, "largest step {biggest}");
    assert!(inner[0][0] > 0.6, "starts low: {:?}", inner[0]);
    let last = inner[inner.len() - 1];
    assert!(last[1] > 0.5, "ends mid: {last:?}");
}

#[test]
fn silence_has_no_band_energy() {
    let data = build(&silence(2.0, RATE));
    assert!(data.levels()[0]
        .points
        .iter()
        .all(|p| p.low == 0.0 && p.mid == 0.0 && p.high == 0.0));
}
