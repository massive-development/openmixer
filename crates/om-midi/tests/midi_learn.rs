//! MIDI learn per control type, relative and 14-bit decoding, map
//! save/load, binding edits) and the map schema.
//!
//! Deviations, each marked where it occurs: the feedback rule validation (`outputs`) belongs to the
//! feedback unit and is only round-tripped here. Two expectations follow what the code produces: the eq offset encoder (the expected value
//! assumed an initial of -48 dB; the request default is 0), and the hotcue button (its release
//! drives the target back to 0, so the last change is 0, not 1; the press is checked first).

mod support;

use om_midi::{
    control_change, parse_hex, replay_trace, Blink, LearnRequest, LearnType, MapMode, Mapping,
    MidiKind, MidiMap, MidiMapper, MidiTrace, OutputKind, OutputMode, OutputRule,
};
use serde_json::json;
use std::sync::{Arc, Mutex};
use support::{assert_near, RecordingTarget};

fn trace_of(hex_messages: &[&str]) -> MidiTrace {
    let mut t = MidiTrace::default();
    let mut time = 0.0;
    for hex in hex_messages {
        time += 0.01;
        t.add(time, parse_hex(hex).expect("valid hex"));
    }
    t
}

struct Fixture {
    m: MidiMapper,
    t: Arc<RecordingTarget>,
    learned: Arc<Mutex<Vec<Mapping>>>,
}

fn fixture() -> Fixture {
    let t = Arc::new(RecordingTarget::default());
    let m = MidiMapper::new();
    m.set_target(t.clone());
    let learned = Arc::new(Mutex::new(Vec::new()));
    let sink = learned.clone();
    m.set_on_learned(move |mapping| sink.lock().unwrap().push(mapping.clone()));
    Fixture { m, t, learned }
}

impl Fixture {
    /// Arms and replays a one-message trace; returns the new binding.
    fn learn(&self, r: LearnRequest, hex: &str) -> Mapping {
        self.m.arm_learn(r);
        replay_trace(&trace_of(&[hex]), &self.m);
        self.m.get_bindings().last().cloned().unwrap_or_default()
    }

    fn replay(&self, hex: &[&str]) -> usize {
        replay_trace(&trace_of(hex), &self.m)
    }
}

fn request(target: &str, learn_type: LearnType) -> LearnRequest {
    LearnRequest {
        learn_type,
        ..LearnRequest::new(target)
    }
}

#[test]
fn cc_becomes_absolute_on_its_channel_and_is_not_forwarded() {
    let f = fixture();
    let b = f.learn(LearnRequest::new("mixer.ch0.fader"), "B2 07 40");
    assert_eq!(b.kind, MidiKind::Cc);
    assert_eq!(b.channel, 3);
    assert_eq!(b.number, 7);
    assert_eq!(b.mode, MapMode::Absolute);
    assert!(f.t.changes().is_empty(), "the learning message is consumed");
    assert!(!f.m.is_learning());
    let learned = f.learned.lock().unwrap();
    assert_eq!(learned.len(), 1);
    assert_eq!(learned[0].target, "mixer.ch0.fader");
    drop(learned);

    f.replay(&["B2 07 7F", "B2 07 00"]);
    assert_eq!(f.t.count("mixer.ch0.fader"), 2);
    assert_near(f.t.last("mixer.ch0.fader"), 0.0);
}

#[test]
fn any_channel_request_binds_all_channels() {
    let f = fixture();
    let r = LearnRequest {
        any_channel: true,
        ..LearnRequest::new("a")
    };
    assert_eq!(f.learn(r, "B4 10 01").channel, 0);
    f.replay(&["B9 10 7F"]);
    assert_eq!(f.t.count("a"), 1);
}

#[test]
fn note_becomes_button_or_toggle_and_ignores_note_off() {
    let f = fixture();
    f.m.arm_learn(LearnRequest::new("deck.0.hotcue0"));
    f.replay(&["80 24 00"]); // a release alone cannot bind
    assert!(f.m.is_learning());
    f.replay(&["90 24 64"]);
    let bindings = f.m.get_bindings();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].mode, MapMode::Button);

    f.replay(&["90 24 64", "80 24 00"]);
    // Press drives the button to 1, the release back to 0.
    let changes = f.t.changes();
    assert_eq!(changes.len(), 2);
    assert_near(changes[0].value, 1.0);
    assert_near(changes[1].value, 0.0);
    assert_eq!(f.t.count("deck.0.hotcue0"), 2);

    let toggle = f.learn(request("deck.0.play", LearnType::Toggle), "90 25 64");
    assert_eq!(toggle.mode, MapMode::Toggle);
    f.t.clear();
    f.replay(&["90 25 64", "80 25 00", "90 25 64", "80 25 00"]);
    let changes = f.t.changes();
    assert_eq!(changes.len(), 2);
    assert_near(changes[0].value, 1.0);
    assert_near(changes[1].value, 0.0);
}

