//! Key detection on synthetic chord progressions,
//! Camelot mapping and harmonic compatibility.

use super::*;
use crate::test_signals::{add_noise, add_tone_between, click_track, silence, RATE};
use crate::{is_harmonically_compatible, MusicalKey, NullAnalysisContext};
use std::collections::BTreeSet;

const CHORD_SECONDS: f64 = 1.5;

/// Harmonic-rich note (4 partials, 1/n amplitudes) so the chroma has realistic overtones.
fn add_note(clip: &mut AudioClip, midi: i32, from: f64, to: f64) {
    let hz = 440.0 * 2f64.powf(f64::from(midi - 69) / 12.0);
    for partial in 1..=4 {
        add_tone_between(
            clip,
            hz * f64::from(partial),
            0.08 / partial as f32,
            from,
            to,
        );
    }
}

/// I-IV-V-I (major) or i-iv-V-i (minor, raised leading tone) around the tonic, twice.
fn cadence(tonic_midi: i32, minor: bool) -> AudioClip {
    let third = if minor { 3 } else { 4 };
    let chords: [Vec<i32>; 4] = [
        vec![0, third, 7],
        vec![5, 5 + third, 12],
        vec![7, 11, 14],
        vec![0, third, 7],
    ];
    let mut clip = silence(CHORD_SECONDS * 8.0, RATE);
    for pass in 0..2 {
        for (c, chord) in chords.iter().enumerate() {
            let from = CHORD_SECONDS * f64::from(pass * 4 + c as i32);
            for &interval in chord {
                add_note(&mut clip, tonic_midi + interval, from, from + CHORD_SECONDS);
            }
            // Tonic pedal in the bass.
            add_note(&mut clip, tonic_midi - 12, from, from + CHORD_SECONDS);
        }
    }
    clip
}

fn estimate(clip: &AudioClip) -> Option<KeyResult> {
    KeyEstimator::default().estimate(clip, &NullAnalysisContext)
}

macro_rules! key_case {
    ($name:ident, $midi:expr, $minor:expr, $key:expr, $camelot:expr) => {
        #[test]
        fn $name() {
            let result = estimate(&cadence($midi, $minor)).expect("cadence has tonal content");
            assert_eq!(result.key.name(), $key);
            assert_eq!(result.key.camelot(), $camelot);
            assert!(result.confidence > 0.0);
        }
    };
}

key_case!(cadence_in_key_c, 60, false, "C", "8B");
key_case!(cadence_in_key_am, 57, true, "Am", "8A");
key_case!(cadence_in_key_f_sharp, 66, false, "F#", "2B");
key_case!(cadence_in_key_d_sharp_minor, 63, true, "D#m", "2A");
key_case!(cadence_in_key_g, 55, false, "G", "9B");
key_case!(cadence_in_key_dm, 62, true, "Dm", "7A");
key_case!(cadence_in_key_a_sharp, 58, false, "A#", "6B");
key_case!(cadence_in_key_em, 64, true, "Em", "9A");

#[test]
fn survives_drums_and_noise() {
    let mut clip = cadence(62, false); // D major
    let drums = click_track(124.0, clip.duration_seconds(), 0.0, 0, 4, RATE);
    for (s, d) in clip.samples.iter_mut().zip(&drums.samples) {
        *s += d;
    }
    add_noise(&mut clip, 0.02, 1234);

    let result = estimate(&clip).expect("cadence has tonal content");
    assert_eq!(result.key.name(), "D");
}

#[test]
fn silence_has_no_key() {
    assert!(estimate(&silence(5.0, RATE)).is_none());
    let payload = KeyAnalyzer::default()
        .analyze(&silence(5.0, RATE), &NullAnalysisContext)
        .expect("silence is a valid result");
    let parsed = KeyResult::from_json(&payload).expect("payload parses");
    assert!(!parsed.tonal);
}

#[test]
fn classifies_chroma_directly() {
    let mut c_major_scale: Chroma = [1.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0];
    c_major_scale[0] = 2.0;
    c_major_scale[7] = 1.5;
    let result = KeyEstimator::classify(&c_major_scale).expect("chroma has content");
    assert_eq!(result.key, MusicalKey::new(0, false));
    assert_eq!(result.chroma[0], 1.0);
    assert!(KeyEstimator::classify(&[0.0; 12]).is_none());
}

#[test]
fn payload_round_trips() {
    let payload = KeyAnalyzer::default()
        .analyze(&cadence(57, true), &NullAnalysisContext)
        .expect("cadence has tonal content");
    let parsed = KeyResult::from_json(&payload).expect("payload parses");
    assert!(parsed.tonal);
    assert_eq!(parsed.key, MusicalKey::new(9, true));
    assert!(parsed.chroma[9] > 0.5);
    assert!(KeyResult::from_json("{\"tonic\": 14}").is_none());
}

#[test]
fn camelot_wheel_covers_all_keys() {
    let mut seen = BTreeSet::new();
    for tonic in 0..12 {
        for minor in [false, true] {
            let key = MusicalKey::new(tonic, minor);
            seen.insert(key.camelot());
            assert_eq!(
                MusicalKey::from_camelot(&key.camelot()),
                Some(key),
                "{}",
                key.name()
            );
            assert_eq!(
                MusicalKey::from_name(&key.name()),
                Some(key),
                "{}",
                key.name()
            );
            assert_eq!(key.relative().camelot_number(), key.camelot_number());
        }
    }
    assert_eq!(seen.len(), 24);
    assert_eq!(MusicalKey::new(5, false).camelot(), "7B"); // F
    assert_eq!(MusicalKey::new(11, true).camelot(), "10A"); // Bm
    assert_eq!(
        MusicalKey::from_name("Bbm"),
        Some(MusicalKey::new(10, true))
    );
    assert_eq!(MusicalKey::from_camelot("13A"), None);
    assert_eq!(MusicalKey::from_camelot("8C"), None);
    assert_eq!(MusicalKey::from_name("H"), None);
}

#[test]
fn harmonic_compatibility() {
    let c = MusicalKey::new(0, false);
    let g = MusicalKey::new(7, false);
    let f = MusicalKey::new(5, false);
    let am = MusicalKey::new(9, true);
    let d = MusicalKey::new(2, false);
    let em = MusicalKey::new(4, true);
    assert!(is_harmonically_compatible(&c, &c));
    assert!(is_harmonically_compatible(&c, &g));
    assert!(is_harmonically_compatible(&c, &f));
    assert!(is_harmonically_compatible(&c, &am));
    assert!(!is_harmonically_compatible(&c, &d));
    assert!(!is_harmonically_compatible(&c, &em));
}
