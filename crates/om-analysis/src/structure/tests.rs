//! Onsets, energy curve, sections and mix points on
//! synthetic tracks.

use super::*;
use crate::test_signals::{add_click, add_kick, add_tone_between, click_track, silence, RATE};
use crate::{
    compute_band_onsets, compute_energy_curve, pick_onsets, AudioClip, NullAnalysisContext,
    OnsetEnvelope, OnsetPickOptions,
};

const BPM: f64 = 120.0;

/// Adds a click and a kick every beat between `from` and `to`.
fn add_beats(clip: &mut AudioClip, from: f64, to: f64, gain: f32) {
    let mut t = from;
    while t < to - 1.0e-9 {
        add_click(clip, t, gain);
        add_kick(clip, t, gain);
        t += 60.0 / BPM;
    }
}

/// intro (quiet beats) 0-16 | main 16-48 | breakdown (soft pad) 48-64 | main 64-96 | outro 96-112
fn arranged_track() -> AudioClip {
    let mut clip = silence(112.0, RATE);
    add_beats(&mut clip, 0.0, 16.0, 0.08);
    add_beats(&mut clip, 16.0, 48.0, 0.8);
    add_tone_between(&mut clip, 220.0, 0.06, 48.0, 64.0);
    add_beats(&mut clip, 64.0, 96.0, 0.8);
    add_beats(&mut clip, 96.0, 112.0, 0.08);
    clip
}

fn estimate(clip: &AudioClip) -> Option<StructureResult> {
    StructureEstimator::default().estimate(clip, &NullAnalysisContext)
}

fn onset_options() -> OnsetOptions {
    OnsetOptions {
        fft_order: 10,
        hop: 512,
        mean_window_seconds: 0.1,
    }
}

#[test]
fn onset_picker_finds_every_click_once() {
    let clip = click_track(BPM, 10.0, 0.25, -1, 4, RATE);
    let env = compute_band_onsets(
        &clip,
        &onset_options(),
        &[FrequencyBand::default()],
        &NullAnalysisContext,
    )
    .expect("not cancelled");

    let onsets = pick_onsets(&env[0], &OnsetPickOptions::default());
    assert_eq!(onsets.len(), 20);
    for (i, &t) in onsets.iter().enumerate() {
        let expected = 0.25 + 0.5 * i as f64;
        assert!((t - expected).abs() <= 0.012, "{i}: {t}");
    }
}

#[test]
fn onset_picker_silence_has_no_onsets() {
    let flat = OnsetEnvelope {
        values: vec![0.0; 500],
        frame_rate: 86.0,
        start_seconds: 0.0,
    };
    assert!(pick_onsets(&flat, &OnsetPickOptions::default()).is_empty());
    assert!(pick_onsets(&OnsetEnvelope::default(), &OnsetPickOptions::default()).is_empty());
}

#[test]
fn finds_sections_and_mix_points() {
    let result = estimate(&arranged_track()).expect("not cancelled");

    let expected = [
        (0.0, "intro"),
        (16.0, "main"),
        (48.0, "breakdown"),
        (64.0, "main"),
        (96.0, "outro"),
    ];
    assert_eq!(result.sections.len(), expected.len());
    for (i, (start, label)) in expected.iter().enumerate() {
        assert!(
            (result.sections[i].start_seconds - start).abs() <= 1.0,
            "section {i}"
        );
        assert_eq!(result.sections[i].label, *label, "section {i}");
    }
    assert_eq!(result.sections.last().map(|s| s.end_seconds), Some(112.0));
    assert!((result.mix_in_seconds - 16.0).abs() <= 1.0);
    assert!((result.mix_out_seconds - 96.0).abs() <= 1.0);
    assert!(result.onset_seconds.len() > 150);
}

#[test]
fn energy_curve_is_relative_to_loudest_window() {
    let mut clip = silence(10.0, RATE);
    add_tone_between(&mut clip, 440.0, 0.5, 0.0, 5.0);
    add_tone_between(&mut clip, 440.0, 0.05, 5.0, 10.0); // 20 dB quieter
    let curve = compute_energy_curve(&clip, 0.5);
    assert_eq!(curve.values.len(), 20);
    assert!((curve.values[3] - 1.0).abs() <= 0.01);
    assert!((curve.values[15] - 0.5).abs() <= 0.02);
    assert!(compute_energy_curve(&AudioClip::default(), 0.5)
        .values
        .is_empty());
}

#[test]
fn steady_track_is_one_main_section() {
    let result = estimate(&click_track(BPM, 40.0, 0.0, 0, 4, RATE)).expect("not cancelled");
    assert_eq!(result.sections.len(), 1);
    assert_eq!(result.sections[0].label, "main");
    assert_eq!(result.mix_in_seconds, 0.0);
    assert_eq!(result.mix_out_seconds, 40.0);
}

#[test]
fn payload_round_trips() {
    let payload = StructureAnalyzer::default()
        .analyze(&arranged_track(), &NullAnalysisContext)
        .expect("not cancelled");
    let parsed = StructureResult::from_json(&payload).expect("payload parses");
    assert_eq!(parsed.sections.len(), 5);
    assert_eq!(parsed.sections[2].label, "breakdown");
    assert_eq!(parsed.energy.values.len(), 224);
    assert!((parsed.mix_out_seconds - 96.0).abs() <= 1.0);
    assert!(StructureResult::from_json("[]").is_none());
}