#[test]
fn pitch_bend_becomes_absolute_with_the_requested_range() {
    let f = fixture();
    let b = f.learn(LearnRequest::for_target("deck.0.rate"), "E0 7F 7F");
    assert_eq!(b.kind, MidiKind::PitchBend);
    assert_near(b.min, 0.92);
    f.replay(&["E0 7F 7F", "E0 00 00"]);
    assert_near(f.t.last("deck.0.rate"), 0.92);
    assert_eq!(f.t.count("deck.0.rate"), 2);
}

#[test]
fn offset_encoder_decodes_around_sixty_four() {
    let f = fixture();
    let r = LearnRequest {
        min: -48.0,
        max: 6.0,
        step: 0.5,
        ..request("mixer.ch0.eq.low", LearnType::Relative)
    };
    assert_eq!(f.learn(r, "B0 15 41").mode, MapMode::Relative);
    f.replay(&["B0 15 41", "B0 15 42", "B0 15 3F"]); // +1, +2, -1 steps of 0.5
                                                     // The learned initial is 0 (the request default), so the three steps leave 0 + 0.5 + 1.0 - 0.5.
                                                     // (The expression assumed an initial of -48 and was looser than the arithmetic by 0.5.)
    assert_near(f.m.get_value("mixer.ch0.eq.low"), 1.0);
    let changes = f.t.changes();
    assert_eq!(changes.len(), 3);
    assert_near(changes[1].value - changes[0].value, 1.0);
    assert_near(changes[2].value - changes[1].value, -0.5);
}

#[test]
fn twos_complement_encoder_decodes_negative_as_high_values() {
    let f = fixture();
    let r = LearnRequest {
        step: 0.1,
        initial: 0.5,
        ..request("mixer.crossfader", LearnType::RelativeTwosComplement)
    };
    assert_eq!(f.learn(r, "B0 16 01").mode, MapMode::RelativeTwosComplement);
    f.t.clear();
    f.replay(&["B0 16 03", "B0 16 7E"]); // +3, -2
    let changes = f.t.changes();
    assert_eq!(changes.len(), 2);
    assert_near(changes[0].value, 0.8);
    assert_near(changes[1].value, 0.6);
}

#[test]
fn fourteen_bit_pair_combines_msb_and_lsb() {
    let f = fixture();
    let b = f.learn(
        request("mixer.ch0.fader", LearnType::Absolute14),
        "B0 01 00",
    );
    assert_eq!(b.mode, MapMode::Absolute14);
    assert_eq!(b.number, 1);
    assert_eq!(b.lsb_number, 33);
    f.t.clear();
    f.replay(&["B0 01 40", "B0 21 00"]); // 64 << 7 = 8192
    assert_near(f.t.last("mixer.ch0.fader"), 8192.0 / 16383.0);
    f.replay(&["B0 01 7F", "B0 21 7F"]);
    assert_near(f.t.last("mixer.ch0.fader"), 1.0);
    f.replay(&["B0 01 00", "B0 21 01"]);
    assert_near(f.t.last("mixer.ch0.fader"), 1.0 / 16383.0);
}

#[test]
fn fourteen_bit_accepts_the_lsb_first_and_refuses_out_of_range_controllers() {
    let f = fixture();
    f.m.arm_learn(request("x", LearnType::Absolute14));
    f.replay(&["B0 40 10"]); // cc 64 has no pair
    assert!(f.m.is_learning());
    f.replay(&["B0 22 10"]); // LSB of cc 2
    let bindings = f.m.get_bindings();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].number, 2);
}

#[test]
fn relearn_replaces_the_previous_bindings_of_target_and_control() {
    let f = fixture();
    f.learn(LearnRequest::new("a"), "B0 07 00");
    f.learn(LearnRequest::new("b"), "B0 08 00");
    f.learn(LearnRequest::new("a"), "B0 09 00"); // a moves to cc 9
    f.learn(LearnRequest::new("c"), "B0 08 00"); // cc 8 now drives c, not b
    let list = f.m.get_bindings();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].target, "a");
    assert_eq!(list[0].number, 9);
    assert_eq!(list[1].target, "c");
}

#[test]
fn cancel_disarms_and_lets_messages_through() {
    let f = fixture();
    f.m.add_binding(Mapping {
        number: 7,
        target: "a".into(),
        ..Mapping::default()
    })
    .unwrap();
    f.m.arm_learn(LearnRequest::new("b"));
    assert!(f.m.is_learning());
    f.m.cancel_learn();
    assert!(!f.m.is_learning());
    f.m.handle_message(&control_change(1, 7, 127));
    assert_eq!(f.t.count("a"), 1);
    assert!(f.learned.lock().unwrap().is_empty());
}

#[test]
fn defaults_follow_the_target_id() {
    assert_eq!(
        LearnRequest::for_target("deck.0.play").learn_type,
        LearnType::Toggle
    );
    assert_eq!(
        LearnRequest::for_target("deck.1.hotcue3").learn_type,
        LearnType::Button
    );
    assert_eq!(
        LearnRequest::for_target("mixer.ch2.pfl").learn_type,
        LearnType::Button
    );
    assert_eq!(
        LearnRequest::for_target("deck.0.loop.exit").learn_type,
        LearnType::Button
    );
    assert_eq!(
        LearnRequest::for_target("mixer.ch0.fader").learn_type,
        LearnType::Auto
    );
    let eq = LearnRequest::for_target("mixer.ch0.eq.high");
    assert_eq!(eq.min, -48.0);
    assert_eq!(eq.max, 6.0);
    assert_eq!(LearnRequest::for_target("mixer.master").max, 2.0);
}

