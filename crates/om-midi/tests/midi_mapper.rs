//! Map parsing, value modes, trace replay, parameter
//! routing, plus MidiTraceTest and ParameterRouterTest (kept in this file).

mod support;

use om_core::{ParameterRouter, ParameterTarget};
use om_engine::DjEngine;
use om_midi::{
    control_change, note_off, note_on, pitch_bend, program_change, replay_trace, MidiEvent,
    MidiMapper, MidiTrace,
};
use std::sync::Arc;
use support::{assert_near, data_dir, FakeState, RecordingTarget};

struct Fixture {
    m: MidiMapper,
    t: Arc<RecordingTarget>,
}

fn fixture() -> Fixture {
    let t = Arc::new(RecordingTarget::default());
    let m = MidiMapper::new();
    m.set_target(t.clone());
    Fixture { m, t }
}

#[test]
fn rejects_invalid_maps_without_changing_state() {
    let f = fixture();
    assert!(f.m.load_from_json("not json").is_err());
    assert!(f.m.load_from_json("{}").is_err());
    assert!(
        f.m.load_from_json(r#"{"mappings":[{"type":"cc","number":1}]}"#)
            .is_err(),
        "missing target"
    );
    assert!(f
        .m
        .load_from_json(r#"{"mappings":[{"type":"zzz","number":1,"target":"a"}]}"#)
        .is_err());
    assert!(f
        .m
        .load_from_json(r#"{"mappings":[{"type":"cc","number":200,"target":"a"}]}"#)
        .is_err());
    assert!(f
        .m
        .load_from_json(r#"{"mappings":[{"type":"cc","number":1,"target":"a","mode":"x"}]}"#)
        .is_err());
    assert!(f
        .m
        .load_from_json(r#"{"mappings":[{"type":"cc","number":1,"target":"a","min":1,"max":1}]}"#)
        .is_err());
    assert_eq!(f.m.num_mappings(), 0);
    assert!(f
        .m
        .load_from_file(&data_dir().join("missing.json"))
        .is_err());
}

#[test]
fn absolute_cc_scales_inverts_and_respects_channel_filter() {
    let f = fixture();
    f.m.load_from_json(
        r#"{"mappings":[
        {"type":"cc","channel":2,"number":7,"target":"a","min":10,"max":20},
        {"type":"cc","number":8,"target":"b","invert":true}]}"#,
    )
    .unwrap();
    assert_eq!(
        f.m.handle_message(&control_change(1, 7, 127)),
        0,
        "wrong channel"
    );
    assert_eq!(f.m.handle_message(&control_change(2, 7, 127)), 1);
    assert_near(f.t.last("a"), 20.0);
    f.m.handle_message(&control_change(2, 7, 0));
    assert_near(f.t.last("a"), 10.0);
    f.m.handle_message(&control_change(9, 8, 127));
    assert_near(f.t.last("b"), 0.0);
    assert_near(f.m.value("b"), 0.0);
}

#[test]
fn relative_modes_accumulate_and_clamp() {
    let f = fixture();
    f.m.load_from_json(
        r#"{"mappings":[
        {"type":"cc","number":1,"target":"r","mode":"relative","step":0.1,"min":0,"max":1,"initial":0.5},
        {"type":"cc","number":2,"target":"c","mode":"relative2c","step":0.1,"min":0,"max":1,"initial":0.5}]}"#,
    )
    .unwrap();
    for _ in 0..3 {
        f.m.handle_message(&control_change(1, 1, 65));
    }
    assert_near(f.t.last("r"), 0.8);
    for _ in 0..100 {
        f.m.handle_message(&control_change(1, 1, 127));
    }
    assert_near(f.t.last("r"), 1.0);
    f.m.handle_message(&control_change(1, 2, 127)); // -1 in two's complement
    assert_near(f.t.last("c"), 0.4);
    f.m.handle_message(&control_change(1, 2, 2));
    assert_near(f.t.last("c"), 0.6);
}

#[test]
fn button_toggle_and_pitch_bend() {
    let f = fixture();
    f.m.load_from_json(
        r#"{"mappings":[
        {"type":"note","number":60,"target":"btn"},
        {"type":"note","number":61,"target":"tog","mode":"toggle"},
        {"type":"pitchbend","target":"pb","min":-1,"max":1}]}"#,
    )
    .unwrap();
    f.m.handle_message(&note_on(1, 60, 100));
    assert_eq!(f.t.last("btn"), 1.0);
    f.m.handle_message(&note_off(1, 60));
    assert_eq!(f.t.last("btn"), 0.0);
    f.m.handle_message(&note_on(1, 61, 100));
    f.m.handle_message(&note_off(1, 61));
    assert_eq!(f.t.count("tog"), 1, "release ignored");
    assert_eq!(f.t.last("tog"), 1.0);
    f.m.handle_message(&note_on(1, 61, 100));
    assert_eq!(f.t.last("tog"), 0.0);
    f.m.handle_message(&pitch_bend(1, 16383));
    assert_near(f.t.last("pb"), 1.0);
    f.m.handle_message(&pitch_bend(1, 0));
    assert_near(f.t.last("pb"), -1.0);
    assert_eq!(
        f.m.handle_message(&program_change(1, 3)),
        0,
        "unsupported type"
    );
}

