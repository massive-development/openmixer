//! Port of tests/unit/MidiFeedbackTest.cpp: output rules, change-only sending, blink on an injected
//! clock, the output router's pairing rule, and the rule schema (OutputRuleTest).

mod support;

use om_midi::{
    pair_outputs, MidiEvent, MidiFeedback, MidiKind, MidiMapper, MidiOutputSink, MidiPortInfo,
    OutputRule,
};
use serde_json::json;
use std::sync::{Arc, Mutex};
use support::{FakeState, RecordingOutputSink};

struct Fixture {
    state: Arc<FakeState>,
    mapper: Arc<MidiMapper>,
    now: Arc<Mutex<f64>>,
    feedback: MidiFeedback,
    sink: Arc<RecordingOutputSink>,
}

fn fixture() -> Fixture {
    let state = Arc::new(FakeState::default());
    let mapper = Arc::new(MidiMapper::new());
    let now = Arc::new(Mutex::new(0.0));
    let clock_now = Arc::clone(&now);
    let feedback = MidiFeedback::new(
        state.clone(),
        mapper.clone(),
        Some(Box::new(move || *clock_now.lock().unwrap())),
    );
    Fixture {
        state,
        mapper,
        now,
        feedback,
        sink: Arc::new(RecordingOutputSink::default()),
    }
}

impl Fixture {
    fn load(&self, outputs_json: &str) {
        self.mapper
            .load_from_json(&format!(
                r#"{{"version":2,"mappings":[],"outputs":{outputs_json}}}"#
            ))
            .expect("map loads");
    }

    fn set_now(&self, ms: f64) {
        *self.now.lock().unwrap() = ms;
    }

    fn attach_sink(&mut self) {
        let sink: Arc<dyn MidiOutputSink> = self.sink.clone();
        self.feedback.set_sink(Some(sink));
    }
}

fn decode(bytes: &[u8]) -> MidiEvent {
    MidiEvent::from_bytes(bytes).expect("a channel message")
}

