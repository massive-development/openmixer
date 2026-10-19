//! MIDI through the composition, over the mock device: a controller message reaches the mixer
//! through the map, deck state lights the controller's LED, and hot-plug follows the device list.
//! No hardware is opened.

use om_midi::MockBackend;
use openmixer::composition::AppComposition;
use std::time::{Duration, Instant};

const MAP: &str = r#"{"version":2,
  "mappings":[{"type":"cc","channel":1,"number":7,"target":"mixer.ch0.fader"}],
  "outputs":[{"source":"deck.0.play","type":"note","channel":1,"number":36}]}"#;

fn composition(scratch: &std::path::Path) -> AppComposition {
    // A prefs file that does not exist: defaults, with no real preferences involved.
    AppComposition::new(&scratch.join("prefs.json"), None)
}

#[test]
fn controller_bytes_reach_the_mixer_through_the_map() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let mut c = composition(scratch.path());
    let mock = MockBackend::new();
    mock.add_device("Pad");
    c.attach_midi(Box::new(mock.clone()));
    let service = c.midi.clone().expect("attached");

    let map = scratch.path().join("map.json");
    std::fs::write(&map, MAP).expect("write map");
    service.load_map_file(&map).expect("map loads");
    service.select_inputs(false, &["Pad".to_owned()]);
    assert_eq!(service.open_inputs(), vec!["Pad".to_owned()]);

    assert!(mock.inject("Pad", &[0xB0, 7, 127]));
    assert!((f64::from(c.engine.mixer().fader(0)) - 1.0).abs() < 1e-6);
}

#[test]
fn deck_state_lights_the_controller_led() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let mut c = composition(scratch.path());
    let mock = MockBackend::new();
    mock.add_device("Pad");
    c.attach_midi(Box::new(mock.clone()));
    let service = c.midi.clone().expect("attached");

    let map = scratch.path().join("map.json");
    std::fs::write(&map, MAP).expect("write map");
    service.load_map_file(&map).expect("map loads");
    service.select_inputs(false, &["Pad".to_owned()]);

    // The feedback thread sends the LED state; wait for the play state to arrive.
    c.engine.deck(0).play();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if mock.sent("Pad").last() == Some(&vec![0x90, 36, 127]) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "LED never lit: {:?}",
            mock.sent("Pad")
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn hot_plug_opens_a_device_that_appears_while_running() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let mut c = composition(scratch.path());
    let mock = MockBackend::new();
    mock.add_device("Pad");
    c.attach_midi(Box::new(mock.clone()));
    let service = c.midi.clone().expect("attached");
    service.select_inputs(true, &[]);
    assert_eq!(service.open_inputs(), vec!["Pad".to_owned()]);

    mock.add_device("Deck");
    assert!(service.refresh(), "the new device is a change");
    let mut open = service.open_inputs();
    open.sort();
    assert_eq!(open, vec!["Deck".to_owned(), "Pad".to_owned()]);
    assert!(!service.refresh(), "nothing changed since");
}

#[test]
fn learn_controls_drive_the_mapper() {
    use om_ui::settings::MidiControl;
    let scratch = tempfile::tempdir().expect("tempdir");
    let mut c = composition(scratch.path());
    let mock = MockBackend::new();
    mock.add_device("Pad");
    c.attach_midi(Box::new(mock.clone()));
    let service = c.midi.clone().expect("attached");
    service.select_inputs(true, &[]);

    service.arm_learn("mixer.ch0.fader");
    assert!(service.is_learning());
    assert!(
        mock.inject("Pad", &[0xB0, 9, 64]),
        "the learned message is captured"
    );
    assert!(!service.is_learning());
    assert_eq!(service.mapper().num_mappings(), 1);
    assert!(mock.inject("Pad", &[0xB0, 9, 127]));
    assert!((f64::from(c.engine.mixer().fader(0)) - 1.0).abs() < 1e-6);
}
