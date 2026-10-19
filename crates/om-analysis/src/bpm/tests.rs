//! Tempo, phase and downbeat on synthetic click tracks,
//! plus grid model edits, the service-backed case (`BpmAnalyzerSlow`) and the analysed grid
//! driving the engine's sync and quantize.

use super::*;
use crate::test_signals::{add_noise, click_track, silence, RATE};
use crate::{
    make_engine_grid, AnalysedBeatGrid, AnalysisJob, AnalysisService, AnalyzerRegistry,
    BeatGridModel, JobStatus, MemoryClipSource, NullAnalysisContext,
};
use om_engine::grid::{BeatGrid, FixedTempoGrid};
use om_engine::quantizer::snap_to_nearest;
use om_engine::sync::SyncDeckState;
use om_engine::DjEngine;
use std::sync::Arc;
use std::time::Duration;

const BEAT_TOLERANCE_SECONDS: f64 = 0.003;

fn estimate(clip: &AudioClip) -> Option<BpmResult> {
    estimate_with(clip, BpmOptions::default())
}

fn estimate_with(clip: &AudioClip, options: BpmOptions) -> Option<BpmResult> {
    BpmEstimator::new(options).estimate(clip, &NullAnalysisContext)
}

/// Distance (seconds) from `seconds` to the nearest grid beat.
fn grid_error(grid: &BeatGridModel, seconds: f64) -> f64 {
    let beat = grid.beat_at_frame(seconds * grid.sample_rate());
    let nearest = grid.frame_at_beat(beat.round()) / grid.sample_rate();
    (nearest - seconds).abs()
}

fn bar_position(grid: &BeatGridModel, seconds: f64) -> f64 {
    let beat = grid.beat_at_frame(seconds * grid.sample_rate()).round();
    let bar = f64::from(grid.beats_per_bar());
    beat - bar * (beat / bar).floor()
}

fn assert_near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1.0e-9 * expected.abs().max(1.0),
        "expected {expected}, got {actual}"
    );
}

fn check_click_track_tempo_and_phase(bpm: f64) {
    let first = 0.37;
    let result =
        estimate(&click_track(bpm, 30.0, first, -1, 4, RATE)).expect("click track has a pulse");

    assert!(
        (result.bpm - bpm).abs() < 0.05,
        "bpm {} vs {bpm}",
        result.bpm
    );
    assert!(result.confidence > 0.5, "confidence {}", result.confidence);
    assert!(result.grid.is_valid());
    let step = 60.0 / bpm * 7.0;
    let mut t = first;
    while t < 30.0 {
        let error = grid_error(&result.grid, t);
        assert!(error < BEAT_TOLERANCE_SECONDS, "t={t} error={error}");
        t += step;
    }
}

macro_rules! click_track_case {
    ($name:ident, $bpm:expr) => {
        #[test]
        fn $name() {
            check_click_track_tempo_and_phase($bpm);
        }
    };
}

click_track_case!(click_track_tempo_and_phase_90_5, 90.5);
click_track_case!(click_track_tempo_and_phase_120, 120.0);
click_track_case!(click_track_tempo_and_phase_128, 128.0);
click_track_case!(click_track_tempo_and_phase_140, 140.0);
click_track_case!(click_track_tempo_and_phase_174, 174.0);

#[test]
fn finds_accented_downbeat() {
    let bpm = 124.0;
    let first = 0.2;
    let result =
        estimate(&click_track(bpm, 30.0, first, 2, 4, RATE)).expect("click track has a pulse");

    let downbeat = first + 2.0 * 60.0 / bpm;
    assert_eq!(bar_position(&result.grid, downbeat), 0.0);
    assert_eq!(bar_position(&result.grid, downbeat + 60.0 / bpm), 1.0);
    let first_grid_downbeat = result.grid.frame_at_beat(0.0) / RATE;
    assert!(first_grid_downbeat >= 0.0);
    assert!(first_grid_downbeat < 4.0 * 60.0 / bpm);
}