#[test]
fn nothing_is_sent_without_a_sink() {
    let f = fixture();
    f.load(r#"[{"source":"deck.0.play","type":"note","number":36}]"#);
    f.state.set("deck.0.play", 1.0);
    let mut feedback = f.feedback;
    assert_eq!(feedback.update(), 0);
}

#[test]
fn sends_only_when_the_value_changes() {
    let mut f = fixture();
    f.load(r#"[{"source":"deck.0.play","type":"note","channel":2,"number":36,"on":100,"off":0}]"#);
    f.attach_sink();
    f.state.set("deck.0.play", 0.0);
    assert_eq!(f.feedback.update(), 1, "initial state is sent once");
    assert_eq!(f.feedback.update(), 0);
    assert_eq!(f.feedback.update(), 0);
    f.state.set("deck.0.play", 1.0);
    assert_eq!(f.feedback.update(), 1);
    assert_eq!(f.feedback.update(), 0);

    let sent = f.sink.sent();
    assert_eq!(sent.len(), 2);
    let off = decode(&sent[0]);
    assert!(!off.note_on, "note-off first");
    let on = decode(&sent[1]);
    assert!(on.note_on);
    assert_eq!(on.channel, 2);
    assert_eq!(on.number, 36);
    assert_eq!(on.value, 100);
}

#[test]
fn level_rule_scales_and_sends_cc_only_when_the_quantised_value_moves() {
    let mut f = fixture();
    f.load(r#"[{"source":"mixer.master.level","type":"cc","number":9,"mode":"level"}]"#);
    f.attach_sink();
    f.state.set("mixer.master.level", 0.5);
    f.feedback.update();
    f.state.set("mixer.master.level", 0.5001); // same 0-127 step
    assert_eq!(f.feedback.update(), 0);
    f.state.set("mixer.master.level", 1.5); // clamps at the top
    f.feedback.update();

    let sent = f.sink.sent();
    assert_eq!(sent.len(), 2);
    let first = decode(&sent[0]);
    assert_eq!(first.kind, MidiKind::Cc);
    assert_eq!(first.value, 64);
    assert_eq!(decode(&sent[1]).value, 127);
}

#[test]
fn blink_alternates_each_half_period_while_the_state_is_on() {
    let mut f = fixture();
    f.load(r#"[{"source":"deck.0.cue","type":"note","number":37,"blink":"on","blinkMs":100}]"#);
    f.attach_sink();
    f.state.set("deck.0.cue", 1.0);
    let mut lit = Vec::new();
    let mut t = 0.0;
    while t < 450.0 {
        f.set_now(t);
        if f.feedback.update() > 0 {
            lit.push(decode(&f.sink.last().unwrap()).note_on);
        }
        t += 10.0;
    }
    assert_eq!(lit, vec![true, false, true, false, true]);

    f.state.set("deck.0.cue", 0.0); // off: steady, no more blinking
    f.set_now(500.0);
    f.feedback.update();
    let n = f.sink.sent().len();
    let mut t = 510.0;
    while t < 900.0 {
        f.set_now(t);
        f.feedback.update();
        t += 10.0;
    }
    assert_eq!(f.sink.sent().len(), n);
    assert!(!decode(&f.sink.last().unwrap()).note_on);
}

#[test]
fn blink_when_off_blinks_only_while_off() {
    let mut f = fixture();
    f.load(r#"[{"source":"deck.0.loaded","type":"note","number":38,"blink":"off","blinkMs":50}]"#);
    f.attach_sink();
    f.state.set("deck.0.loaded", 0.0);
    let mut t = 0.0;
    while t < 200.0 {
        f.set_now(t);
        f.feedback.update();
        t += 10.0;
    }
    assert_eq!(f.sink.sent().len(), 4);
    f.state.set("deck.0.loaded", 1.0);
    f.set_now(200.0);
    f.feedback.update();
    let n = f.sink.sent().len();
    f.set_now(260.0);
    assert_eq!(f.feedback.update(), 0);
    assert_eq!(f.sink.sent().len(), n);
    assert!(decode(&f.sink.last().unwrap()).note_on);
}

#[test]
fn unknown_sources_are_skipped_and_a_map_reload_resends_everything() {
    let mut f = fixture();
    f.load(
        r#"[{"source":"nope.x","type":"note","number":1},{"source":"a","type":"note","number":2}]"#,
    );
    f.attach_sink();
    f.state.set("a", 1.0);
    assert_eq!(f.feedback.update(), 1);
    assert_eq!(f.feedback.update(), 0);

    f.load(r#"[{"source":"a","type":"note","number":3}]"#);
    assert_eq!(f.feedback.update(), 1, "new rules light up from scratch");
    assert_eq!(decode(&f.sink.last().unwrap()).number, 3);

    f.mapper.set_outputs(Vec::new());
    assert_eq!(f.feedback.update(), 0);
}

#[test]
fn a_new_sink_gets_the_current_state_again() {
    let mut f = fixture();
    f.load(r#"[{"source":"a","type":"note","number":2}]"#);
    f.state.set("a", 1.0);
    f.attach_sink();
    f.feedback.update();
    let second = Arc::new(RecordingOutputSink::default());
    let second_sink: Arc<dyn MidiOutputSink> = second.clone();
    f.feedback.set_sink(Some(second_sink));
    assert_eq!(f.feedback.update(), 1);
    assert_eq!(second.sent().len(), 1);
}

#[test]
fn output_rule_parse_rejects_bad_rules_and_round_trips() {
    assert!(
        OutputRule::parse(&json!({ "type": "note" })).is_err(),
        "no source"
    );
    assert!(OutputRule::parse(&json!({ "source": "a", "type": "x" })).is_err());
    assert!(OutputRule::parse(&json!({ "source": "a", "type": "cc", "number": 200 })).is_err());
    assert!(OutputRule::parse(&json!({ "source": "a", "type": "cc", "blink": "x" })).is_err());
    assert!(OutputRule::parse(&json!({ "source": "a", "type": "cc", "mode": "x" })).is_err());
    let rule = OutputRule::parse(
        &json!({ "source": "a", "type": "cc", "number": 5, "on": 90, "blink": "on" }),
    )
    .expect("valid rule");
    let again = OutputRule::parse(&rule.to_value()).expect("round trip");
    assert_eq!(again.on_value, 90);
    assert_eq!(again.blink, om_midi::Blink::WhenOn);
    assert_eq!(again.number, 5);
    assert_eq!(again, rule);
}

#[test]
fn output_rule_errors_match_the_schema_messages() {
    let message = |v: serde_json::Value| OutputRule::parse(&v).unwrap_err().to_string();
    assert_eq!(
        message(json!({ "source": "a", "type": "x" })),
        "output type must be note or cc"
    );
    assert_eq!(
        message(json!({ "source": "a", "type": "cc", "mode": "wobble" })),
        "unknown output mode \"wobble\""
    );
    assert_eq!(
        message(json!({ "source": "a", "type": "cc", "blink": "x" })),
        "blink must be \"on\" or \"off\""
    );
    assert_eq!(
        message(json!({ "source": "a", "type": "cc", "min": 1, "max": 1 })),
        "min and max must differ, blinkMs must be positive"
    );
}

#[test]
fn output_pairing_matches_outputs_to_enabled_inputs_by_name() {
    let port = |id: &str, name: &str| MidiPortInfo {
        id: id.into(),
        name: name.into(),
    };
    let inputs = vec![port("in1", "Pad"), port("in2", "Deck")];
    let outputs = vec![
        port("out1", "Deck"),
        port("out2", "Other"),
        port("out3", "Pad"),
    ];
    let some = pair_outputs(&inputs, &["in2".to_owned()], false, &outputs);
    assert_eq!(some.len(), 1);
    assert_eq!(some[0].id, "out1");
    assert_eq!(pair_outputs(&inputs, &[], true, &outputs).len(), 2);
    assert_eq!(pair_outputs(&inputs, &[], false, &outputs).len(), 0);
}