#[test]
fn trace_replay_of_a_recorded_stream_through_a_map_file() {
    let f = fixture();
    f.m.load_from_file(&data_dir().join("test_map.json"))
        .unwrap();
    assert_eq!(f.m.num_mappings(), 7);

    let trace = MidiTrace::from_file(&data_dir().join("test_trace.json")).unwrap();
    assert_eq!(trace.events.len(), 12);

    let fired = replay_trace(&trace, &f.m);
    // 2 fader + 3 eq + 2 toggle presses + 2 pitch bend; note-offs on toggle and the
    // channel-6 CC match nothing.
    assert_eq!(fired, 9);
    assert_eq!(f.t.count("mixer.ch0.fader"), 2);
    assert_near(f.t.last("mixer.ch0.fader"), 1.0);
    assert_near(f.t.last("mixer.ch0.eq.low"), 0.5); // +0.5 +0.5 -0.5
    assert_eq!(f.t.count("deck.0.play"), 2);
    assert_eq!(f.t.last_or("deck.0.play", -1.0), 0.0); // toggled on, then off
    assert_near(f.t.last("deck.0.rate"), 0.92);
    assert_eq!(f.t.count("mixer.ch0.pfl"), 0);
}

#[test]
fn midi_trace_json_round_trips() {
    let mut a = MidiTrace {
        name: "rt".into(),
        ..MidiTrace::default()
    };
    a.add(0.0, control_change(3, 11, 99));
    a.add(0.5, note_on(1, 36, 127));
    let b = MidiTrace::from_json(&a.to_json()).unwrap();
    assert_eq!(b.events.len(), 2);
    assert_eq!(b.name, a.name);
    let first = MidiEvent::from_bytes(&b.events[0].bytes).unwrap();
    assert_eq!(first.value, 99);
    assert_near(b.events[1].time, 0.5);
    assert!(
        MidiTrace::from_json(r#"{"events":[{"bytes":"B0 07"}]}"#).is_err(),
        "truncated message"
    );
}

#[test]
fn parameter_router_drives_mixer_and_deck_from_a_replayed_trace() {
    let (engine, _audio) = DjEngine::new(2);
    let router = Arc::new(ParameterRouter::new());
    engine.register(&router);
    let m = MidiMapper::new();
    m.set_target(router.clone());
    m.load_from_file(&data_dir().join("test_map.json")).unwrap();
    let trace = MidiTrace::from_file(&data_dir().join("test_trace.json")).unwrap();
    replay_trace(&trace, &m);

    let mixer = engine.mixer();
    assert!((f64::from(mixer.fader(0)) - 1.0).abs() < 1e-6);
    assert!((f64::from(mixer.eq(0, 0)) - 0.5).abs() < 1e-6);
    assert_near(engine.deck(0).rate(), 0.92);
    assert!(!engine.deck(0).is_playing(), "toggled on then off");
    assert_eq!(router.unknown_count(), 0);

    m.handle_message(&note_on(1, 40, 127));
    assert!(mixer.pfl(0));
    m.handle_message(&note_off(1, 40));
    assert!(!mixer.pfl(0));
    m.handle_message(&control_change(1, 8, 127)); // inverted ch1 fader
    assert!((f64::from(mixer.fader(1)) - 0.0).abs() < 1e-6);

    assert!(!router.set_parameter("mixer.nonsense", 1.0));
    assert!(!router.set_parameter("nobody.home", 1.0));
    assert_eq!(router.unknown_count(), 2);
}

#[test]
fn user_targets_can_be_registered_replaced_and_removed() {
    let a = Arc::new(RecordingTarget::default());
    let b = Arc::new(RecordingTarget::default());
    let router = ParameterRouter::new();
    router.register_target("plug", a.clone()).unwrap();
    router.register_target("plug.delay", b.clone()).unwrap(); // longer prefix wins
    assert!(router.set_parameter("plug.delay.time", 0.3));
    assert_eq!(b.last("time"), 0.3);
    assert!(router.set_parameter("plug.reverb.size", 0.7));
    assert_eq!(a.last("reverb.size"), 0.7);
    router.unregister_target("plug.delay");
    assert!(router.set_parameter("plug.delay.time", 0.9));
    assert_eq!(a.last("delay.time"), 0.9);
    assert!(
        !router.set_parameter("plugx.time", 1.0),
        "prefix must match a whole segment"
    );
}

#[test]
fn toggle_and_relative_start_from_the_engine_value_and_keep_it_across_reloads() {
    let f = fixture();
    let engine = Arc::new(FakeState::default());
    f.m.set_state_reader(engine.clone());
    let map = r#"{"mappings":[
        {"type":"note","number":36,"target":"deck.0.play","mode":"toggle"},
        {"type":"cc","number":21,"target":"mixer.ch0.eq.low","mode":"relative","step":1,"min":-100,"max":100}]}"#;
    f.m.load_from_json(map).unwrap();

    // The deck was started with the mouse: the engine says playing, so the press stops it.
    engine.set("deck.0.play", 1.0);
    f.m.handle_message(&note_on(0, 36, 127));
    assert_eq!(f.t.last("deck.0.play"), 0.0);

    // A relative encoder adds to the engine's value (10), not the mapper's stored 0.
    engine.set("mixer.ch0.eq.low", 10.0);
    f.m.handle_message(&control_change(0, 21, 65));
    assert_eq!(f.t.last("mixer.ch0.eq.low"), 11.0);

    // A reload with the engine unknown keeps the last value for the unchanged target.
    f.m.set_state_reader(Arc::new(FakeState::default()));
    f.m.load_from_json(map).unwrap();
    f.m.handle_message(&control_change(0, 21, 65));
    assert_eq!(f.t.last("mixer.ch0.eq.low"), 12.0);
}

