//! Multi-resolution levels, peaks, band split and
//! serialisation.

use super::*;
use crate::test_signals::{add_tone, add_tone_between, silence, RATE};
use crate::NullAnalysisContext;

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

/// Mean band levels over the middle of the clip (skips filter settling).
fn mean_bands(data: &WaveformData) -> WaveformPoint {
    let points = &data.levels()[0].points;
    let (from, to) = (points.len() / 4, points.len() * 3 / 4);
    let (mut low, mut mid, mut high) = (0.0f32, 0.0f32, 0.0f32);
    for p in &points[from..to] {
        low += p.low;
        mid += p.mid;
        high += p.high;
    }
    let n = (to - from) as f32;
    WaveformPoint {
        min: 0.0,
        max: 0.0,
        low: low / n,
        mid: mid / n,
        high: high / n,
    }
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
fn splits_bands() {
    let bass = mean_bands(&build(&tone(60.0, 0.5, 5.0)));
    assert!(bass.low > 0.3);
    assert!(bass.mid < bass.low * 0.2);
    assert!(bass.high < bass.low * 0.01);

    let mid = mean_bands(&build(&tone(1000.0, 0.5, 5.0)));
    assert!(mid.mid > 0.3);
    assert!(mid.low < mid.mid * 0.1);
    assert!(mid.high < mid.mid * 0.1);

    let treble = mean_bands(&build(&tone(10_000.0, 0.5, 5.0)));
    assert!(treble.high > 0.3);
    assert!(treble.low < treble.high * 0.01);
}

#[test]
fn json_round_trip_keeps_shape() {
    let data = build(&tone(1000.0, 0.7, 3.0));
    let copy = WaveformData::from_json(&data.to_json()).expect("round trip");
    assert_eq!(copy.levels().len(), data.levels().len());
    assert_eq!(copy.sample_rate(), RATE);

    let a = &data.levels()[0].points;
    let b = &copy.levels()[0].points;
    assert_eq!(a.len(), b.len());
    for i in (0..a.len()).step_by(97) {
        assert!((a[i].max - b[i].max).abs() <= 1.0 / 127.0, "max {i}");
        assert!((a[i].min - b[i].min).abs() <= 1.0 / 127.0, "min {i}");
        assert!((a[i].mid - b[i].mid).abs() <= 0.01, "mid {i}");
    }
    assert!(WaveformData::from_json("{\"version\":1,\"points\":\"%%%\"}").is_none());
    assert!(WaveformData::from_json("{}").is_none());
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
