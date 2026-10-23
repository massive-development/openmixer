//! Multi-resolution levels, peaks, spectral points, the pyramid merge, serialisation, the old-cache
//! path and the cost limits.

use super::*;
use crate::test_signals::{add_tone, add_tone_between, silence, RATE};
use crate::{NullAnalysisContext, CENTROID_MIN_HZ, CENTROID_OCTAVES, SPREAD_MAX_OCTAVES};
use om_testing::signals::{mix_fixture, noise, MixStyle};

mod cost;

fn build(clip: &AudioClip) -> WaveformData {
    WaveformBuilder::default()
        .build(clip, &NullAnalysisContext)
        .unwrap_or_default()
}

fn tone(hz: f64, gain: f32, seconds: f64) -> AudioClip {
    let mut clip = silence(seconds, RATE);
    add_tone(&mut clip, hz, gain);
    clip
}

/// Mean of the middle half of the base level (skips the edges).
fn middle(data: &WaveformData) -> WaveformPoint {
    let points = &data.levels()[0].points;
    let (from, to) = (points.len() / 4, points.len() * 3 / 4);
    let n = (to - from) as f32;
    let sum = |f: fn(&WaveformPoint) -> f32| points[from..to].iter().map(f).sum::<f32>() / n;
    WaveformPoint {
        min: 0.0,
        max: 0.0,
        loudness: sum(|p| p.loudness),
        centroid: sum(|p| p.centroid),
        spread: sum(|p| p.spread),
    }
}

fn centroid_hz(p: &WaveformPoint) -> f64 {
    CENTROID_MIN_HZ * 2f64.powf(f64::from(p.centroid * CENTROID_OCTAVES))
}

fn octaves_between(a: f64, b: f64) -> f64 {
    (a / b).log2().abs()
}

#[test]
fn builds_levels_down_to_overview() {
    let data = build(&silence(60.0, RATE));
    assert!(data.is_valid());
    let levels = data.levels();
    assert_eq!(levels.len(), 3);
    assert_eq!(levels[0].samples_per_point, 128);
    assert_eq!(levels[0].points.len(), 20_672);
    assert_eq!(levels[1].samples_per_point, 512);
    assert_eq!(levels[2].points.len(), 1292);
    assert!(std::ptr::eq(data.overview(), &levels[2]));

    assert_eq!(data.level_for(100.0).samples_per_point, 128);
    assert_eq!(data.level_for(600.0).samples_per_point, 512);
    assert_eq!(data.level_for(1.0e7).samples_per_point, 2048);
    assert_eq!(data.num_samples(), (60.0 * RATE) as i64);
}

#[test]
fn peaks_track_amplitude_and_time() {
    let mut clip = silence(20.0, RATE);
    add_tone_between(&mut clip, 440.0, 0.5, 10.0, 20.0);
    let data = build(&clip);

    for level in data.levels() {
        let at =
            |seconds: f64| level.points[(seconds * RATE / level.samples_per_point as f64) as usize];
        assert!(at(5.0).max.abs() <= 1.0e-6);
        assert!((at(15.0).max - 0.5).abs() <= 0.01);
        assert!((at(15.0).min + 0.5).abs() <= 0.01);
    }
}

#[test]
fn tones_land_on_their_frequency_in_order() {
    let found: Vec<f64> = [60.0, 1000.0, 10_000.0]
        .iter()
        .map(|&hz| centroid_hz(&middle(&build(&tone(hz, 0.5, 5.0)))))
        .collect();
    assert!(found[0] < found[1] && found[1] < found[2], "{found:?}");
    for (hz, got) in [60.0, 1000.0, 10_000.0].iter().zip(&found) {
        assert!(octaves_between(*hz, *got) < 0.5, "{hz} Hz read as {got}");
    }
}