#[test]
fn tolerates_noise() {
    let mut clip = click_track(126.0, 30.0, 0.5, -1, 4, RATE);
    add_noise(&mut clip, 0.05, 1234);
    let result = estimate(&clip).expect("noisy click track has a pulse");
    assert!((result.bpm - 126.0).abs() < 0.05, "bpm {}", result.bpm);
}

#[test]
fn folds_tempo_into_range() {
    let fast = estimate(&click_track(200.0, 30.0, 0.1, -1, 4, RATE))
        .expect("fast click track has a pulse");
    assert!((fast.bpm - 100.0).abs() < 0.05, "bpm {}", fast.bpm);

    let options = BpmOptions {
        min_bpm: 60.0,
        max_bpm: 120.0,
        beats_per_bar: 4,
    };
    let slow = estimate_with(&click_track(172.0, 30.0, 0.1, -1, 4, RATE), options)
        .expect("click track has a pulse");
    assert!((slow.bpm - 86.0).abs() < 0.05, "bpm {}", slow.bpm);
}

#[test]
fn no_pulse_or_too_short_gives_nothing() {
    assert!(estimate(&silence(10.0, RATE)).is_none());
    assert!(estimate(&click_track(120.0, 2.0, 0.0, -1, 4, RATE)).is_none());
}

#[test]
fn payload_round_trips_and_silence_is_valid() {
    let analyzer = BpmAnalyzer::default();
    let payload = analyzer
        .analyze(
            &click_track(128.0, 20.0, 0.1, -1, 4, RATE),
            &NullAnalysisContext,
        )
        .expect("click track has a pulse");
    let parsed = BpmResult::from_json(&payload).expect("payload parses");
    assert!((parsed.bpm - 128.0).abs() < 0.05, "bpm {}", parsed.bpm);
    assert!(parsed.grid.is_valid());

    let quiet_payload = analyzer
        .analyze(&silence(10.0, RATE), &NullAnalysisContext)
        .expect("silence is a valid result");
    let quiet = BpmResult::from_json(&quiet_payload).expect("payload parses");
    assert_eq!(quiet.bpm, 0.0);
    assert!(!quiet.grid.is_valid());
    assert!(BpmResult::from_json("not json").is_none());
}

#[test]
fn constant_grid_maps_beats_and_frames() {
    let grid = BeatGridModel::constant(120.0, RATE, 1000.0, 4);
    assert!(grid.is_valid());
    assert!(!grid.is_manual());
    assert_near(grid.beat_at_frame(1000.0 + RATE), 2.0);
    assert_near(grid.frame_at_beat(-1.0), 1000.0 - RATE * 0.5);

    let fixed = FixedTempoGrid::new(120.0, RATE, 1000);
    let adapted = AnalysedBeatGrid::new(grid.clone());
    for f in [-5000i64, 0, 12_345, 999_999] {
        assert!((adapted.beat_at_frame(f) - fixed.beat_at_frame(f)).abs() < 1.0e-9);
    }
    assert_eq!(adapted.frame_at_beat(17.25), fixed.frame_at_beat(17.25));
    assert!(make_engine_grid(&BeatGridModel::default()).is_none());
}

#[test]
fn manual_corrections() {
    let mut grid = BeatGridModel::constant(120.0, RATE, 0.0, 4);
    grid.shift(441.0);
    assert!(grid.is_manual());
    assert_near(grid.frame_at_beat(0.0), 441.0);

    grid.set_downbeat_near(441.0 + 3.1 * RATE * 0.5); // beat 3 becomes a downbeat
    assert_near(
        (grid.beat_at_frame(441.0 + 3.0 * RATE * 0.5)).rem_euclid(4.0),
        0.0,
    );

    grid.set_bpm(125.0);
    assert_near(grid.bpm(), 125.0);
    grid.set_bpm(-3.0);
    assert_near(grid.bpm(), 125.0);
}

#[test]
fn tempo_change_keeps_beats_continuous() {
    let mut grid = BeatGridModel::constant(120.0, RATE, 0.0, 4);
    grid.insert_tempo_change(10.0 * RATE, 60.0); // beat 20 at 10 s, then one beat per second
    assert_near(grid.beat_at_frame(10.0 * RATE), 20.0);
    assert_near(grid.beat_at_frame(12.0 * RATE), 22.0);
    assert_near(grid.frame_at_beat(22.0), 12.0 * RATE);
    assert_near(grid.frame_at_beat(10.0), 5.0 * RATE);
    assert_near(grid.bpm_at_frame(11.0 * RATE), 60.0);
    assert_eq!(grid.markers().len(), 2);
}

