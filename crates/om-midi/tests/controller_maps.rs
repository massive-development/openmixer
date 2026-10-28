//! Hardware-free checks of the controller-map catalog: the schema check over every shipped and
//! fixture map, selection by device name, and replay of traces through the generic maps into the
//! `Command`s they produce.

mod support;

use om_core::{Command, ParamId};
use om_midi::{
    check_map, check_map_json, replay_trace, select_map, MapCandidate, MapChoice, MidiMap,
    MidiMapper, MidiTrace,
};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use support::{data_dir, RecordingTarget};

fn shipped_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/controllers")
}

/// Every map file (not trace) in `dir`, sorted by path.
fn map_files(dir: &PathBuf) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter(|p| !p.to_string_lossy().ends_with("_trace.json"))
        .collect();
    files.sort();
    files
}

#[test]
fn every_shipped_and_fixture_map_passes_the_schema_check() {
    for dir in [shipped_dir(), data_dir()] {
        for path in map_files(&dir) {
            let issues = check_map_json(&fs::read_to_string(&path).unwrap());
            assert!(issues.is_empty(), "{}: {issues:?}", path.display());
        }
    }
}

#[test]
fn shipped_maps_are_original_and_selectable() {
    let files = map_files(&shipped_dir());
    assert!(files.len() >= 2);
    for path in files {
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("Clean-room"),
            "{} lacks the origin note",
            path.display()
        );
        let map = MidiMap::parse(&text).unwrap();
        assert!(
            !map.device.is_empty(),
            "{} has no device hint",
            path.display()
        );
    }
}

