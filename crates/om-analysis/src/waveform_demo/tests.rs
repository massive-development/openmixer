//! The demo loop: deterministic, bounded, and shaped so the waveform shows kicks, hats, a mid stab
//! and a quiet breakdown.

use super::*;
use crate::{AudioClip, BpmEstimator, BpmOptions, NullAnalysisContext, WaveformBuilder};

const RATE: f64 = 44_100.0;
const BPM: f64 = 124.0;
const SECONDS: f64 = 62.0;

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0, |m, x| m.max(x.abs()))
}

fn analyse(samples: Vec<f32>) -> crate::WaveformData {
    WaveformBuilder::default()
        .build(&AudioClip::new(samples, RATE), &NullAnalysisContext)
        .expect("not cancelled")
}

fn beat_seconds() -> f64 {
    60.0 / BPM
}

#[test]
fn has_the_requested_length_and_a_sane_level() {
    let loop_ = demo_loop(SECONDS, BPM, RATE);
    assert_eq!(loop_.len(), (SECONDS * RATE) as usize);
    let p = peak(&loop_);
    assert!(p <= 0.9 + 1e-6 && p > 0.5, "peak {p}");
    assert!(demo_loop(0.0, BPM, RATE).is_empty());
    assert!(
        demo_loop(5.0, 0.0, RATE).iter().all(|&x| x == 0.0),
        "no tempo, no loop"
    );
}

#[test]
fn is_deterministic() {
    assert_eq!(demo_loop(8.0, BPM, RATE), demo_loop(8.0, BPM, RATE));
}

#[test]
fn breakdown_bars_are_aligned_and_in_the_middle() {
    let (first, count) = breakdown_bars(31);
    assert_eq!(count, 4);
    assert_eq!(first % 4, 0, "starts on a phrase");
    assert!(
        first >= 8 && first + count <= 24,
        "middle of 31 bars: {first}"
    );
    assert_eq!(breakdown_bars(3).1, 0, "too short for a breakdown");
}

/// Mean of `f` over the base points whose time lies in `[from, to)` seconds.
fn mean_between(
    data: &crate::WaveformData,
    from: f64,
    to: f64,
    f: impl Fn(&crate::WaveformPoint) -> f32,
) -> f32 {
    let level = &data.levels()[0];
    let per_second = RATE / level.samples_per_point as f64;
    let (a, b) = ((from * per_second) as usize, (to * per_second) as usize);
    let slice = &level.points[a..b.min(level.points.len())];
    slice.iter().map(f).sum::<f32>() / slice.len() as f32
}

#[test]
fn shows_all_three_bands_with_low_on_kicks_and_high_in_the_gaps() {
    let data = analyse(demo_loop(SECONDS, BPM, RATE));
    let level = &data.levels()[0];
    for (name, f) in [
        ("low", (|p: &crate::WaveformPoint| p.low) as fn(&_) -> f32),
        ("mid", |p| p.mid),
        ("high", |p| p.high),
    ] {
        let top = level.points.iter().map(f).fold(0.0, f32::max);
        assert!(top > 0.2, "{name} peaks at {top}");
    }

    let bar = beat_seconds() * 4.0;
    let (first, count) = breakdown_bars(((SECONDS / bar) as usize).max(1));
    let (b0, b1) = (first as f64 * bar, (first + count) as f64 * bar);

    // The first 30 ms after a kick, outside the breakdown and the first bars: low leads.
    let mut kicks = 0;
    let mut low_leads = 0;
    let mut t = 2.0 * bar;
    while t < SECONDS - 2.0 * bar {
        if t < b0 - 0.1 || t > b1 + 0.1 {
            kicks += 1;
            let low = mean_between(&data, t, t + 0.03, |p| p.low);
            let mid = mean_between(&data, t, t + 0.03, |p| p.mid);
            let high = mean_between(&data, t, t + 0.03, |p| p.high);
            if low > mid && low > high {
                low_leads += 1;
            }
        }
        t += beat_seconds();
    }
    assert!(
        low_leads as f32 >= 0.9 * kicks as f32,
        "low leads on {low_leads} of {kicks} kicks"
    );

    // The first two bars of the breakdown are hats only: high leads.
    let (from, to) = (b0 + 0.2, b0 + 2.0 * bar);
    let low = mean_between(&data, from, to, |p| p.low);
    let mid = mean_between(&data, from, to, |p| p.mid);
    let high = mean_between(&data, from, to, |p| p.high);
    assert!(
        high > low && high > mid,
        "breakdown hats: {low} {mid} {high}"
    );
}

#[test]
fn the_breakdown_is_much_quieter_than_the_rest() {
    let data = analyse(demo_loop(SECONDS, BPM, RATE));
    let bar = beat_seconds() * 4.0;
    let (first, count) = breakdown_bars(((SECONDS / bar) as usize).max(1));
    let (b0, b1) = (first as f64 * bar, (first + count) as f64 * bar);
    let mean_peak = |from, to| mean_between(&data, from, to, |p| p.min.abs().max(p.max.abs()));
    let inside = mean_peak(b0, b1);
    let before = mean_peak(2.0 * bar, b0);
    let after = mean_peak(b1, SECONDS - bar);
    let rest = (before + after) / 2.0;
    assert!(inside < 0.5 * rest, "breakdown {inside} vs rest {rest}");
}

#[test]
fn the_beat_grid_analyser_finds_the_tempo() {
    let clip = AudioClip::new(demo_loop(30.0, BPM, RATE), RATE);
    let result = BpmEstimator::new(BpmOptions::default())
        .estimate(&clip, &NullAnalysisContext)
        .expect("the loop has a pulse");
    assert!((result.bpm - BPM).abs() < 0.5, "found {}", result.bpm);
}