#[test]
fn json_round_trip_and_validation() {
    let mut grid = BeatGridModel::constant(128.0, 48_000.0, 512.0, 3);
    grid.insert_tempo_change(480_000.0, 130.0);
    let copy = BeatGridModel::from_json(&grid.to_json()).expect("round trip");
    assert_eq!(copy.beats_per_bar(), 3);
    assert!(copy.is_manual());
    assert_near(copy.frame_at_beat(100.0), grid.frame_at_beat(100.0));

    let parse = |text: &str| {
        BeatGridModel::from_json(&serde_json::from_str::<Value>(text).expect("valid json"))
    };
    assert!(parse(r#"{"version":1,"sampleRate":44100,"markers":[]}"#).is_none());
    assert!(
        parse(r#"{"version":1,"sampleRate":44100,"markers":[{"frame":0,"beat":0,"bpm":-1}]}"#)
            .is_none()
    );
    assert!(parse(r#"{"version":9}"#).is_none());
}

/// Runs a clip through the job service with the `bpm` analyser, as the library does.
fn analyse_through_service(clip: AudioClip) -> Arc<AnalysisJob> {
    let registry = Arc::new(AnalyzerRegistry::new());
    registry
        .add(Arc::new(BpmAnalyzer::default()))
        .expect("bpm registers");
    let service = AnalysisService::new(registry, None, 1);
    let source = Box::new(MemoryClipSource::new(clip, "clicks"));
    let job = service.submit(Some(source), vec!["bpm".to_owned()], None);
    assert!(job.wait_until_finished(Duration::from_secs(60)));
    job
}

#[test]
fn bpm_analyzer_slow_runs_through_service_and_feeds_engine_grid() {
    let job = analyse_through_service(click_track(128.0, 20.0, 0.0, -1, 4, RATE));
    assert_eq!(job.status(), JobStatus::Done);

    let result = BpmResult::from_json(&job.results()["bpm"]).expect("payload parses");
    let grid = make_engine_grid(&result.grid).expect("grid is valid");
    let beat_frames = (RATE * 60.0 / 128.0) as i64;
    let snapped = snap_to_nearest(Some(grid.as_ref()), 10 * beat_frames + 300, 1.0);
    assert!((snapped - 10 * beat_frames).abs() as f64 <= RATE * 0.01);
}

#[test]
fn analysed_grid_drives_sync_and_quantize_through_the_engine() {
    let job = analyse_through_service(click_track(128.0, 20.0, 0.0, -1, 4, RATE));
    let result = BpmResult::from_json(&job.results()["bpm"]).expect("payload parses");

    let (engine, _audio) = DjEngine::new(2);
    engine.set_grid(0, make_engine_grid(&result.grid));
    engine.set_grid(1, Some(Arc::new(FixedTempoGrid::new(120.0, RATE, 0))));
    let sync = engine.sync();
    sync.set_leader(Some(0));
    sync.set_enabled(1, true);

    // Follower at 120 BPM against the analysed 128 BPM leader: it speeds up by the ratio.
    let playing = SyncDeckState {
        position: 0,
        rate: 1.0,
        playing: true,
    };
    let rate = sync.step(&[playing, playing])[1].expect("follower syncs to the analysed leader");
    assert!((rate - 128.0 / 120.0).abs() < 0.01, "rate {rate}");

    // Quantize on the analysed grid: a position a little past beat 10 snaps back onto it.
    let grid = engine
        .deck_control(0)
        .grid()
        .expect("deck 0 carries the analysed grid");
    let beat_frames = (RATE * 60.0 / 128.0).round() as i64;
    let snapped = snap_to_nearest(Some(grid.as_ref()), 10 * beat_frames + 300, 1.0);
    assert!((snapped - 10 * beat_frames).abs() as f64 <= RATE * 0.01);
}
