//! Sample bank: round trip, missing files, corrupt and newer files, imports (copy, clash, decode
//! failure) and hostile pad numbers. Decodes run on the bank's worker; `settle` waits for them.

use om_engine::sampler::PadMode;
use om_engine::DjEngine;
use om_testing::fs::TempDir;
use om_testing::wav::write_float_wav;
use om_ui::sample_bank::{self, BankError, SampleBank, BANK_FILE};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(10);

struct Rig {
    _tmp: TempDir,
    root: PathBuf,
    engine: Arc<DjEngine>,
}

impl Rig {
    fn new() -> Self {
        let tmp = TempDir::new("sample-bank");
        let root = tmp.path().to_path_buf();
        let (engine, _audio) = DjEngine::new(2);
        Self {
            _tmp: tmp,
            root,
            engine: Arc::new(engine),
        }
    }

    fn samples(&self) -> PathBuf {
        self.root.join("samples")
    }

    fn open(&self) -> SampleBank {
        let sink = self.engine.command_sink(None);
        SampleBank::open(self.samples(), Arc::clone(&self.engine), sink)
    }

    /// A WAV of `frames` frames at the engine rate, outside the bank folder.
    fn wav(&self, name: &str, value: f32) -> PathBuf {
        let dir = self.root.join("incoming");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let rate = self.engine.sampler().engine_rate() as u32;
        let mono = vec![value; 2_000];
        write_float_wav(&path, 1, rate, &mono);
        path
    }
}

fn settle(bank: &SampleBank) {
    bank.settle(WAIT);
}

#[test]
fn round_trip_keeps_entries_and_loads_them_on_the_next_start() {
    let rig = Rig::new();
    let bank = rig.open();
    let handle = bank.import_file(2, &rig.wav("kick.wav", 0.5)).unwrap();
    assert!(handle.wait(WAIT));
    settle(&bank);
    assert!(rig.engine.snapshot().sampler.pads[2].loaded);
    bank.set_settings(2, 0.5, 2.0, PadMode::Gate).unwrap();
    bank.save().unwrap();

    let reopened = rig.open();
    let entry = reopened.entry(2).expect("pad 2 kept");
    assert_eq!(entry.file, "kick.wav");
    assert_eq!(entry.name, "kick");
    assert_eq!(entry.gain, 0.5);
    assert_eq!(entry.pitch, 2.0);
    assert_eq!(entry.mode, PadMode::Gate);
    assert!(reopened.take_notes().is_empty());
    assert_eq!(reopened.apply(), 1);
    settle(&reopened);
    let pad = rig.engine.snapshot().sampler.pads[2];
    assert!(pad.loaded);
    assert_eq!(pad.mode, PadMode::Gate);
    assert_eq!(pad.gain, 0.5);
}

#[test]
fn a_missing_file_leaves_the_pad_empty_with_a_note() {
    let rig = Rig::new();
    fs::create_dir_all(rig.samples()).unwrap();
    fs::write(
        rig.samples().join(BANK_FILE),
        r#"{"version":1,"pads":[{"pad":4,"file":"gone.wav","name":"Gone"}]}"#,
    )
    .unwrap();
    let bank = rig.open();
    assert_eq!(bank.apply(), 0, "nothing to decode");
    let notes = bank.take_notes();
    assert!(
        notes.iter().any(|n| n.contains("gone.wav is missing")),
        "{notes:?}"
    );
    assert!(bank.is_missing(4));
    assert!(!rig.engine.snapshot().sampler.pads[4].loaded);
    // The entry stays on disk so the file can come back.
    assert!(bank.entry(4).is_some());
}

#[test]
fn bad_json_is_kept_as_bad_and_the_bank_starts_empty() {
    let rig = Rig::new();
    fs::create_dir_all(rig.samples()).unwrap();
    let path = rig.samples().join(BANK_FILE);
    fs::write(&path, "{not json").unwrap();
    let bank = rig.open();
    assert!(bank.pads().iter().all(Option::is_none));
    assert!(rig.samples().join("sampler.json.bad").is_file());
    let notes = bank.take_notes();
    assert!(
        notes.iter().any(|n| n.contains("could not be read")),
        "{notes:?}"
    );
    // A save replaces the broken file.
    bank.import_file(0, &rig.wav("a.wav", 0.1)).unwrap();
    settle(&bank);
    bank.save().unwrap();
    assert!(parse_ok(&path));
}

#[test]
fn a_newer_version_is_refused_and_left_alone() {
    let rig = Rig::new();
    fs::create_dir_all(rig.samples()).unwrap();
    let path = rig.samples().join(BANK_FILE);
    let text = r#"{"version":2,"pads":[]}"#;
    fs::write(&path, text).unwrap();
    let bank = rig.open();
    assert!(matches!(bank.save(), Err(BankError::NotWritable)));
    assert!(matches!(
        bank.import_file(0, &rig.wav("x.wav", 0.1)),
        Err(BankError::NotWritable)
    ));
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
}

#[test]
fn import_copies_the_file_into_the_bank_folder() {
    let rig = Rig::new();
    let bank = rig.open();
    let source = rig.wav("snare.wav", 0.3);
    bank.import_file(1, &source).unwrap().wait(WAIT);
    settle(&bank);
    assert!(rig.samples().join("snare.wav").is_file());
    assert_eq!(bank.entry(1).unwrap().file, "snare.wav");
    assert!(rig.engine.snapshot().sampler.pads[1].loaded);
}