#[test]
fn save_load_round_trip_keeps_everything_at_schema_v2() {
    let f = fixture();
    f.learn(
        request("mixer.ch0.fader", LearnType::Absolute14),
        "B0 01 00",
    );
    let r = LearnRequest {
        min: -48.0,
        max: 6.0,
        step: 0.5,
        invert: true,
        ..request("mixer.ch0.eq.low", LearnType::Relative)
    };
    f.learn(r, "B1 15 41");
    f.learn(request("deck.0.play", LearnType::Toggle), "90 24 64");
    // Feedback rules must survive a save.
    let outputs = vec![
        OutputRule {
            source: "deck.0.play".into(),
            number: 24,
            blink: Blink::WhenOff,
            blink_ms: 100,
            ..OutputRule::default()
        },
        OutputRule {
            source: "mixer.master.level".into(),
            kind: OutputKind::Cc,
            number: 9,
            mode: OutputMode::Level,
            ..OutputRule::default()
        },
    ];
    f.m.set_outputs(outputs.clone());
    f.m.set_name("Round trip");

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("nested").join("map.json");
    f.m.save_to_file(&file).unwrap();
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(saved["version"], json!(2));
    assert_eq!(saved["outputs"][0]["blink"], json!("off"));

    let loaded = MidiMapper::new();
    loaded.load_from_file(&file).unwrap();
    assert_eq!(loaded.name(), "Round trip");
    assert_eq!(loaded.get_bindings(), f.m.get_bindings());
    assert_eq!(loaded.get_outputs(), outputs);
    assert_eq!(loaded.to_json(), f.m.to_json());
}

#[test]
fn version_one_files_without_a_version_field_still_load() {
    let map =
        MidiMap::parse(r#"{"name":"old","mappings":[{"type":"cc","number":7,"target":"a"}]}"#)
            .unwrap();
    assert_eq!(map.version, 1);
    assert_eq!(map.mappings.len(), 1);
    assert!(map.outputs.is_empty());
    assert!(MidiMap::parse(r#"{"version":3,"mappings":[]}"#).is_err());
    assert!(MidiMap::parse(
        r#"{"mappings":[{"type":"note","number":1,"target":"a","mode":"absolute14"}]}"#
    )
    .is_err());
}

#[test]
fn feedback_rules_are_validated_when_the_map_is_parsed() {
    // The deferred half of MidiMapSchemaTest: a rule with an unknown type fails the whole map.
    let err =
        MidiMap::parse(r#"{"version":2,"mappings":[],"outputs":[{"source":"x","type":"zzz"}]}"#)
            .unwrap_err();
    assert_eq!(err.to_string(), "output 0: output type must be note or cc");
    let err = MidiMap::parse(
        r#"{"version":2,"mappings":[],"outputs":[{"source":"a","type":"note"},{"type":"cc"}]}"#,
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "output 1: missing source");
    let ok = MidiMap::parse(
        r#"{"version":2,"mappings":[],"outputs":[{"source":"deck.0.play","type":"note","number":36}]}"#,
    )
    .unwrap();
    assert_eq!(ok.outputs.len(), 1);
    assert_eq!(ok.outputs[0].number, 36);
}

#[test]
fn binding_editing_adds_replaces_removes_and_validates() {
    let f = fixture();
    let a = Mapping {
        number: 1,
        target: "a".into(),
        ..Mapping::default()
    };
    let b = Mapping {
        number: 2,
        target: "b".into(),
        ..a.clone()
    };
    f.m.add_binding(a.clone()).unwrap();
    f.m.add_binding(b).unwrap();
    let bad = Mapping {
        target: String::new(),
        ..a.clone()
    };
    assert!(f.m.add_binding(bad).is_err());
    let flat = Mapping {
        max: a.min,
        ..a.clone()
    };
    assert!(f.m.replace_binding(0, flat).is_err());
    assert!(f.m.replace_binding(5, a.clone()).is_err());
    assert_eq!(f.m.num_mappings(), 2);

    let moved = Mapping { number: 9, ..a };
    f.m.replace_binding(0, moved).unwrap();
    f.replay(&["B0 01 7F", "B0 09 7F"]);
    assert_eq!(f.t.count("a"), 1, "old address is gone, the new one works");

    assert!(f.m.remove_binding(1));
    assert!(!f.m.remove_binding(1));
    assert_eq!(f.m.remove_bindings_for("a"), 1);
    assert_eq!(f.m.num_mappings(), 0);
}

#[test]
fn every_edit_bumps_the_generation() {
    let f = fixture();
    let g0 = f.m.generation();
    f.m.add_binding(Mapping {
        target: "a".into(),
        ..Mapping::default()
    })
    .unwrap();
    let g1 = f.m.generation();
    f.m.set_outputs(Vec::new());
    assert!(g1 > g0);
    assert!(f.m.generation() > g1);
}