fn check_json(mappings: &str) -> Vec<String> {
    check_map_json(&format!(r#"{{"name":"t","mappings":[{mappings}]}}"#))
}

#[test]
fn schema_check_flags_bad_targets() {
    let issues = check_json(r#"{"type":"cc","number":1,"target":"mixer.ch9.fader"}"#);
    assert_eq!(issues.len(), 1, "{issues:?}");
    let issues = check_json(r#"{"type":"cc","number":1,"target":"mixer.nonsense"}"#);
    assert!(issues[0].contains("does not parse"), "{issues:?}");
    let issues = check_json(r#"{"type":"cc","number":1,"target":"plug.thing"}"#);
    assert!(issues[0].contains("not an engine id"), "{issues:?}");
}

#[test]
fn schema_check_flags_duplicate_and_overlapping_controls() {
    let dup = r#"{"type":"cc","channel":1,"number":7,"target":"mixer.ch0.fader"},
                 {"type":"cc","channel":1,"number":7,"target":"mixer.ch1.fader"}"#;
    let issues = check_json(dup);
    assert!(issues[0].contains("both fire"), "{issues:?}");
    let any_channel = r#"{"type":"cc","number":7,"target":"mixer.ch0.fader"},
                         {"type":"cc","channel":3,"number":7,"target":"mixer.ch1.fader"}"#;
    assert_eq!(check_json(any_channel).len(), 1);
    let distinct = r#"{"type":"cc","channel":1,"number":7,"target":"mixer.ch0.fader"},
                      {"type":"cc","channel":2,"number":7,"target":"mixer.ch1.fader"}"#;
    assert!(check_json(distinct).is_empty());
}

#[test]
fn schema_check_flags_bad_ranges() {
    let issues = check_json(r#"{"type":"cc","number":1,"target":"mixer.ch0.fader","initial":5}"#);
    assert!(issues[0].contains("initial"), "{issues:?}");
    let issues = check_json(
        r#"{"type":"cc","number":1,"target":"mixer.ch0.fader","mode":"relative","step":0}"#,
    );
    assert!(issues[0].contains("step"), "{issues:?}");
    let issues =
        check_json(r#"{"type":"cc","number":1,"target":"mixer.ch0.fader","min":1,"max":1}"#);
    assert!(issues[0].contains("min and max"), "{issues:?}");
}

#[test]
fn schema_check_flags_nameless_maps_and_output_problems() {
    assert_eq!(check_map(&MidiMap::default()), vec!["map has no name"]);
    let outputs = r#"{"name":"t","mappings":[],"outputs":[
        {"source":"deck.0.play","type":"note","number":5},
        {"source":"deck.1.play","type":"note","number":5},
        {"source":"mixer.nope","type":"note","number":6}]}"#;
    let issues = check_map_json(outputs);
    assert_eq!(issues.len(), 2, "{issues:?}");
    assert!(issues.iter().any(|i| i.contains("same LED")));
    assert!(issues.iter().any(|i| i.contains("output 2 source")));
}

fn candidates(entries: &[(&str, &str)]) -> Vec<MapCandidate> {
    entries
        .iter()
        .map(|(id, device)| MapCandidate {
            id: (*id).into(),
            device: (*device).into(),
        })
        .collect()
}

fn catalog() -> Vec<MapCandidate> {
    candidates(&[
        ("pads", "Generic 8 Knob 8 Pad"),
        ("mixer", "Generic 4 Channel Mixer"),
        ("acme", "Acme Deck"),
        ("acme-pro", "Acme Deck Pro"),
        ("blank", ""),
    ])
}

#[test]
fn selection_matches_alsa_and_windows_style_port_names() {
    let m = |name: &str| select_map(name, &catalog());
    assert_eq!(
        m("Generic 8 Knob 8 Pad:Generic 8 Knob 8 Pad MIDI 1 20:0"),
        MapChoice::Match("pads".into())
    );
    assert_eq!(
        m("2- Generic 4 Channel Mixer"),
        MapChoice::Match("mixer".into())
    );
    assert_eq!(
        m("GENERIC 4 CHANNEL MIXER In"),
        MapChoice::Match("mixer".into())
    );
    assert_eq!(
        m("Acme Deck Pro MIDI 1"),
        MapChoice::Match("acme-pro".into())
    );
}

#[test]
fn selection_prefers_the_longest_hint_and_reports_ties() {
    assert_eq!(
        select_map("Acme Deck MIDI 1", &catalog()),
        MapChoice::Match("acme".into())
    );
    let twins = candidates(&[("a", "Twin Pad"), ("b", "twin   pad")]);
    assert_eq!(
        select_map("Twin Pad", &twins),
        MapChoice::Ambiguous(vec!["a".into(), "b".into()])
    );
}

#[test]
fn selection_reports_no_match_for_unknown_partial_and_empty_names() {
    for name in [
        "Midi Through 14:0",
        "Generic 8 Knob",
        "Acme Decks",
        "",
        "MIDI",
    ] {
        assert_eq!(select_map(name, &catalog()), MapChoice::NoMatch, "{name}");
    }
    assert_eq!(select_map("anything", &[]), MapChoice::NoMatch);
}

#[test]
fn selection_works_from_the_shipped_maps() {
    let cands: Vec<MapCandidate> = map_files(&shipped_dir())
        .iter()
        .map(|p| MapCandidate {
            id: p.file_name().unwrap().to_string_lossy().into_owned(),
            device: MidiMap::parse(&fs::read_to_string(p).unwrap())
                .unwrap()
                .device,
        })
        .collect();
    assert_eq!(
        select_map("Generic 4 Channel Mixer MIDI 1", &cands),
        MapChoice::Match("generic_2deck_4ch_mixer.json".into())
    );
}

/// Replays `trace` through `map` and returns the commands the mapper sent.
fn commands_from(map: &str, trace: &str) -> Vec<Command> {
    let target = Arc::new(RecordingTarget::default());
    let mapper = MidiMapper::new();
    mapper.set_target(target.clone());
    mapper.load_from_file(&shipped_dir().join(map)).unwrap();
    let trace = MidiTrace::from_file(&data_dir().join(trace)).unwrap();
    replay_trace(&trace, &mapper);
    target
        .changes()
        .iter()
        .map(|c| Command::param(c.id.parse::<ParamId>().unwrap(), c.value).unwrap())
        .collect()
}

fn param(id: &str, value: f64) -> Command {
    Command::param(id.parse().unwrap(), value).unwrap()
}

#[test]
fn generic_mixer_trace_replays_to_the_expected_commands() {
    let got = commands_from("generic_2deck_4ch_mixer.json", "generic_mixer_trace.json");
    let want = vec![
        param("mixer.ch0.fader", 1.0),
        param("mixer.ch1.fader", 0.0),
        param("mixer.ch0.pfl", 1.0),
        param("mixer.ch0.pfl", 0.0),
        param("deck.0.play", 1.0),
        param("deck.0.play", 0.0),
        param("deck.1.rate", 1.08),
        param("deck.1.rate", 0.92),
        param("mixer.crossfader", 1.0),
    ];
    assert_eq!(got, want);
}

#[test]
fn generic_pads_trace_replays_to_the_expected_commands() {
    let got = commands_from("generic_8knob_8pad.json", "generic_pads_trace.json");
    let want = vec![
        param("mixer.ch0.filter", 0.0),
        param("mixer.ch0.filter", 1.0),
        param("mixer.crossfader", 0.0),
        param("sampler.pad0.trigger", 1.0),
        param("sampler.pad0.trigger", 0.0),
        param("sampler.pad7.trigger", 1.0),
        param("sampler.pad7.trigger", 0.0),
    ];
    assert_eq!(got, want);
}
