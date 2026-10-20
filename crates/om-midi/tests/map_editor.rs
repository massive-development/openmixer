//! The mapping editor model: edits, validation, undo and redo, conflicts, learn, import and export,
//! and live apply through the mapper. Mock backend and recording target only; no hardware.

mod support;

use om_midi::{
    control_change, find_conflicts, note_on, pitch_bend, BindingDraft, Keep, MapEditor, MapMode,
    Mapping, MidiKind, MidiMap, MidiMapper, HISTORY_LIMIT, MAX_MAP_BYTES, MONITOR_CAPACITY,
};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use support::RecordingTarget;

fn cc(channel: i32, number: i32, target: &str) -> Mapping {
    Mapping {
        kind: MidiKind::Cc,
        channel,
        number,
        target: target.to_owned(),
        ..Mapping::default()
    }
}

fn note(channel: i32, number: i32, target: &str) -> Mapping {
    Mapping {
        kind: MidiKind::Note,
        channel,
        number,
        target: target.to_owned(),
        mode: MapMode::Button,
        ..Mapping::default()
    }
}

struct Fixture {
    mapper: Arc<MidiMapper>,
    target: Arc<RecordingTarget>,
    editor: MapEditor,
    dir: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let mapper = Arc::new(MidiMapper::new());
    let target = Arc::new(RecordingTarget::default());
    mapper.set_target(target.clone());
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("map.json");
    let editor = MapEditor::new(Arc::clone(&mapper), Some(path));
    Fixture {
        mapper,
        target,
        editor,
        dir,
    }
}

fn path_in(f: &Fixture, name: &str) -> PathBuf {
    f.dir.path().join(name)
}

#[test]
fn add_edit_delete_round_trip_through_the_live_map() {
    let mut f = fixture();
    let i = f.editor.add_binding(cc(1, 7, "mixer.ch0.fader")).unwrap();
    assert_eq!(i, 0);
    f.editor.add_binding(note(0, 36, "deck.0.play")).unwrap();
    assert_eq!(f.editor.bindings().len(), 2);

    f.editor
        .set_binding(0, cc(2, 8, "mixer.ch0.fader"))
        .unwrap();
    assert_eq!(f.editor.bindings()[0].channel, 2);
    assert_eq!(f.editor.bindings()[0].number, 8);

    f.editor.delete_binding(1).unwrap();
    assert_eq!(f.editor.bindings().len(), 1);
    assert!(f.editor.delete_binding(5).is_err());
}

#[test]
fn undo_and_redo_walk_the_history() {
    let mut f = fixture();
    f.editor.add_binding(cc(0, 7, "mixer.ch0.fader")).unwrap();
    f.editor.add_binding(cc(0, 8, "mixer.ch1.fader")).unwrap();
    f.editor.delete_binding(0).unwrap();
    assert_eq!(f.editor.bindings().len(), 1);

    assert!(f.editor.undo());
    assert_eq!(f.editor.bindings().len(), 2);
    assert!(f.editor.undo());
    assert!(f.editor.undo());
    assert!(f.editor.bindings().is_empty());
    assert!(!f.editor.undo(), "nothing left to undo");

    assert!(f.editor.redo());
    assert!(f.editor.redo());
    assert_eq!(f.editor.bindings().len(), 2);
    assert!(f.editor.redo());
    assert_eq!(f.editor.bindings().len(), 1);
    assert!(!f.editor.redo());
}

#[test]
fn a_new_change_clears_redo() {
    let mut f = fixture();
    f.editor.add_binding(cc(0, 7, "mixer.ch0.fader")).unwrap();
    assert!(f.editor.undo());
    assert!(f.editor.can_redo());
    f.editor.add_binding(cc(0, 9, "mixer.ch2.fader")).unwrap();
    assert!(!f.editor.can_redo());
}

#[test]
fn history_is_bounded() {
    let mut f = fixture();
    for n in 0..(HISTORY_LIMIT + 20) {
        f.editor
            .add_binding(cc(0, (n % 128) as i32, "mixer.ch0.fader"))
            .unwrap();
    }
    let mut undone = 0;
    while f.editor.undo() {
        undone += 1;
    }
    assert_eq!(undone, HISTORY_LIMIT);
}