#[test]
fn a_clashing_name_gets_a_numbered_copy() {
    let rig = Rig::new();
    let bank = rig.open();
    let first = rig.wav("hat.wav", 0.2);
    bank.import_file(0, &first).unwrap().wait(WAIT);
    settle(&bank);
    // A different file with the same name, from another folder.
    let other = rig.root.join("other");
    fs::create_dir_all(&other).unwrap();
    let second = other.join("hat.wav");
    write_float_wav(&second, 1, 44_100, &[0.9; 500]);
    bank.import_file(3, &second).unwrap().wait(WAIT);
    settle(&bank);
    assert!(rig.samples().join("hat.wav").is_file());
    assert!(rig.samples().join("hat-2.wav").is_file());
    assert_eq!(bank.entry(3).unwrap().file, "hat-2.wav");
}

#[test]
fn a_decode_failure_keeps_the_previous_sample() {
    let rig = Rig::new();
    let bank = rig.open();
    bank.import_file(5, &rig.wav("good.wav", 0.4))
        .unwrap()
        .wait(WAIT);
    settle(&bank);
    let before = rig.engine.snapshot().sampler.pads[5];
    assert!(before.loaded);

    let broken = rig.root.join("incoming").join("broken.wav");
    fs::create_dir_all(broken.parent().unwrap()).unwrap();
    fs::write(&broken, b"this is not audio").unwrap();
    bank.import_file(5, &broken).unwrap().wait(WAIT);
    assert_eq!(bank.settle(WAIT), 0);

    let after = rig.engine.snapshot().sampler.pads[5];
    assert_eq!(after.name_hash, before.name_hash);
    assert_eq!(bank.entry(5).unwrap().file, "good.wav");
    assert!(
        !rig.samples().join("broken.wav").exists(),
        "a bad file is not copied in"
    );
    let notes = bank.take_notes();
    assert!(
        notes.iter().any(|n| n.contains("could not be loaded")),
        "{notes:?}"
    );
}

#[test]
fn hostile_pad_numbers_are_refused_or_skipped() {
    let rig = Rig::new();
    let bank = rig.open();
    let source = rig.wav("ok.wav", 0.1);
    assert!(matches!(
        bank.import_file(8, &source),
        Err(BankError::BadPad(8))
    ));
    assert!(matches!(
        bank.set_settings(9, 1.0, 0.0, PadMode::OneShot),
        Err(BankError::BadPad(9))
    ));
    assert!(matches!(bank.clear_pad(255), Err(BankError::BadPad(255))));

    let parsed = sample_bank::parse(
        r#"{"version":1,"pads":[
            {"pad":-1,"file":"a.wav"},
            {"pad":8,"file":"b.wav"},
            {"pad":"3","file":"c.wav"},
            {"pad":1.5,"file":"d.wav"},
            {"pad":2,"file":"e.wav","mode":"strobe","gain":9,"pitch":"high"},
            {"pad":2,"file":"f.wav"},
            {"pad":7}
        ]}"#,
    )
    .unwrap();
    assert!([0, 1, 3, 4, 5, 6, 7]
        .iter()
        .all(|&p| parsed.pads[p].is_none()));
    let entry = parsed.pads[2]
        .as_ref()
        .expect("pad 2 kept from its first entry");
    assert_eq!(entry.file, "e.wav");
    assert_eq!(entry.mode, PadMode::OneShot, "unknown mode falls back");
    assert_eq!(entry.gain, 1.5, "gain clamped to the pad maximum");
    assert_eq!(entry.pitch, 0.0, "a string pitch falls back");
    assert_eq!(parsed.issues.len(), 8, "{:?}", parsed.issues);
    assert!(parsed.pads[7].is_none());
}

#[test]
fn a_newer_version_in_the_parser_and_non_objects_are_errors() {
    assert!(matches!(
        sample_bank::parse(r#"{"version":9}"#),
        Err(BankError::UnsupportedVersion(9))
    ));
    assert!(matches!(
        sample_bank::parse("[1,2]"),
        Err(BankError::NotAnObject)
    ));
    assert!(matches!(
        sample_bank::parse("nope"),
        Err(BankError::Json(_))
    ));
}

#[test]
fn clear_pad_empties_the_pad_and_drops_the_entry() {
    let rig = Rig::new();
    let bank = rig.open();
    bank.import_file(6, &rig.wav("pad6.wav", 0.6))
        .unwrap()
        .wait(WAIT);
    settle(&bank);
    bank.clear_pad(6).unwrap();
    assert!(bank.entry(6).is_none());
    assert!(!rig.engine.snapshot().sampler.pads[6].loaded);
    assert!(rig.samples().join("pad6.wav").is_file(), "the file stays");
    let reopened = rig.open();
    assert!(reopened.entry(6).is_none());
}

#[test]
fn to_json_writes_version_one_and_only_filled_pads() {
    let mut pads: sample_bank::BankPads = Default::default();
    pads[3] = Some(sample_bank::PadEntry::new("x.wav", "x"));
    let text = sample_bank::to_json(&pads);
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["version"], 1);
    let list = value["pads"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["pad"], 3);
    assert_eq!(list[0]["mode"], "oneShot");
}

fn parse_ok(path: &Path) -> bool {
    let text = fs::read_to_string(path).unwrap();
    sample_bank::parse(&text).is_ok_and(|p| p.issues.is_empty())
}