#[test]
fn stem_pad_map_drives_each_stem_of_deck_zero_through_the_router() {
    let (engine, _audio) = DjEngine::new(2);
    let router = Arc::new(ParameterRouter::new());
    engine.register(&router);
    let m = MidiMapper::new();
    m.set_target(router.clone());
    m.load_from_file(&data_dir().join("stem_map.json")).unwrap();
    assert_eq!(m.num_mappings(), 9);

    // Mute toggles: the first press mutes drums, the second un-mutes it.
    m.handle_message(&note_on(1, 44, 127));
    assert!(engine.deck(0).stem_control().is_muted(0));
    m.handle_message(&note_on(1, 44, 127));
    assert!(!engine.deck(0).stem_control().is_muted(0));
    m.handle_message(&note_on(1, 47, 127));
    assert!(engine.deck(0).stem_control().is_muted(3));

    // Volume: 0..127 maps onto 0..1.5 and filter onto -1..1.
    m.handle_message(&control_change(1, 20, 127));
    assert_near(f64::from(engine.deck(0).stem_control().gain(0)), 1.5);
    m.handle_message(&control_change(1, 24, 0));
    assert_near(f64::from(engine.deck(0).stem_control().filter(2)), -1.0);
    assert_eq!(router.unknown_count(), 0);
}

#[test]
fn sampler_pad_map_triggers_pads_with_velocity_and_gates_through_the_router() {
    use om_engine::PadMode;
    let (engine, mut audio) = DjEngine::new(2);
    om_engine::AudioCallback::prepare(&mut audio, 48_000.0, 256);
    let router = Arc::new(ParameterRouter::new());
    engine.register(&router);
    let m = MidiMapper::new();
    m.set_target(router.clone());
    m.load_from_file(&data_dir().join("sampler_map.json"))
        .unwrap();
    assert_eq!(m.num_mappings(), 11);

    let sample =
        Arc::new(om_engine::Sample::from_mono("pad", 48_000.0, &vec![0.5; 48_000]).unwrap());
    engine.sampler().assign(0, Arc::clone(&sample)).unwrap();
    engine.sampler().assign(3, sample).unwrap();
    engine.sampler().set_mode(3, PadMode::Gate);

    let mut out = [vec![0.0_f32; 256], vec![0.0; 256]];
    let mut render = |audio: &mut om_engine::DjEngineAudio| {
        let [l, r] = &mut out;
        let mut bufs: [&mut [f32]; 2] = [l, r];
        om_engine::AudioCallback::process(audio, &[], &mut bufs);
        bufs[0][255]
    };
    // Note on at half velocity triggers pad 3; its release (note off) fades the gate pad out.
    m.handle_message(&note_on(10, 39, 64));
    render(&mut audio);
    let level = render(&mut audio);
    assert_near(f64::from(level), 0.5 * 64.0 / 127.0);
    m.handle_message(&note_off(10, 39));
    render(&mut audio);
    render(&mut audio);
    assert_eq!(render(&mut audio), 0.0);
    assert_eq!(engine.sampler().snapshot().voices_active, 0);

    // Volume and pitch CCs.
    m.handle_message(&control_change(10, 20, 127));
    m.handle_message(&control_change(10, 21, 127));
    let pad = engine.sampler().snapshot().pads[0];
    assert_near(f64::from(pad.gain), 1.5);
    assert_near(f64::from(pad.pitch), 12.0);
    assert_eq!(router.unknown_count(), 0);
}

