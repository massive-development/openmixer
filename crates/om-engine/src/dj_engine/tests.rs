//! Engine-level cases that need no device. The device-driven
//! cases live in `crates/om-audio/tests/dj_engine.rs`.

use super::DjEngine;
use crate::callback::AudioCallback;
use crate::mixer::ChannelInput;
use crate::ProcessStage;
use om_core::{ParamId, ParameterRouter, ParameterTarget};
use std::sync::{Arc, Mutex};

const RATE: f64 = 44_100.0;

fn run(audio: &mut impl AudioCallback, channels: usize, frames: usize) -> Vec<Vec<f32>> {
    let mut bufs = vec![vec![-9.0_f32; frames]; channels];
    let mut outs: Vec<&mut [f32]> = bufs.iter_mut().map(Vec::as_mut_slice).collect();
    audio.process(&[], &mut outs);
    bufs
}

#[test]
fn silent_when_nothing_is_loaded() {
    let (_engine, mut audio) = DjEngine::new(2);
    audio.prepare(RATE, 256);
    for buf in run(&mut audio, 4, 256) {
        assert!(buf.iter().all(|&s| s == 0.0));
    }
}

#[test]
fn oversize_blocks_are_chunked() {
    let (_engine, mut audio) = DjEngine::new(2);
    audio.prepare(RATE, 64);
    for buf in run(&mut audio, 2, 300) {
        assert!(
            buf.iter().all(|&s| s == 0.0),
            "300 frames rendered in 64-frame chunks"
        );
    }
}

#[test]
fn unprepared_engine_outputs_silence() {
    let (_engine, mut audio) = DjEngine::new(2);
    for buf in run(&mut audio, 4, 128) {
        assert!(buf.iter().all(|&s| s == 0.0));
    }
}

#[test]
fn channel_count_is_clamped_to_the_mixer() {
    let (engine, _audio) = DjEngine::new(9);
    assert_eq!(engine.num_decks(), 4);
}

/// Records every `prepare` call a probe stage receives.
#[derive(Clone, Default)]
struct PrepareLog(Arc<Mutex<Vec<(f64, usize)>>>);

impl PrepareLog {
    fn last(&self) -> Option<(f64, usize)> {
        self.0.lock().unwrap().last().copied()
    }
}

struct Probe(PrepareLog);

impl ProcessStage for Probe {
    fn prepare(&mut self, sample_rate: f64, max_block_size: usize) {
        self.0
             .0
            .lock()
            .unwrap()
            .push((sample_rate, max_block_size));
    }

    fn process(&mut self, _left: &mut [f32], _right: &mut [f32]) {}
}

#[test]
fn stages_are_prepared_at_the_device_rate_and_block() {
    for rate in [48_000.0, 96_000.0] {
        let (engine, mut audio) = DjEngine::new(2);
        // Queued before prepare: must reach its slot and be prepared with the device rate.
        let queued = PrepareLog::default();
        assert!(engine
            .mixer()
            .set_channel_insert(0, 1, Some(Box::new(Probe(queued.clone())))));
        audio.prepare(rate, 256);
        assert_eq!(
            queued.last(),
            Some((rate, 256)),
            "rate {rate}: queued stage"
        );

        // Queued after prepare: uses the rate and block the engine was prepared with.
        let late = PrepareLog::default();
        assert!(engine
            .mixer()
            .set_channel_insert(1, 1, Some(Box::new(Probe(late.clone())))));
        assert_eq!(late.last(), Some((rate, 256)), "rate {rate}: late stage");
    }
}