#[test]
fn validation_errors_name_the_problem_and_change_nothing() {
    let mut f = fixture();
    let bad_target = cc(0, 7, "deck.0.nonsense");
    let err = f.editor.add_binding(bad_target).unwrap_err().to_string();
    assert!(err.contains("not a parameter id"), "{err}");

    let mut bad_channel = cc(0, 7, "mixer.ch0.fader");
    bad_channel.channel = 17;
    assert!(f.editor.add_binding(bad_channel).is_err());

    let mut bad_number = cc(0, 7, "mixer.ch0.fader");
    bad_number.number = 128;
    assert!(f.editor.add_binding(bad_number).is_err());

    let mut flat = cc(0, 7, "mixer.ch0.fader");
    flat.min = 1.0;
    flat.max = 1.0;
    assert!(f.editor.add_binding(flat).is_err());

    assert!(f.editor.bindings().is_empty());
    assert!(!f.editor.can_undo(), "a rejected edit records no undo step");
}

#[test]
fn draft_reports_the_field_that_is_wrong() {
    let base = BindingDraft::from_mapping(&cc(1, 7, "mixer.ch0.fader"));
    assert!(base.to_mapping().is_ok());

    let mut d = base.clone();
    d.target = "deck.9.nonsense".into();
    let err = d.to_mapping().unwrap_err().to_string();
    assert!(err.contains("deck.9.nonsense"), "{err}");

    let mut d = base.clone();
    d.channel = "x".into();
    assert!(d.to_mapping().unwrap_err().to_string().contains("Channel"));

    let mut d = base.clone();
    d.min = "nan".into();
    assert!(d.to_mapping().unwrap_err().to_string().contains("Min"));

    let mut d = base.clone();
    d.max = d.min.clone();
    assert!(d.to_mapping().is_err());

    let mut d = base;
    d.step = String::new();
    let m = d.to_mapping().unwrap();
    assert!(
        (m.step - 1.0 / 127.0).abs() < 1e-12,
        "empty step takes the default"
    );
}

#[test]
fn duplicate_and_set_channel() {
    let mut f = fixture();
    f.editor.add_binding(note(1, 36, "deck.0.play")).unwrap();
    let copy = f.editor.duplicate_binding(0).unwrap();
    assert_eq!(copy, 1);
    assert_eq!(f.editor.bindings()[1], f.editor.bindings()[0]);
    f.editor.set_channel(1, 0).unwrap();
    assert_eq!(f.editor.bindings()[1].channel, 0);
    assert_eq!(f.editor.bindings()[0].channel, 1);
}

#[test]
fn channel_zero_and_channel_specific_conflict_and_resolve() {
    let mut f = fixture();
    f.editor.add_binding(cc(0, 7, "mixer.ch0.fader")).unwrap();
    f.editor.add_binding(cc(3, 7, "mixer.ch1.fader")).unwrap();
    f.editor.add_binding(cc(4, 7, "mixer.ch2.fader")).unwrap();
    f.editor.add_binding(cc(0, 9, "mixer.ch3.fader")).unwrap();

    let conflicts = f.editor.conflicts();
    assert_eq!(
        conflicts.len(),
        2,
        "any-channel fires with both channel-specific ones"
    );
    assert_eq!((conflicts[0].first, conflicts[0].second), (0, 1));
    assert_eq!((conflicts[1].first, conflicts[1].second), (0, 2));
    assert!(conflicts[0].control.contains("CC 7"));

    // Keep the channel-specific binding on channel 3; the any-channel one goes.
    f.editor
        .resolve_conflict(&conflicts[0], Keep::Second)
        .unwrap();
    let bindings = f.editor.bindings();
    assert_eq!(bindings.len(), 3);
    assert_eq!(bindings[0].channel, 3);
    assert_eq!(
        f.editor.conflicts().len(),
        0,
        "channels 3 and 4 do not overlap"
    );

    // A stale conflict is refused.
    assert!(f
        .editor
        .resolve_conflict(&conflicts[1], Keep::First)
        .is_err());
}