/// Every id in the shipped sampler map must be understood by the engine: mapping targets parse as
/// `ParamId`s and are taken by the router; output sources parse as `StateId`s and read a value.
#[test]
fn every_id_in_sampler_map_json_is_valid() {
    use om_core::{ParamId, StateId};
    let text = std::fs::read_to_string(data_dir().join("sampler_map.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();

    let (engine, _audio) = DjEngine::new(2);
    let router = Arc::new(ParameterRouter::new());
    engine.register(&router);
    let snapshot = engine.snapshot();

    let mappings = json["mappings"].as_array().unwrap();
    assert_eq!(mappings.len(), 11);
    let mut triggers = std::collections::BTreeSet::new();
    for m in mappings {
        let target = m["target"].as_str().unwrap();
        let id: ParamId = target.parse().unwrap_or_else(|e| panic!("{target}: {e}"));
        assert_eq!(id.to_string(), target, "canonical form");
        assert!(router.set_parameter(target, 0.5), "{target} rejected");
        if let ParamId::Sampler { pad, p } = id {
            assert!(pad < 8);
            if p == om_core::param_id::SamplerParam::Trigger {
                triggers.insert(pad);
            }
        } else {
            panic!("{target} is not a sampler id");
        }
    }
    assert_eq!(triggers.len(), 8, "all eight pads have a trigger mapping");

    let outputs = json["outputs"].as_array().unwrap();
    assert_eq!(outputs.len(), 16);
    let mut seen = std::collections::BTreeSet::new();
    for o in outputs {
        let source = o["source"].as_str().unwrap();
        let id: StateId = source.parse().unwrap_or_else(|e| panic!("{source}: {e}"));
        assert!(snapshot.read(&id).is_some(), "{source} reads nothing");
        assert!(seen.insert(source.to_owned()), "{source} listed twice");
    }
    for pad in 0..8 {
        for state in ["playing", "loaded"] {
            assert!(seen.contains(&format!("sampler.pad{pad}.{state}")));
        }
    }
    assert_eq!(router.unknown_count(), 0);
}

#[test]
fn sampler_state_ids_for_every_pad_parse_and_read() {
    use om_core::StateId;
    let (engine, _audio) = DjEngine::new(2);
    let snapshot = engine.snapshot();
    for pad in 0..8 {
        for state in ["loaded", "playing", "volume", "pitch", "mode"] {
            let text = format!("sampler.pad{pad}.{state}");
            let id: StateId = text.parse().unwrap_or_else(|e| panic!("{text}: {e}"));
            assert!(snapshot.read(&id).is_some(), "{text}");
        }
    }
    for bad in ["sampler.pad8.playing", "sampler.pad99.mode"] {
        let id: StateId = bad.parse().expect("syntactically fine");
        assert!(snapshot.read(&id).is_none(), "{bad} names no pad");
    }
    for bad in [
        "sampler.pad0.trigger",
        "sampler.pad.mode",
        "sampler.padx.mode",
        "sampler.pad0",
    ] {
        assert!(bad.parse::<StateId>().is_err(), "{bad}");
    }
}

/// The record map's ids parse: the target is a `record.*` ParamId (taken by the app's typed target,
/// not the engine's), the LED source is a readable state id.
#[test]
fn every_id_in_record_map_json_is_valid() {
    use om_core::{ParamId, StateId};
    let text = std::fs::read_to_string(data_dir().join("record_map.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let (engine, _audio) = DjEngine::new(2);
    let snapshot = engine.snapshot();
    for m in json["mappings"].as_array().unwrap() {
        let target = m["target"].as_str().unwrap();
        let id: ParamId = target.parse().unwrap();
        assert!(matches!(id, ParamId::Record(_)));
        assert_eq!(id.to_string(), target);
    }
    for o in json["outputs"].as_array().unwrap() {
        let id: StateId = o["source"].as_str().unwrap().parse().unwrap();
        assert_eq!(snapshot.read(&id), Some(0.0), "idle reads 0");
    }
}