#[test]
fn echo_repeat_lands_on_the_beat_grid_at_48_khz() {
    const RATE: f64 = 48_000.0;
    const FRAMES: usize = 256;
    const BLOCKS: usize = 120;
    let (engine, mut audio) = DjEngine::new(2);
    audio.prepare(RATE, FRAMES);
    let rack = engine.fx().channel_rack(0).expect("channel 0 rack");
    let echo = rack.find_unit("echo").expect("echo in the default rack");
    rack.set_tempo(120.0);
    for (id, value) in [
        (format!("ch0.fx{echo}.enable"), 1.0),
        (format!("ch0.fx{echo}.mix"), 1.0),
        (format!("ch0.fx{echo}.feedback"), 0.0),
        (format!("ch0.fx{echo}.time"), 1.0),
    ] {
        assert!(engine.fx().set_parameter(&id, value), "{id}");
    }

    let total = FRAMES * BLOCKS;
    let mut impulse = vec![0.0_f32; total];
    impulse[0] = 1.0;
    let (mut ml, mut mr) = (vec![0.0_f32; total], vec![0.0_f32; total]);
    let (mut cl, mut cr) = (vec![0.0_f32; total], vec![0.0_f32; total]);
    let inputs = [ChannelInput {
        left: Some(&impulse),
        right: Some(&impulse),
    }];
    audio
        .mixer
        .process(&inputs, &mut ml, &mut mr, &mut cl, &mut cr);

    // The dry impulse sits at frame 0; skip past it and look for the repeat.
    let peak = ml
        .iter()
        .enumerate()
        .skip(1000)
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .map(|(i, _)| i)
        .unwrap();
    // One beat at 120 BPM is 0.5 s: 24000 frames at 48 kHz (22050 would mean 44.1 kHz). The
    // channel EQ and filter add a few milliseconds, so allow 500 frames; the rates differ by 1950.
    assert!(
        (peak as i64 - 24_000).abs() <= 500,
        "echo repeat at frame {peak}, expected about 24000"
    );
}

/// Ids the engine applies, with a value each. Driven once through the string façade and once
/// through `ParameterRouter::dispatch` on a parsed `ParamId`; the engine state must match.
const SAME_EFFECT: &[(&str, f64)] = &[
    ("deck.0.rate", 1.05),
    ("deck.0.play", 1.0),
    ("deck.0.slip", 1.0),
    ("deck.0.quantize", 0.0),
    ("sync.leader", 1.0),
    ("deck.0.sync.master", 1.0),
    ("sync.1.sync", 1.0),
    ("mixer.ch1.fader", 0.25),
    ("mixer.crossfader", 0.9),
    ("fx.ch1.fx2.enable", 1.0),
    ("fx.ch1.fx2.mix", 0.4),
];

/// Ids the grammar rejects: both paths must report `false` and count them as unknown.
const REJECTED: &[&str] = &[
    "deck.0.nope",
    "deck.9.play",
    "deck.0.loop.quantized.0",
    "mixer.ch9.fader",
    "fx.ch0000.fx0.mix",
    "sync.x.sync",
];

/// Ids that parse, but the two-deck engine does not have the target: `false` on both paths.
const ENGINE_REJECTS: &[&str] = &[
    "deck.3.play",
    "mixer.ch3.fader",
    "fx.ch3.fx0.mix",
    "deck.0.loop.quantized.2",
];

#[derive(Debug, PartialEq)]
struct Observed {
    rate: f64,
    playing: bool,
    fader: f32,
    crossfader: f32,
    fx_mix: f32,
    fx_enabled: bool,
    leader: Option<usize>,
    sync_on: bool,
    slip: bool,
    quantize: bool,
}

fn observe(engine: &DjEngine) -> Observed {
    let unit = engine
        .fx()
        .channel_rack(1)
        .and_then(|rack| rack.unit(2))
        .expect("channel 1, unit 2");
    Observed {
        rate: engine.deck(0).rate(),
        playing: engine.deck(0).is_playing(),
        fader: engine.mixer().fader(1),
        crossfader: engine.mixer().crossfader(),
        fx_mix: unit.mix(),
        fx_enabled: unit.is_enabled(),
        leader: engine.sync().leader(),
        sync_on: engine.sync().is_enabled(1),
        slip: engine.deck_control(0).is_slipping(),
        quantize: engine.deck_control(0).is_quantize(),
    }
}

#[test]
fn typed_dispatch_has_the_same_effect_as_the_string_path() {
    let (by_string, _string_audio) = DjEngine::new(2);
    let (by_typed, _typed_audio) = DjEngine::new(2);
    let string_router = ParameterRouter::new();
    let typed_router = ParameterRouter::new();
    by_string.register(&string_router);
    by_typed.register(&typed_router);

    for &(id, value) in SAME_EFFECT {
        let parsed: ParamId = id.parse().expect(id);
        assert!(string_router.set_parameter(id, value), "{id} via string");
        assert!(typed_router.dispatch(&parsed, value), "{id} via dispatch");
    }
    assert_eq!(observe(&by_string), observe(&by_typed));
    assert!(observe(&by_typed).playing && observe(&by_typed).slip);
    assert_eq!(observe(&by_typed).leader, Some(0));
    assert_eq!(string_router.unknown_count(), 0);
    assert_eq!(typed_router.unknown_count(), 0);
}