#[test]
fn loudness_is_the_rms_and_silence_is_silent() {
    let loud = middle(&build(&tone(1000.0, 0.5, 5.0)));
    assert!(
        (loud.loudness - 0.5 / 2f32.sqrt()).abs() < 0.02,
        "{}",
        loud.loudness
    );
    let quiet = middle(&build(&tone(1000.0, 0.05, 5.0)));
    assert!((quiet.loudness - 0.05 / 2f32.sqrt()).abs() < 0.003);
    assert_eq!(middle(&build(&silence(5.0, RATE))).loudness, 0.0);
}

#[test]
fn noise_spreads_wider_than_a_tone() {
    let tone_spread = middle(&build(&tone(1000.0, 0.5, 5.0))).spread;
    let noise_clip = AudioClip::new(noise(3, (5.0 * RATE) as usize), RATE);
    let noise_spread = middle(&build(&noise_clip)).spread;
    assert!(tone_spread * SPREAD_MAX_OCTAVES < 0.4, "tone {tone_spread}");
    assert!(
        noise_spread > tone_spread * 3.0,
        "{noise_spread} vs {tone_spread}"
    );
}

#[test]
fn a_chord_has_a_centroid_between_its_notes_and_a_wide_spread() {
    let mut clip = tone(100.0, 0.3, 5.0);
    add_tone(&mut clip, 4000.0, 0.3);
    let p = middle(&build(&clip));
    let hz = centroid_hz(&p);
    assert!(hz > 100.0 && hz < 4000.0, "{hz}");
    assert!(p.spread * SPREAD_MAX_OCTAVES > 1.5, "{}", p.spread);
}

#[test]
fn overview_points_equal_merged_fine_points() {
    let data = build(&mix_fixture_clip(MixStyle::Full, 40.0));
    let (fine, coarse) = (&data.levels()[0], &data.levels()[1]);
    for (i, merged) in coarse
        .points
        .iter()
        .enumerate()
        .take(coarse.points.len() - 1)
    {
        let again = WaveformPoint::merge(&fine.points[i * 4..i * 4 + 4]);
        assert_eq!(*merged, again);
    }
    // Merging is associative: 16 fine points equal 4 merged groups of 4.
    let direct = WaveformPoint::merge(&fine.points[160..176]);
    let grouped = WaveformPoint::merge(&coarse.points[40..44]);
    assert!((direct.loudness - grouped.loudness).abs() < 1e-5);
    assert!((direct.centroid - grouped.centroid).abs() < 1e-4);
    assert!((direct.spread - grouped.spread).abs() < 1e-3);
}

#[test]
fn merging_a_silent_point_does_not_move_the_colour() {
    let loud = WaveformPoint {
        min: -0.5,
        max: 0.5,
        loudness: 0.4,
        centroid: 0.7,
        spread: 0.1,
    };
    let merged = WaveformPoint::merge(&[loud, WaveformPoint::default(), WaveformPoint::default()]);
    assert!((merged.centroid - 0.7).abs() < 1e-5);
    assert!((merged.spread - 0.1).abs() < 1e-4);
    assert!(merged.loudness < loud.loudness);
    assert_eq!(
        WaveformPoint::merge(&[WaveformPoint::default()]).centroid,
        0.0
    );
}

#[test]
fn merging_two_distant_tones_widens_the_spread() {
    let low = WaveformPoint {
        min: -0.5,
        max: 0.5,
        loudness: 0.3,
        centroid: 0.2,
        spread: 0.02,
    };
    let high = WaveformPoint {
        centroid: 0.8,
        ..low
    };
    let merged = WaveformPoint::merge(&[low, high]);
    assert!((merged.centroid - 0.5).abs() < 1e-5);
    let octaves = merged.spread * SPREAD_MAX_OCTAVES;
    assert!((octaves - 0.3 * CENTROID_OCTAVES).abs() < 0.2, "{octaves}");
}

fn mix_fixture_clip(style: MixStyle, seconds: f64) -> AudioClip {
    AudioClip::new(mix_fixture(style, RATE, seconds), RATE)
}

