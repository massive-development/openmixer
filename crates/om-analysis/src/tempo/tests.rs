//! The half-beat check on synthetic envelopes: period 40 frames (80 frames per second, 120 bpm).

use super::*;

const FRAME_RATE: f64 = 80.0;
const BPM: f64 = 120.0;
const PERIOD: usize = 40;
const LENGTH: usize = 20 * PERIOD;

/// Peaks of `gain` at `offset + PERIOD * k`, zero elsewhere.
fn peaks(offset: usize, gain: f32) -> Vec<f32> {
    (0..LENGTH)
        .map(|i| if i % PERIOD == offset { gain } else { 0.0 })
        .collect()
}

fn envelope(values: Vec<f32>) -> OnsetEnvelope {
    OnsetEnvelope {
        values,
        frame_rate: FRAME_RATE,
        start_seconds: 0.0,
    }
}

fn add(a: Vec<f32>, b: Vec<f32>) -> Vec<f32> {
    a.iter().zip(&b).map(|(x, y)| x + y).collect()
}

/// Strong off-beats at `20 + 40k`, weaker kicks at `40k`: the full band locks to the off-beats.
fn full_band() -> OnsetEnvelope {
    envelope(add(peaks(20, 1.0), peaks(0, 0.5)))
}

/// What the full-band search found: the off-beat phase.
fn found() -> BeatPhase {
    BeatPhase {
        bpm: BPM,
        phase: 20.0,
        score: 1.0,
    }
}

/// Distance between two phases on the circle of one period.
fn phase_distance(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(PERIOD as f64);
    d.min(PERIOD as f64 - d)
}

#[test]
fn moves_to_the_kicks_when_the_attack_is_half_a_beat_later() {
    let attack = envelope(peaks(0, 1.0));
    let out = resolve_half_beat(&full_band(), &attack, found());
    assert!(phase_distance(out.phase, 0.0) < 1.0, "phase {}", out.phase);
    assert!((out.bpm - BPM).abs() < 0.01, "bpm {}", out.bpm);
}

#[test]
fn stays_when_the_attack_is_on_the_found_phase() {
    let attack = envelope(peaks(20, 1.0));
    assert_eq!(resolve_half_beat(&full_band(), &attack, found()), found());
}

#[test]
fn stays_without_any_attack() {
    let silent = envelope(vec![0.0; LENGTH]);
    assert_eq!(resolve_half_beat(&full_band(), &silent, found()), found());
    let empty = envelope(Vec::new());
    assert_eq!(resolve_half_beat(&full_band(), &empty, found()), found());
}

#[test]
fn a_near_tie_keeps_the_found_phase() {
    let attack = envelope(add(peaks(20, 1.0), peaks(0, 1.2)));
    assert_eq!(resolve_half_beat(&full_band(), &attack, found()), found());
}
