//! Port of tests/unit/MidiMapperTest.cpp: map parsing, value modes, trace replay, parameter
//! routing, plus MidiTraceTest and ParameterRouterTest (kept in the same file as in C++).

mod support;

use om_core::{ParameterRouter, ParameterTarget};
use om_engine::DjEngine;
use om_midi::{
    control_change, note_off, note_on, pitch_bend, program_change, replay_trace, MidiEvent,
    MidiMapper, MidiTrace,
};
use std::sync::Arc;
use support::{assert_near, data_dir, RecordingTarget};

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
    assert_near(f.m.get_value("b"), 0.0);
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
    router.register_target("fx", a.clone());
    router.register_target("fx.delay", b.clone()); // longer prefix wins
    assert!(router.set_parameter("fx.delay.time", 0.3));
    assert_eq!(b.last("time"), 0.3);
    assert!(router.set_parameter("fx.reverb.size", 0.7));
    assert_eq!(a.last("reverb.size"), 0.7);
    router.unregister_target("fx.delay");
    assert!(router.set_parameter("fx.delay.time", 0.9));
    assert_eq!(a.last("delay.time"), 0.9);
    assert!(
        !router.set_parameter("fxx.time", 1.0),
        "prefix must match a whole segment"
    );
}
