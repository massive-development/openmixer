//! Stem control surface through the engine: router ids, snapshot reads, the `LoadStems` command and
//! a full-engine render. Signals are rendered to buffers only; nothing is played.

use om_core::{Command, CommandError, ParameterRouter, ParameterTarget, StateId, StemSetRef};
use om_engine::{AudioCallback, DjEngine};
use om_testing::fs::TempDir;
use om_testing::wait::{wait_until, DEFAULT_POLL};
use om_testing::wav::write_float_wav;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const RATE: f64 = 44_100.0;
const BLOCK: usize = 256;
const FRAMES: usize = 40_000;

fn tone(hz: f32, amp: f32, n: usize) -> f32 {
    amp * (std::f32::consts::TAU * hz * n as f32 / RATE as f32).sin()
}

fn write_set(dir: &Path) {
    let hz = [60.0, 220.0, 880.0, 3_000.0];
    let mut mix = vec![0.0_f32; FRAMES * 2];
    for (i, name) in ["drums.wav", "bass.wav", "vocals.wav", "other.wav"]
        .iter()
        .enumerate()
    {
        let samples: Vec<f32> = (0..FRAMES)
            .flat_map(|n| [tone(hz[i], 0.15, n), tone(hz[i], 0.15, n)])
            .collect();
        for (m, s) in mix.iter_mut().zip(&samples) {
            *m += s;
        }
        write_float_wav(&dir.join(name), 2, RATE as u32, &samples);
    }
    write_float_wav(&dir.join("main.wav"), 2, RATE as u32, &mix);
}

fn id(text: &str) -> StateId {
    text.parse().expect("state id")
}

#[test]
fn router_drives_stem_params_and_the_snapshot_reads_them_back() {
    let (engine, _audio) = DjEngine::new(2);
    let router = ParameterRouter::new();
    engine.register(&router);
    assert!(router.set_parameter("stem.1.2.volume", 0.5));
    assert!(router.set_parameter("stem.1.2.mute", 1.0));
    assert!(router.set_parameter("stem.1.3.filter", -0.25));
    assert!(
        router.set_parameter("stem.0.0.volume", 9.0),
        "clamped, not refused"
    );
    assert!(!router.set_parameter("stem.5.0.mute", 1.0), "no such deck");
    assert!(!router.set_parameter("stem.0.4.mute", 1.0), "no such stem");

    let snap = engine.snapshot();
    assert_eq!(snap.read(&id("stem.1.2.volume")), Some(0.5));
    assert_eq!(snap.read(&id("stem.1.2.mute")), Some(1.0));
    assert_eq!(snap.read(&id("stem.1.3.filter")), Some(-0.25));
    assert_eq!(snap.read(&id("stem.0.0.volume")), Some(1.5));
    assert_eq!(snap.read(&id("stem.0.1.volume")), Some(1.0));
    assert_eq!(snap.read(&id("stem.9.0.volume")), None);
    assert!(!snap.stems[1][2].available);
    assert!(snap.stems[1][2].muted);
}

#[test]
fn load_stems_command_attaches_the_set_and_everything_else_is_rejected() {
    let dir = TempDir::new("om_engine_stem_cmd");
    write_set(dir.path());
    let (engine, _audio) = DjEngine::new(2);
    engine.deck(0).load(&dir.file("main.wav")).unwrap();
    let sink = engine.command_sink(None);
    let set = engine.register_stem_set(dir.path());

    assert!(sink.send(Command::LoadStems { deck: 0, set }).is_ok());
    assert!(engine.deck(0).has_stems());
    assert!(engine.snapshot().stems[0].iter().all(|s| s.available));

    assert!(matches!(
        sink.send(Command::LoadStems {
            deck: 0,
            set: StemSetRef(999)
        }),
        Err(CommandError::Rejected(_))
    ));
    assert!(matches!(
        sink.send(Command::LoadStems { deck: 7, set }),
        Err(CommandError::Rejected(_))
    ));
    // Deck 1 has no track: the deck's own error comes back as a rejection.
    assert!(matches!(
        sink.send(Command::LoadStems { deck: 1, set }),
        Err(CommandError::Rejected(m)) if m.contains("no track")
    ));
    let param = Command::param("stem.0.0.mute".parse().unwrap(), 1.0).unwrap();
    assert!(matches!(
        sink.send(param.clone()),
        Err(CommandError::Rejected(_))
    ));

    // With the router as fallback, parameter commands pass through.
    let router = Arc::new(ParameterRouter::new());
    engine.register(&router);
    let sink = engine.command_sink(Some(router));
    assert!(sink.send(param).is_ok());
    assert!(engine.snapshot().stems[0][0].muted);
}

/// Renders paced blocks through the whole engine and returns the master-left energy.
fn run(engine: &DjEngine, audio: &mut om_engine::DjEngineAudio, blocks: usize) -> f32 {
    let mut bufs = [[0.0_f32; BLOCK]; 4];
    let mut energy = 0.0;
    for _ in 0..blocks {
        assert!(wait_until(Duration::from_secs(10), DEFAULT_POLL, || engine
            .deck(0)
            .buffered_frames()
            >= BLOCK * 2));
        let [a, b, c, d] = &mut bufs;
        let mut outs: [&mut [f32]; 4] = [a, b, c, d];
        audio.process(&[], &mut outs);
        assert!(outs[0].iter().all(|v| v.is_finite()));
        energy += outs[0].iter().map(|v| v * v).sum::<f32>();
    }
    energy
}

#[test]
fn engine_renders_stems_and_the_router_mutes_them() {
    let dir = TempDir::new("om_engine_stem_run");
    write_set(dir.path());
    let (engine, mut audio) = DjEngine::new(2);
    audio.prepare(RATE, BLOCK);
    let router = ParameterRouter::new();
    engine.register(&router);
    engine.deck(0).load(&dir.file("main.wav")).unwrap();
    engine.deck(0).load_stems(dir.path()).unwrap();
    engine.mixer().set_fader(0, 1.0);
    // Rendering services the hand-over handshake; pump until the decoder has buffered.
    let mut pump = [[0.0_f32; BLOCK]; 4];
    assert!(wait_until(Duration::from_secs(10), DEFAULT_POLL, || {
        let [a, b, c, d] = &mut pump;
        let mut outs: [&mut [f32]; 4] = [a, b, c, d];
        audio.process(&[], &mut outs);
        engine.deck(0).buffered_frames() >= BLOCK * 4
    }));
    engine.deck(0).play();
    run(&engine, &mut audio, 8);
    let audible = run(&engine, &mut audio, 20);
    assert!(audible > 0.1, "stems play through the engine: {audible}");

    for stem in 0..4 {
        assert!(router.set_parameter(&format!("stem.0.{stem}.mute"), 1.0));
    }
    run(&engine, &mut audio, 20);
    let muted = run(&engine, &mut audio, 10);
    assert!(
        muted < audible * 1e-4,
        "all stems muted: {muted} vs {audible}"
    );

    assert!(router.set_parameter("stem.0.1.mute", 0.0));
    run(&engine, &mut audio, 20);
    let bass_only = run(&engine, &mut audio, 10);
    assert!(
        bass_only > muted * 100.0 && bass_only < audible,
        "bass only: {bass_only}"
    );
}
