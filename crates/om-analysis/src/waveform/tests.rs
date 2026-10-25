//! Multi-resolution levels, peaks, band points, the pyramid merge, serialisation, the old-cache
//! paths and the cost limits.

use super::*;
use crate::test_signals::{add_tone, add_tone_between, silence, RATE};
use crate::NullAnalysisContext;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use om_testing::mix::{mix_fixture, MixStyle};

mod bands;
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

/// Mean loudness over the middle half of the base level (skips the edges).
fn middle_loudness(data: &WaveformData) -> f32 {
    let points = &data.levels()[0].points;
    let (from, to) = (points.len() / 4, points.len() * 3 / 4);
    points[from..to].iter().map(|p| p.loudness).sum::<f32>() / (to - from) as f32
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
fn loudness_is_the_rms_and_silence_is_silent() {
    let loud = middle_loudness(&build(&tone(1000.0, 0.5, 5.0)));
    assert!((loud - 0.5 / 2f32.sqrt()).abs() < 0.02, "{loud}");
    let quiet = middle_loudness(&build(&tone(1000.0, 0.05, 5.0)));
    assert!((quiet - 0.05 / 2f32.sqrt()).abs() < 0.003);
    assert_eq!(middle_loudness(&build(&silence(5.0, RATE))), 0.0);
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
    assert_eq!(
        (direct.low, direct.mid, direct.high),
        (grouped.low, grouped.mid, grouped.high)
    );
}

#[test]
fn merging_peak_holds_the_bands_and_keeps_the_rms() {
    let a = WaveformPoint {
        min: -0.5,
        max: 0.4,
        loudness: 0.3,
        low: 0.5,
        mid: 0.1,
        high: 0.0,
    };
    let b = WaveformPoint {
        min: -0.2,
        max: 0.6,
        loudness: 0.1,
        low: 0.0,
        mid: 0.2,
        high: 0.6,
    };
    let merged = WaveformPoint::merge(&[a, b, WaveformPoint::default()]);
    assert_eq!((merged.min, merged.max), (-0.5, 0.6));
    assert_eq!((merged.low, merged.mid, merged.high), (0.5, 0.2, 0.6));
    let rms = ((0.09f32 + 0.01) / 3.0).sqrt();
    assert!((merged.loudness - rms).abs() < 1e-6);
    assert_eq!(WaveformPoint::merge(&[]), WaveformPoint::default());
}

fn mix_fixture_clip(style: MixStyle, seconds: f64) -> AudioClip {
    AudioClip::new(mix_fixture(style, RATE, seconds), RATE)
}

#[test]
fn json_round_trip_keeps_shape_and_bands() {
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
        for (name, x, y) in [
            ("low", a[i].low, b[i].low),
            ("mid", a[i].mid, b[i].mid),
            ("high", a[i].high, b[i].high),
        ] {
            // The level quantiser is a square root: the step grows with the value.
            assert!((x - y).abs() <= 2.0 / 255.0 * x.sqrt() + 1e-4, "{name} {i}");
        }
    }
    assert!(WaveformData::from_json("{\"version\":3,\"points\":\"%%%\"}").is_none());
    assert!(WaveformData::from_json("{}").is_none());
}

#[test]
fn format_one_payloads_are_missing_but_give_a_silhouette() {
    let v1 = legacy_reference::to_v1_json(&mix_fixture_clip(MixStyle::Full, 3.0));
    assert!(
        WaveformData::from_json(&v1).is_none(),
        "never misread as v3"
    );
    let silhouette = WaveformData::silhouette_from_v1_json(&v1).expect("v1 silhouette");
    assert!(silhouette.is_valid() && !silhouette.has_spectrum());
    assert!(silhouette.overview().points.iter().any(|p| p.max > 0.1));
    let v3 = build(&tone(500.0, 0.5, 1.0)).to_json();
    assert!(WaveformData::silhouette_from_v1_json(&v3).is_none());
    assert!(WaveformData::silhouette_from_v1_json("{\"version\":1,\"points\":\"AAAA\"}").is_none());
}

#[test]
fn analyzer_version_changes_so_old_entries_miss() {
    assert_eq!(WaveformAnalyzer::default().version(), 3);
    assert_eq!(crate::WAVEFORM_FORMAT_VERSION, 3);
}

/// A format 2 payload: `min, max, loudness, centroid, spread, 0` per point, hand built.
fn v2_json(points: &[[u8; 5]]) -> String {
    let bytes: Vec<u8> = points
        .iter()
        .flat_map(|p| [p[0], p[1], p[2], p[3], p[4], 0])
        .collect();
    format!(
        "{{\"version\":2,\"sampleRate\":44100.0,\"numSamples\":{},\"samplesPerPoint\":128,\"points\":\"{}\"}}",
        points.len() * 128,
        STANDARD.encode(bytes)
    )
}

#[test]
fn format_two_payloads_still_decode_through_the_model() {
    // Centroid byte 0..255 spans 25 Hz to 16 kHz on a log axis: 60 Hz is about 0.14, 9 kHz 0.93.
    let bass = [230, 25, 120, 36, 20];
    let hats = [230, 25, 120, 237, 20];
    let v2 = v2_json(&[bass, hats]);
    assert!(
        WaveformData::from_json(&v2).is_none(),
        "not a format 3 payload"
    );
    let data = WaveformData::from_v2_json(&v2).expect("format 2 reads");
    assert!(data.has_spectrum());
    let points = &data.levels()[0].points;
    for p in points {
        let peak = p.min.abs().max(p.max.abs());
        assert!(p.low <= peak + 1e-6 && p.mid <= peak + 1e-6 && p.high <= peak + 1e-6);
    }
    assert!(
        points[0].low > points[0].mid && points[0].low > points[0].high,
        "{:?}",
        points[0]
    );
    assert!(points[1].high > points[1].low, "{:?}", points[1]);
    assert!(WaveformData::from_v2_json(&build(&tone(500.0, 0.5, 1.0)).to_json()).is_none());
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
