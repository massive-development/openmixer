//! Device layer over the mock backend: selection, hot-plug, recording, and output pairing. No
//! hardware is opened; the mock delivers injected bytes exactly as a controller would.

mod support;

use om_midi::{
    MidiFeedback, MidiInputRouter, MidiMapper, MidiOutputRouter, MidiOutputSink, MockBackend,
};
use std::sync::Arc;
use support::{FakeState, RecordingTarget};

const MAP: &str = r#"{"version":2,"mappings":[
  {"type":"cc","channel":1,"number":7,"target":"mixer.ch0.fader"}],
  "outputs":[{"source":"deck.0.play","type":"note","channel":1,"number":36}]}"#;

fn mapper() -> (Arc<MidiMapper>, Arc<RecordingTarget>) {
    let target = Arc::new(RecordingTarget::default());
    let mapper = Arc::new(MidiMapper::new());
    mapper.set_target(target.clone());
    mapper.load_from_json(MAP).expect("map loads");
    (mapper, target)
}

#[test]
fn enabled_input_routes_injected_bytes_into_the_mapper() {
    let (mapper, target) = mapper();
    let router = MidiInputRouter::new(mapper);
    let mut backend = MockBackend::new();
    backend.add_device("Pad");
    let report = router.enable(&mut backend, true, &[]).expect("enable");
    assert_eq!(report.opened, 1);
    assert!(report.failed.is_empty());

    assert!(backend.inject("Pad", &[0xB0, 7, 127]));
    assert!((target.last("mixer.ch0.fader") - 1.0).abs() < 1e-9);
}

#[test]
fn selection_by_id_opens_only_the_chosen_inputs() {
    let (mapper, _) = mapper();
    let router = MidiInputRouter::new(mapper);
    let mut backend = MockBackend::new();
    backend.add_device("Pad");
    backend.add_device("Deck");

    assert_eq!(
        router
            .enable(&mut backend, false, &["Other".into()])
            .unwrap()
            .opened,
        0
    );
    assert!(router.open_ids().is_empty());
    assert_eq!(
        router
            .enable(&mut backend, false, &["Deck".into()])
            .unwrap()
            .opened,
        1
    );
    assert_eq!(router.open_ids(), vec!["Deck".to_owned()]);
    assert!(!backend.inject("Pad", &[0xB0, 7, 1]), "Pad is not open");
}

#[test]
fn hot_plug_opens_new_selected_inputs_and_closes_removed_ones() {
    let (mapper, _) = mapper();
    let router = MidiInputRouter::new(mapper);
    let mut backend = MockBackend::new();
    backend.add_device("Pad");
    router.enable(&mut backend, true, &[]).unwrap();

    backend.add_device("Deck");
    assert!(
        router.refresh(&mut backend).unwrap(),
        "a new device is a change"
    );
    let mut open = router.open_ids();
    open.sort();
    assert_eq!(open, vec!["Deck".to_owned(), "Pad".to_owned()]);

    backend.remove_device("Pad");
    assert!(router.refresh(&mut backend).unwrap());
    assert_eq!(router.open_ids(), vec!["Deck".to_owned()]);
    assert!(!backend.is_open("Pad"));

    assert!(
        !router.refresh(&mut backend).unwrap(),
        "no change, nothing to do"
    );
}

#[test]
fn disable_all_closes_every_input() {
    let (mapper, _) = mapper();
    let router = MidiInputRouter::new(mapper);
    let mut backend = MockBackend::new();
    backend.add_device("Pad");
    router.enable(&mut backend, true, &[]).unwrap();
    router.disable_all();
    assert!(router.open_ids().is_empty());
    assert!(!backend.is_open("Pad"));
}

#[test]
fn recording_captures_the_bytes_that_were_routed() {
    let (mapper, _) = mapper();
    let router = MidiInputRouter::new(mapper);
    let mut backend = MockBackend::new();
    backend.add_device("Pad");
    router.enable(&mut backend, true, &[]).unwrap();

    router.start_recording();
    backend.inject("Pad", &[0xB0, 7, 64]);
    backend.inject("Pad", &[0xB0, 7, 65]);
    let trace = router.stop_recording().expect("was recording");
    assert_eq!(trace.events.len(), 2);
    assert_eq!(trace.events[1].bytes, vec![0xB0, 7, 65]);
    assert!(router.stop_recording().is_none());
}

#[test]
fn outputs_pair_with_enabled_inputs_and_carry_feedback() {
    let (mapper, _) = mapper();
    let state = Arc::new(FakeState::default());
    state.set("deck.0.play", 1.0);
    let mut feedback = MidiFeedback::new(state, mapper, None);

    let mut backend = MockBackend::new();
    backend.add_device("Pad");
    backend.add_device("Other");
    let router = Arc::new(MidiOutputRouter::new());
    assert_eq!(
        router
            .open_paired_with(&mut backend, &["Pad".into()], false)
            .unwrap(),
        1
    );
    assert_eq!(router.num_open(), 1);

    let sink: Arc<dyn MidiOutputSink> = router.clone();
    feedback.set_sink(Some(sink));
    assert_eq!(feedback.update(), 1);
    assert_eq!(backend.sent("Pad"), vec![vec![0x90, 36, 127]]);
    assert!(backend.sent("Other").is_empty());

    router.close_all();
    assert_eq!(router.num_open(), 0);
}

#[test]
fn mock_backend_reports_refresh_changes_once() {
    let mut backend = MockBackend::new();
    use om_midi::MidiBackend;
    assert!(
        backend.refresh().unwrap(),
        "the first refresh reports the list"
    );
    assert!(!backend.refresh().unwrap());
    backend.add_output("Pad");
    assert!(backend.refresh().unwrap());
}