#[test]
fn rejected_ids_are_refused_and_counted_on_both_paths() {
    let (engine, _audio) = DjEngine::new(2);
    let router = ParameterRouter::new();
    engine.register(&router);

    for id in REJECTED {
        assert!(id.parse::<ParamId>().is_err(), "{id} should not parse");
        assert!(!router.set_parameter(id, 1.0), "{id}");
    }
    for id in ENGINE_REJECTS {
        let parsed: ParamId = id.parse().expect(id);
        assert!(!router.dispatch(&parsed, 1.0), "{id} via dispatch");
        assert!(!router.set_parameter(id, 1.0), "{id} via string");
    }
    assert_eq!(
        router.unknown_count(),
        REJECTED.len() + 2 * ENGINE_REJECTS.len()
    );
}

/// A playing deck with a loop, a cue, key-lock, slip and sync, next to a second deck, read two ways:
/// through the getters and through one snapshot taken with the audio side idle. The two must agree.
#[test]
fn snapshot_matches_the_getters_of_a_playing_engine() {
    use om_core::{EngineSnapshot, StateId};
    use om_testing::fs::TempDir;
    use om_testing::signals::stereo_sine;
    use om_testing::wait::{wait_until, DEFAULT_POLL};
    use om_testing::wav::write_float_wav;
    use std::time::Duration;

    let dir = TempDir::new("openmixer_snapshot_test");
    let file = dir.file("sine.wav");
    write_float_wav(&file, 2, 44_100, &stereo_sine(440.0, 0.5, RATE, 44_100));

    let (engine, mut audio) = DjEngine::new(2);
    audio.prepare(RATE, 256);
    engine.deck(0).load(&file).expect("load");
    assert!(wait_until(Duration::from_secs(3), DEFAULT_POLL, || {
        let _ = run(&mut audio, 4, 256);
        engine.deck(0).buffered_frames() > 4096
    }));
    engine.deck(0).play();
    for _ in 0..8 {
        let _ = run(&mut audio, 4, 256);
    }
    engine.deck(0).set_loop(crate::quantizer::Loop {
        start: 1_000,
        end: 9_000,
    });
    engine.deck(0).set_hotcue(2, 4_321);
    engine.deck(0).set_keylock(true);
    engine.deck_control(0).set_slip(true);
    engine.sync().set_enabled(1, true);
    engine.sync().set_leader(Some(0));
    engine.mixer().set_fader(1, 0.4);
    engine.mixer().set_pfl(1, true);
    engine.mixer().set_crossfader(-0.25);
    engine.mixer().set_master_gain(0.7);
    engine.update_control(0.02);

    let snap: EngineSnapshot = engine.snapshot();
    assert_eq!(snap.ticks, 1, "one control tick so far");
    assert_eq!(snap.built, 1, "first snapshot of this engine");
    assert_eq!(snap.decks.len(), 2);
    let deck = engine.deck(0);
    let d = &snap.decks[0];
    assert!(d.playing && d.loaded && d.keylock && d.looping && d.slip);
    assert!(d.sync_master && !snap.decks[1].sync_master);
    assert!(snap.decks[1].sync);
    assert_eq!(d.position_frames, deck.position_frames());
    assert_eq!(d.length_frames, deck.length_frames());
    assert_eq!(d.file_sample_rate, deck.file_sample_rate());
    assert_eq!(d.rate, deck.rate());
    assert_eq!((d.loop_start, d.loop_end), (1_000, 9_000));
    assert_eq!(d.hotcues[2], deck.hotcue(2));
    assert_eq!(d.hotcues[0], None);
    assert_eq!(snap.decks[0], engine.deck_snapshot(0));
    assert_eq!(snap.mixer.channels[1].fader, engine.mixer().fader(1));
    assert!(snap.mixer.channels[1].pfl);
    assert_eq!(snap.mixer.crossfader, engine.mixer().crossfader());
    assert_eq!(snap.mixer.master_gain, engine.mixer().master_gain());
    assert_eq!(
        snap.mixer.channels[0].peak_left,
        engine.mixer().channel_peak_left(0)
    );

    // The id path reads the same numbers as the typed fields.
    let read = |id: &str| snap.read(&id.parse::<StateId>().expect(id));
    assert_eq!(read("deck.0.loop"), Some(1.0));
    assert_eq!(read("deck.0.cue"), Some(0.0));
    assert_eq!(read("deck.1.sync.master"), Some(0.0));
    assert_eq!(read("deck.0.hotcue2.set"), Some(1.0));
    assert_eq!(
        read("mixer.ch1.fader"),
        Some(f64::from(engine.mixer().fader(1)))
    );
    assert_eq!(read("deck.9.play"), None);
}