#[test]
fn json_round_trip_keeps_shape_and_spectrum() {
    let data = build(&mix_fixture_clip(MixStyle::Full, 6.0));
    let copy = WaveformData::from_json(&data.to_json()).expect("round trip");
    assert_eq!(copy.levels().len(), data.levels().len());
    assert_eq!(copy.sample_rate(), RATE);
    assert!(copy.has_spectrum());

    let a = &data.levels()[0].points;
    let b = &copy.levels()[0].points;
    assert_eq!(a.len(), b.len());
    for i in (0..a.len()).step_by(97) {
        assert!((a[i].max - b[i].max).abs() <= 1.0 / 127.0, "max {i}");
        assert!((a[i].min - b[i].min).abs() <= 1.0 / 127.0, "min {i}");
        assert!(
            (a[i].loudness - b[i].loudness).abs() <= 0.01,
            "loudness {i}"
        );
        assert!(
            (a[i].centroid - b[i].centroid).abs() <= 1.0 / 255.0,
            "centroid {i}"
        );
        assert!(
            (a[i].spread - b[i].spread).abs() <= 1.0 / 255.0,
            "spread {i}"
        );
    }
    assert!(WaveformData::from_json("{\"version\":2,\"points\":\"%%%\"}").is_none());
    assert!(WaveformData::from_json("{}").is_none());
}

#[test]
fn format_one_payloads_are_missing_but_give_a_silhouette() {
    let v1 = legacy_reference::to_v1_json(&mix_fixture_clip(MixStyle::Full, 3.0));
    assert!(
        WaveformData::from_json(&v1).is_none(),
        "never misread as v2"
    );
    let silhouette = WaveformData::silhouette_from_v1_json(&v1).expect("v1 silhouette");
    assert!(silhouette.is_valid() && !silhouette.has_spectrum());
    assert!(silhouette.overview().points.iter().any(|p| p.max > 0.1));
    let v2 = build(&tone(500.0, 0.5, 1.0)).to_json();
    assert!(WaveformData::silhouette_from_v1_json(&v2).is_none());
    assert!(WaveformData::silhouette_from_v1_json("{\"version\":1,\"points\":\"AAAA\"}").is_none());
}

#[test]
fn analyzer_version_changes_so_old_entries_miss() {
    assert_eq!(WaveformAnalyzer::default().version(), 2);
    assert_eq!(crate::WAVEFORM_FORMAT_VERSION, 2);
}

#[test]
fn empty_clip_and_analyzer() {
    let empty = build(&AudioClip::new(Vec::new(), RATE));
    assert!(!empty.is_valid());
    assert!(empty.level_for(100.0).points.is_empty());
    let parsed = WaveformData::from_json(&empty.to_json()).expect("empty payload parses");
    assert!(!parsed.is_valid());

    let payload = WaveformAnalyzer::default()
        .analyze(&tone(200.0, 0.3, 2.0), &NullAnalysisContext)
        .expect("not cancelled");
    let data = WaveformData::from_json(&payload).expect("payload parses");
    assert!((data.overview().points[0].max - 0.3).abs() <= 0.02);
}

#[test]
fn short_clips_and_cancellation() {
    let short = build(&AudioClip::new(vec![0.25; 100], RATE));
    assert_eq!(short.levels()[0].points.len(), 1);
    struct Cancelled;
    impl AnalysisContext for Cancelled {
        fn is_cancelled(&self) -> bool {
            true
        }
        fn set_progress(&self, _: f32) {}
    }
    assert!(WaveformBuilder::default()
        .build(&tone(440.0, 0.5, 2.0), &Cancelled)
        .is_none());
}

#[test]
fn fixtures_are_deterministic_and_bounded() {
    for style in [
        MixStyle::Full,
        MixStyle::HeavyBass,
        MixStyle::VocalLed,
        MixStyle::Sparse,
    ] {
        let a = mix_fixture_clip(style, 8.0);
        assert_eq!(a, mix_fixture_clip(style, 8.0));
        let peak = a.samples.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.05 && peak < 1.0, "{style:?} peak {peak}");
    }
}