#[test]
fn same_channel_and_different_kinds_do_not_conflict() {
    let bindings = vec![
        cc(1, 7, "mixer.ch0.fader"),
        cc(2, 7, "mixer.ch1.fader"),
        note(1, 7, "deck.0.play"),
    ];
    assert!(find_conflicts(&bindings).is_empty());
    let same = vec![cc(1, 7, "mixer.ch0.fader"), cc(1, 7, "mixer.ch1.fader")];
    assert_eq!(find_conflicts(&same).len(), 1);
}

#[test]
fn absolute14_lsb_counts_as_a_control_address() {
    let mut wide = cc(0, 1, "mixer.ch0.fader");
    wide.mode = MapMode::Absolute14;
    wide.lsb_number = 33;
    let clash = cc(0, 33, "mixer.ch1.fader");
    let conflicts = find_conflicts(&[wide, clash]);
    assert_eq!(conflicts.len(), 1, "the LSB controller is taken too");
}

#[test]
fn learn_into_keeps_target_and_scaling() {
    let mut f = fixture();
    let mut fader = cc(1, 7, "mixer.ch0.fader");
    fader.min = -48.0;
    fader.max = 6.0;
    fader.initial = -10.0;
    fader.invert = true;
    f.editor.add_binding(fader).unwrap();

    f.editor.learn_into(0).unwrap();
    assert!(f.editor.is_learning());
    // The next CC, on channel 2, binds to the same target.
    f.mapper.handle_message(&control_change(2, 21, 90));
    assert!(f.editor.poll_learn().is_some());

    let bindings = f.editor.bindings();
    assert_eq!(
        bindings.len(),
        1,
        "the old control is replaced, not duplicated"
    );
    let b = &bindings[0];
    assert_eq!(b.target, "mixer.ch0.fader");
    assert_eq!(b.number, 21);
    assert_eq!(b.channel, 2);
    assert_eq!(
        (b.min, b.max, b.initial, b.invert),
        (-48.0, 6.0, -10.0, true)
    );
    assert_eq!(b.mode, MapMode::Absolute);
}

#[test]
fn learn_new_for_a_target_binds_the_next_control() {
    let mut f = fixture();
    f.editor.learn_new("deck.0.play").unwrap();
    f.mapper.handle_message(&note_on(0, 36, 100));
    let learned = f.editor.poll_learn().expect("a binding was captured");
    assert_eq!(learned.target, "deck.0.play");
    assert_eq!(learned.kind, MidiKind::Note);
    assert_eq!(learned.mode, MapMode::Toggle, "play learns as a toggle");
    assert!(f.editor.can_undo(), "learn is an undo step");
    assert!(f.editor.undo());
    assert!(f.editor.bindings().is_empty());
}

#[test]
fn cancelled_learn_records_nothing() {
    let mut f = fixture();
    f.editor.learn_new("deck.0.play").unwrap();
    f.editor.cancel_learn();
    assert!(!f.editor.is_learning());
    assert!(f.editor.poll_learn().is_none());
    assert!(f.editor.bindings().is_empty());
    assert!(!f.editor.can_undo());
}

#[test]
fn live_apply_reaches_the_target() {
    let mut f = fixture();
    f.editor.add_binding(cc(1, 7, "mixer.ch0.fader")).unwrap();
    f.mapper.handle_message(&control_change(1, 7, 127));
    assert!((f.target.last("mixer.ch0.fader") - 1.0).abs() < 1e-9);

    f.editor.undo();
    f.target.clear();
    f.mapper.handle_message(&control_change(1, 7, 127));
    assert_eq!(
        f.target.count("mixer.ch0.fader"),
        0,
        "undone binding no longer fires"
    );
}

