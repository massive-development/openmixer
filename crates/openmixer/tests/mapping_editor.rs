//! The mapping editor over the attached MIDI service: an edit reaches the running mapper at once,
//! Save writes the effective map file, and the unsaved guard reports edits. Mock device only.

use om_midi::MockBackend;
use openmixer::composition::AppComposition;

fn composition(scratch: &std::path::Path) -> AppComposition {
    AppComposition::new(&scratch.join("prefs.json"), None)
}

/// Fills the open form as a CC binding on channel 1, controller 7, driving the channel 0 fader.
fn fader_form(page: &mut om_ui::mapping_editor::MappingEditorPage) {
    page.new_form();
    let form = page.form_mut().expect("form is open");
    form.channel = "1".into();
    form.number = "7".into();
    form.target = "mixer.ch0.fader".into();
    page.apply_form().expect("the form applies");
}

#[test]
fn no_editor_until_midi_is_attached() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let c = composition(scratch.path());
    assert!(c.mapping_editor().is_none());
}

#[test]
fn an_edit_reaches_the_target_at_once() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let mut c = composition(scratch.path());
    let mock = MockBackend::new();
    mock.add_device("Pad");
    c.attach_midi(Box::new(mock.clone()));
    c.services
        .midi
        .clone()
        .expect("attached")
        .select_inputs(false, &["Pad".to_owned()]);
    c.prefs.midi.map_file = scratch
        .path()
        .join("map.json")
        .to_string_lossy()
        .into_owned();

    let mut page = c.mapping_editor().expect("MIDI is attached");
    fader_form(&mut page);
    assert!(mock.inject("Pad", &[0xB0, 7, 127]));
    assert!((f64::from(c.engine.mixer().fader(0)) - 1.0).abs() < 1e-6);
}

#[test]
fn save_writes_the_effective_map_file_and_clears_the_guard() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let mut c = composition(scratch.path());
    let mock = MockBackend::new();
    mock.add_device("Pad");
    c.attach_midi(Box::new(mock));
    let file = scratch.path().join("maps").join("map.json");
    c.prefs.midi.map_file = file.to_string_lossy().into_owned();

    let mut page = c.mapping_editor().expect("MIDI is attached");
    fader_form(&mut page);
    assert!(
        page.needs_save_prompt(),
        "an unsaved edit must trip the guard"
    );
    assert!(!file.exists(), "nothing is written before Save");

    page.save().expect("save succeeds");
    assert!(file.is_file(), "Save writes the map file");
    assert!(!page.needs_save_prompt(), "a saved map is clean");

    let text = std::fs::read_to_string(&file).expect("read saved map");
    assert!(text.contains("mixer.ch0.fader"), "saved map: {text}");
}

#[test]
fn undo_after_an_edit_is_unsaved_again_and_removes_the_binding() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let mut c = composition(scratch.path());
    let mock = MockBackend::new();
    mock.add_device("Pad");
    c.attach_midi(Box::new(mock));
    c.prefs.midi.map_file = scratch
        .path()
        .join("map.json")
        .to_string_lossy()
        .into_owned();

    let mut page = c.mapping_editor().expect("MIDI is attached");
    fader_form(&mut page);
    assert_eq!(
        c.services
            .midi
            .as_ref()
            .expect("attached")
            .mapper()
            .num_mappings(),
        1
    );
    page.undo().expect("the edit is undoable");
    assert_eq!(
        c.services
            .midi
            .as_ref()
            .expect("attached")
            .mapper()
            .num_mappings(),
        0
    );
    assert!(!page.needs_save_prompt(), "back to the saved (empty) map");
}
