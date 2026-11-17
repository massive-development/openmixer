//! The linear kick-band flux: peaks on low bursts, silent on silence, same frames as the onsets.

use super::*;
use crate::test_signals::{add_kick, silence, RATE};
use crate::NullAnalysisContext;

const KICK_BAND: FrequencyBand = FrequencyBand {
    low_hz: 30.0,
    high_hz: 200.0,
};
const BURSTS_SECONDS: [f64; 3] = [1.0, 2.5, 4.0];

fn linear_flux(clip: &AudioClip) -> OnsetEnvelope {
    compute_linear_band_flux(
        clip,
        &OnsetOptions::default(),
        KICK_BAND,
        &NullAnalysisContext,
    )
    .expect("not cancelled")
}

fn bursts() -> AudioClip {
    let mut clip = silence(6.0, RATE);
    for t in BURSTS_SECONDS {
        add_kick(&mut clip, t, 0.9);
    }
    clip
}

/// Index of the largest value in `[from, to)`.
fn argmax(values: &[f32], from: usize, to: usize) -> usize {
    (from..to.min(values.len())).fold(
        from,
        |best, i| {
            if values[i] > values[best] {
                i
            } else {
                best
            }
        },
    )
}

#[test]
fn linear_flux_peaks_on_the_low_bursts() {
    let env = linear_flux(&bursts());
    let window = (0.2 * env.frame_rate) as usize;
    let mut quietest_burst = f32::MAX;
    for t in BURSTS_SECONDS {
        let at = env.index_of(t);
        let peak = argmax(&env.values, at as usize - window, at as usize + window);
        assert!(
            (peak as f64 - at).abs() <= 2.0,
            "burst {t}: peak {peak} vs {at}"
        );
        quietest_burst = quietest_burst.min(env.values[peak]);
    }
    let near_burst = |i: usize| {
        BURSTS_SECONDS
            .iter()
            .any(|&t| (i as f64 - env.index_of(t)).abs() <= window as f64)
    };
    let elsewhere = (0..env.values.len())
        .filter(|&i| !near_burst(i))
        .fold(0.0f32, |m, i| m.max(env.values[i]));
    assert!(
        elsewhere < 0.1 * quietest_burst,
        "{elsewhere} vs {quietest_burst}"
    );
}

#[test]
fn linear_flux_is_zero_for_silence() {
    let env = linear_flux(&silence(5.0, RATE));
    assert!(!env.values.is_empty());
    assert!(env.values.iter().all(|&v| v == 0.0));
}

#[test]
fn linear_flux_shares_the_onset_frame_grid() {
    let clip = bursts();
    let full = compute_band_onsets(
        &clip,
        &OnsetOptions::default(),
        &[FrequencyBand::default()],
        &NullAnalysisContext,
    )
    .expect("not cancelled");
    let linear = linear_flux(&clip);
    assert_eq!(linear.values.len(), full[0].values.len());
    assert_eq!(linear.frame_rate, full[0].frame_rate);
    assert_eq!(linear.start_seconds, full[0].start_seconds);
}