#[test]
fn fourteen_bit_and_relative_bindings_are_listed_with_their_kind() {
    let json = r#"{ "version": 2, "name": "wide", "mappings": [
        { "type": "cc", "number": 1, "lsb": 33, "target": "mixer.ch0.fader", "mode": "absolute14" },
        { "type": "cc", "number": 21, "target": "mixer.ch0.eq.low", "mode": "relative", "step": 0.5, "min": -48, "max": 6 },
        { "type": "cc", "number": 22, "target": "mixer.ch1.eq.low", "mode": "relative2c" }
    ] }"#;
    let f = fixture();
    f.mapper.load_from_json(json).unwrap();
    let b = f.editor.bindings();
    assert_eq!(b[0].mode, MapMode::Absolute14);
    assert_eq!(b[0].lsb_number, 33);
    assert_eq!(b[1].mode, MapMode::Relative);
    assert_eq!(b[2].mode, MapMode::RelativeTwosComplement);
    assert_eq!(BindingDraft::from_mapping(&b[0]).lsb, "33");
    // The 14-bit pair: MSB then LSB reach the target as one value.
    f.mapper.handle_message(&control_change(0, 1, 64));
    f.mapper.handle_message(&control_change(0, 33, 0));
    assert!((f.target.last("mixer.ch0.fader") - 8192.0 / 16383.0).abs() < 1e-6);
}

#[test]
fn write_then_parse_is_equal() {
    let mut f = fixture();
    f.editor.set_metadata("Test pad", "Launchpad").unwrap();
    f.editor.add_binding(cc(1, 7, "mixer.ch0.fader")).unwrap();
    f.editor.add_binding(note(0, 36, "deck.0.play")).unwrap();
    let mut pb = cc(0, 0, "deck.1.rate");
    pb.kind = MidiKind::PitchBend;
    pb.min = 0.92;
    pb.max = 1.08;
    pb.initial = 1.0;
    f.editor.add_binding(pb).unwrap();
    let mut wide = cc(0, 2, "mixer.ch1.fader");
    wide.mode = MapMode::Absolute14;
    wide.lsb_number = 34;
    f.editor.add_binding(wide).unwrap();

    let json = f.mapper.to_json();
    let reparsed = MidiMap::parse(&json).unwrap();
    assert_eq!(reparsed, f.mapper.snapshot());
    assert_eq!(reparsed.device, "Launchpad");
    assert_eq!(reparsed.name, "Test pad");
    assert_eq!(MidiMap::parse(&reparsed.to_json()).unwrap(), reparsed);
}

#[test]
fn save_writes_bak_and_clears_dirty() {
    let mut f = fixture();
    let path = f.editor.path().unwrap().to_path_buf();
    f.editor.add_binding(cc(0, 7, "mixer.ch0.fader")).unwrap();
    assert!(f.editor.is_dirty());
    f.editor.save().unwrap();
    assert!(!f.editor.is_dirty());
    assert!(path.is_file());
    assert!(
        !path_in(&f, "map.json.bak").exists(),
        "no previous file, no backup"
    );

    f.editor.add_binding(cc(0, 8, "mixer.ch1.fader")).unwrap();
    f.editor.save().unwrap();
    let bak = path_in(&f, "map.json.bak");
    let before = MidiMap::parse(&fs::read_to_string(&bak).unwrap()).unwrap();
    assert_eq!(before.mappings.len(), 1, "the .bak holds the previous save");
    let now = MidiMap::parse(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(now.mappings.len(), 2);
    assert!(
        !path_in(&f, "map.json.tmp").exists(),
        "the temp file is renamed away"
    );
}

#[test]
fn save_without_a_path_says_so() {
    let mapper = Arc::new(MidiMapper::new());
    let mut editor = MapEditor::new(mapper, None);
    let err = editor.save().unwrap_err().to_string();
    assert!(err.contains("No map file"), "{err}");
}

#[test]
fn import_refuses_newer_versions_hostile_and_oversized_files() {
    let mut f = fixture();
    f.editor.add_binding(cc(0, 7, "mixer.ch0.fader")).unwrap();
    let before = f.mapper.to_json();

    let newer = path_in(&f, "newer.json");
    fs::write(&newer, r#"{"version": 3, "mappings": []}"#).unwrap();
    assert!(f.editor.import_file(&newer).is_err());

    let hostile = path_in(&f, "hostile.json");
    fs::write(&hostile, "\u{0}\u{1}not json at all {{{").unwrap();
    assert!(f.editor.import_file(&hostile).is_err());

    let bad_binding = path_in(&f, "bad.json");
    fs::write(
        &bad_binding,
        r#"{"mappings":[{"type":"cc","number":500,"target":"mixer.ch0.fader"}]}"#,
    )
    .unwrap();
    assert!(f.editor.import_file(&bad_binding).is_err());

    let deep = path_in(&f, "deep.json");
    fs::write(&deep, format!("{}{}", "[".repeat(5000), "]".repeat(5000))).unwrap();
    assert!(f.editor.import_file(&deep).is_err());

    let huge = path_in(&f, "huge.json");
    let file = fs::File::create(&huge).unwrap();
    file.set_len(MAX_MAP_BYTES + 1).unwrap();
    let err = f.editor.import_file(&huge).unwrap_err().to_string();
    assert!(err.contains("larger than 1 MB"), "{err}");

    assert_eq!(
        f.mapper.to_json(),
        before,
        "no refused import changed the map"
    );
    assert!(f.editor.is_dirty(), "the earlier add is still unsaved");
}

#[test]
fn import_then_export_round_trip() {
    let mut f = fixture();
    let src = path_in(&f, "src.json");
    fs::write(
        &src,
        r#"{"version":1,"name":"Old","mappings":[{"type":"note","number":36,"target":"deck.0.play"}]}"#,
    )
    .unwrap();
    f.editor.import_file(&src).unwrap();
    assert_eq!(f.editor.name(), "Old");
    assert_eq!(f.editor.bindings()[0].mode, MapMode::Button);
    let out = path_in(&f, "out.json");
    f.editor.export_file(&out).unwrap();
    assert_eq!(
        MidiMap::parse(&fs::read_to_string(&out).unwrap())
            .unwrap()
            .version,
        2
    );
    assert!(
        !f.editor.path().unwrap().exists(),
        "export does not touch the save path"
    );
    assert!(f.editor.undo(), "import is undoable");
    assert!(f.editor.bindings().is_empty());
}

#[test]
fn reset_to_default_is_undoable() {
    let mut f = fixture();
    f.editor.add_binding(cc(0, 7, "mixer.ch0.fader")).unwrap();
    f.editor.reset_to_default().unwrap();
    assert!(f.editor.bindings().is_empty());
    assert!(f.editor.undo());
    assert_eq!(f.editor.bindings().len(), 1);
}

#[test]
fn dirty_flag_follows_the_saved_state() {
    let mut f = fixture();
    assert!(!f.editor.is_dirty());
    f.editor.add_binding(cc(0, 7, "mixer.ch0.fader")).unwrap();
    assert!(f.editor.is_dirty());
    assert!(f.editor.undo());
    assert!(!f.editor.is_dirty(), "back to the saved state");
}

#[test]
fn monitor_keeps_the_last_fifty_events_in_order() {
    let f = fixture();
    for n in 0..(MONITOR_CAPACITY as u8 + 10) {
        f.mapper.handle_message(&control_change(0, n % 120, 1));
    }
    let recent = f.mapper.recent_events();
    assert_eq!(recent.len(), MONITOR_CAPACITY);
    assert_eq!(
        recent.last().unwrap().event.number,
        ((MONITOR_CAPACITY as i32 + 9) % 120)
    );
    assert!(recent[0].at <= recent[recent.len() - 1].at);
    f.mapper.handle_message(&pitch_bend(0, 8192));
    assert_eq!(
        f.mapper.recent_events().last().unwrap().event.kind,
        MidiKind::PitchBend
    );
}

#[test]
fn bak_is_not_written_when_the_file_is_new() {
    let mut f = fixture();
    f.editor.add_binding(cc(0, 7, "mixer.ch0.fader")).unwrap();
    f.editor.save().unwrap();
    assert!(!path_in(&f, "map.json.bak").exists());
}
