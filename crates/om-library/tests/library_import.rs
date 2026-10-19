//! Port of tests/unit/LibraryImportTest.cpp: iTunes / Rekordbox importers, merge policy and
//! transactionality. FileUrl decoding is covered by the unit tests in `src/file_url.rs`.
//! Fixtures are read from tests/data/import at the workspace root.

mod common;

use common::{write_wav, Tags, TempDir};
use om_analysis::MusicalKey;
use om_library::{
    core_migrations, import_cues, import_library_file, merge_track_record, normalise_key,
    read_xml_file, CueKind, ImportReport, ImportedCue, ImportedLibrary, ImportedTrack,
    ItunesImporter, Ingestor, IngestSummary, Library, LibraryImporter, LibraryImporterRegistry,
    MetadataReaderRegistry, OpenMode, Playlist, RekordboxImporter, SearchEngine, TrackRecord,
    ASSUMED_SAMPLE_RATE, FIRST_MEMORY_SLOT,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const PULSE_ONE: &str = "/music/Techno/Pulse One.flac";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/data/import")
        .join(name)
}

fn memory_library() -> Library {
    Library::open(":memory:", OpenMode::ReadWrite, core_migrations()).expect("open :memory:")
}

fn parse_with(importer: &dyn LibraryImporter, name: &str, report: &mut ImportReport) -> ImportedLibrary {
    let root = read_xml_file(&fixture(name)).expect("fixture parses");
    assert!(importer.recognises(&root));
    let mut data = ImportedLibrary::default();
    importer.parse(&root, &mut data, report).expect("parse");
    data
}

fn run_import(lib: &Library, name: &str, report: &mut ImportReport) -> om_library::Result<()> {
    import_library_file(
        lib,
        &LibraryImporterRegistry::with_defaults(),
        &fixture(name),
        report,
    )
}

fn child_named(lib: &Library, parent: Option<i64>, name: &str) -> Option<Playlist> {
    lib.playlists()
        .children(parent)
        .unwrap()
        .into_iter()
        .find(|p| p.name == name)
}

fn entry_titles(lib: &Library, playlist_id: i64) -> Vec<String> {
    lib.playlists()
        .entries(playlist_id)
        .unwrap()
        .iter()
        .map(|e| lib.tracks().find_by_id(e.track_id).unwrap().unwrap().title)
        .collect()
}

#[test]
fn itunes_parse_tracks_metadata_and_skips() {
    let mut report = ImportReport::default();
    let data = parse_with(&ItunesImporter, "itunes_small.xml", &mut report);
    assert_eq!(data.tracks.len(), 2);
    assert_eq!(report.tracks_skipped, 2); // URL track and the one without a location
    let t = &data.tracks[0].record;
    assert_eq!(t.path, "/music/House/Night Drive & Dawn+Mix.mp3");
    assert_eq!(t.title, "Night Drive & Dawn");
    assert_eq!(t.rating, 4);
    assert_eq!(t.play_count, 7);
    assert_eq!(t.year, 2019);
    assert_eq!(t.duration_sec, 215.5);
    assert_eq!(t.bpm, Some(124.0));
    assert!(t.date_added_ms > 0);
    assert!(t.last_played_ms.is_some());
    assert_eq!(data.tracks[1].record.rating, 0); // computed rating is not user intent
    assert_eq!(data.tracks[1].record.path, "/music/Caf\u{e9} Loop.wav");
}

#[test]
fn itunes_parse_playlist_tree_skips_master_and_keeps_folders() {
    let mut report = ImportReport::default();
    let data = parse_with(&ItunesImporter, "itunes_small.xml", &mut report);
    assert_eq!(data.playlists.len(), 2);
    assert_eq!(data.playlists[0].name, "Sets");
    assert!(data.playlists[0].is_folder);
    assert_eq!(data.playlists[0].children.len(), 1);
    assert_eq!(data.playlists[0].children[0].name, "Warm Up");
    assert_eq!(data.playlists[0].children[0].track_keys.len(), 3);
    assert_eq!(data.playlists[1].name, "Favourites");
}

#[test]
fn importers_recognise_only_their_own_format() {
    let rb = read_xml_file(&fixture("rekordbox_small.xml")).unwrap();
    let it = read_xml_file(&fixture("itunes_small.xml")).unwrap();
    assert!(!ItunesImporter.recognises(&rb));
    assert!(!RekordboxImporter.recognises(&it));
}

#[test]
fn rekordbox_parse_track_fields_tempo_and_marks() {
    let mut report = ImportReport::default();
    let data = parse_with(&RekordboxImporter, "rekordbox_small.xml", &mut report);
    assert_eq!(data.tracks.len(), 2);
    assert_eq!(report.tracks_skipped, 1);
    let t = &data.tracks[0];
    assert_eq!(t.record.path, PULSE_ONE);
    assert_eq!(t.record.rating, 3);
    assert_eq!(t.record.key, "Am");
    assert_eq!(t.record.bpm, Some(128.0));
    assert_eq!(t.sample_rate, 48000.0);
    assert!(t.record.date_added_ms > 0);
    assert_eq!(t.cues.len(), 5); // the fade mark is dropped
    assert_eq!(t.cues[0].colour_argb, 0xff28_e214);
    assert_eq!(data.tracks[1].record.bpm, Some(130.5)); // falls back to TEMPO
    assert_eq!(data.tracks[1].record.rating, 5);
}

#[test]
fn rekordbox_parse_playlist_tree_and_location_keys() {
    let mut report = ImportReport::default();
    let data = parse_with(&RekordboxImporter, "rekordbox_small.xml", &mut report);
    assert_eq!(data.playlists.len(), 2); // ROOT itself is not imported
    assert!(data.playlists[0].is_folder);
    assert_eq!(data.playlists[0].children.len(), 1);
    assert_eq!(data.playlists[0].children[0].track_keys.len(), 3);
    assert_eq!(data.playlists[1].track_keys.len(), 1);
    assert_eq!(data.playlists[1].track_keys[0], "1"); // KeyType 1 resolved through Location
}

#[test]
fn itunes_writes_tracks_playlists_and_report() {
    let lib = memory_library();
    let mut report = ImportReport::default();
    run_import(&lib, "itunes_small.xml", &mut report).unwrap();
    assert_eq!(report.tracks_added, 2);
    assert_eq!(report.tracks_merged, 0);
    assert_eq!(report.tracks_skipped, 2);
    assert_eq!(report.playlists_created, 3);
    assert_eq!(report.entries_added, 3);
    assert_eq!(report.entries_skipped, 1); // the remote track in "Warm Up"
    assert_eq!(report.errors, 0);
    assert_eq!(lib.tracks().count().unwrap(), 2);

    let sets = child_named(&lib, None, "Sets").expect("Sets folder");
    assert!(sets.is_folder);
    let warm = child_named(&lib, Some(sets.id), "Warm Up").expect("Warm Up playlist");
    assert_eq!(
        entry_titles(&lib, warm.id),
        vec!["Caf\u{e9} Loop".to_owned(), "Night Drive & Dawn".to_owned()]
    );
}

#[test]
fn rekordbox_cues_are_stored_in_frames_at_the_track_sample_rate() {
    let lib = memory_library();
    let mut report = ImportReport::default();
    run_import(&lib, "rekordbox_small.xml", &mut report).unwrap();
    assert_eq!(report.tracks_added, 2);
    assert_eq!(report.errors, 1); // loop with end before start
    assert_eq!(report.cues_imported, 5);

    let id = lib.tracks().find_by_path(PULSE_ONE).unwrap().unwrap().id;
    let cues = lib.cues().cues(id).unwrap();
    assert_eq!(cues.len(), 3);
    assert_eq!(cues[0].slot, 0);
    assert_eq!(cues[0].position_frames, 48000);
    assert_eq!(cues[0].label, "Intro");
    assert_eq!(cues[1].position_frames, 1_560_000); // 32.5 s * 48000
    assert_eq!(cues[2].slot, FIRST_MEMORY_SLOT); // memory cue
    assert_eq!(cues[2].position_frames, 64 * 48000);
    let loops = lib.cues().loops(id).unwrap();
    assert_eq!(loops.len(), 2);
    assert_eq!(loops[0].slot, 2);
    assert_eq!(loops[0].end_frames, 100 * 48000);
    assert_eq!(loops[1].slot, FIRST_MEMORY_SLOT);
}

#[test]
fn rekordbox_unknown_sample_rate_assumes_44100() {
    let lib = memory_library();
    let mut report = ImportReport::default();
    run_import(&lib, "rekordbox_small.xml", &mut report).unwrap();
    #[cfg(not(windows))]
    {
        let two = lib.tracks().find_by_path("C:/Users/dj/Music/Pulse Two.wav").unwrap();
        assert_eq!(two.expect("Pulse Two imported").title, "Pulse Two");
    }
    let mut t = ImportedTrack::default();
    t.record.path = "/x".into();
    t.cues.push(ImportedCue {
        kind: CueKind::Cue,
        num: 0,
        start_sec: 2.0,
        ..ImportedCue::default()
    });
    let id = lib.tracks().insert(&t.record).unwrap();
    let mut r = ImportReport::default();
    import_cues(&lib.cues(), id, &t, &mut r).unwrap();
    assert_eq!(lib.cues().cues(id).unwrap()[0].position_frames, 88_200);
    assert_eq!(ASSUMED_SAMPLE_RATE, 44_100.0);
}

#[test]
fn rekordbox_playlists_folder_and_missing_track_entry() {
    let lib = memory_library();
    let mut report = ImportReport::default();
    run_import(&lib, "rekordbox_small.xml", &mut report).unwrap();
    assert_eq!(report.playlists_created, 3);
    assert_eq!(report.entries_added, 3); // peak time: 2 (track 3 skipped), by location: 1
    assert_eq!(report.entries_skipped, 1);
    let club = child_named(&lib, None, "Club").expect("Club folder");
    let peak = child_named(&lib, Some(club.id), "Peak Time").expect("Peak Time playlist");
    assert_eq!(
        entry_titles(&lib, peak.id),
        vec!["Pulse Two".to_owned(), "Pulse One".to_owned()]
    );
}

#[test]
fn reimport_adds_nothing() {
    let lib = memory_library();
    let mut first = ImportReport::default();
    let mut second = ImportReport::default();
    run_import(&lib, "rekordbox_small.xml", &mut first).unwrap();
    run_import(&lib, "rekordbox_small.xml", &mut second).unwrap();
    assert_eq!(second.tracks_added, 0);
    assert_eq!(second.tracks_merged, 2);
    assert_eq!(second.cues_imported, 0);
    assert_eq!(second.playlists_created, 0);
    assert_eq!(second.entries_added, 0);
    assert_eq!(lib.tracks().count().unwrap(), 2);
    let id = lib.tracks().find_by_path(PULSE_ONE).unwrap().unwrap().id;
    assert_eq!(lib.cues().cues(id).unwrap().len(), 3);
}

#[test]
fn merge_fills_gaps_and_local_data_wins() {
    let lib = memory_library();
    let local = TrackRecord {
        path: PULSE_ONE.into(),
        content_hash: "abc".into(),
        title: "My Title".into(), // set locally: kept
        rating: 0,               // default: import may fill
        play_count: 9,           // set locally: kept
        date_added_ms: 123,
        ..TrackRecord::default()
    };
    let id = lib.tracks().insert(&local).unwrap();
    lib.cues()
        .set_cue(&om_library::CuePoint {
            track_id: id,
            slot: 0,
            position_frames: 777,
            colour_argb: 0,
            label: "mine".into(),
        })
        .unwrap(); // slot 0 taken locally

    let mut report = ImportReport::default();
    run_import(&lib, "rekordbox_small.xml", &mut report).unwrap();
    assert_eq!(report.tracks_merged, 1);
    assert_eq!(report.tracks_added, 1);

    let merged = lib.tracks().find_by_id(id).unwrap().unwrap();
    assert_eq!(merged.title, "My Title");
    assert_eq!(merged.artist, "Fixture Artist"); // gap filled
    assert_eq!(merged.rating, 3); // default rating filled
    assert_eq!(merged.play_count, 9); // local count kept
    assert_eq!(merged.content_hash, "abc");
    assert_eq!(merged.date_added_ms, 123);
    assert_eq!(merged.bpm, Some(128.0));

    let cues = lib.cues().cues(id).unwrap();
    assert_eq!(cues.len(), 3); // slot 0 kept, slot 1 and memory added
    assert_eq!(cues[0].label, "mine");
    assert_eq!(cues[0].position_frames, 777);
}

#[test]
fn existing_rating_is_not_overwritten() {
    let mut local = TrackRecord {
        rating: 2,
        play_count: 0,
        ..TrackRecord::default()
    };
    let imported = TrackRecord {
        rating: 5,
        play_count: 12,
        ..TrackRecord::default()
    };
    assert!(merge_track_record(&mut local, &imported));
    assert_eq!(local.rating, 2);
    assert_eq!(local.play_count, 12);
    assert!(!merge_track_record(&mut local, &imported));
}

#[test]
fn database_error_rolls_everything_back() {
    let lib = memory_library();
    lib.database().exec("DROP TABLE playlist_entries").unwrap();
    let mut report = ImportReport::default();
    let result = run_import(&lib, "itunes_small.xml", &mut report);
    assert!(result.is_err());
    assert_eq!(lib.tracks().count().unwrap(), 0); // tracks inserted before the failure are gone
    assert!(lib.playlists().children(None).unwrap().is_empty());
    assert_eq!(report.tracks_added, 0);
    assert!(report.errors >= 1);
}

#[test]
fn failed_lookup_is_not_treated_as_new() {
    let lib = memory_library();
    let mut first = ImportReport::default();
    run_import(&lib, "rekordbox_small.xml", &mut first).unwrap();
    lib.database()
        .exec("ALTER TABLE tracks RENAME TO tracks_away")
        .unwrap();
    let mut second = ImportReport::default();
    assert!(run_import(&lib, "rekordbox_small.xml", &mut second).is_err());
    lib.database()
        .exec("ALTER TABLE tracks_away RENAME TO tracks")
        .unwrap();
    assert_eq!(lib.tracks().count().unwrap(), 2); // no duplicates
}

#[test]
fn unrecognised_and_unreadable_files_fail() {
    let lib = memory_library();
    let mut report = ImportReport::default();
    let dir = TempDir::new().unwrap();
    let other = dir.path().join("other.xml");
    std::fs::write(&other, "<something/>").unwrap();
    let registry = LibraryImporterRegistry::with_defaults();
    assert!(import_library_file(&lib, &registry, &other, &mut report).is_err());
    std::fs::write(&other, "<not closed").unwrap();
    assert!(import_library_file(&lib, &registry, &other, &mut report).is_err());
    assert!(import_library_file(&lib, &registry, &dir.path().join("missing.xml"), &mut report).is_err());
    assert_eq!(lib.tracks().count().unwrap(), 0);
}

#[test]
fn later_registration_wins_and_names_are_listed() {
    struct Fake;
    impl LibraryImporter for Fake {
        fn name(&self) -> &str {
            "fake"
        }
        fn recognises(&self, root: &om_library::XmlElement) -> bool {
            root.has_tag_name("DJ_PLAYLISTS")
        }
        fn parse(
            &self,
            _root: &om_library::XmlElement,
            out: &mut ImportedLibrary,
            _report: &mut ImportReport,
        ) -> Result<(), String> {
            let mut track = ImportedTrack::default();
            track.record.path = "/fake".into();
            out.tracks.push(track);
            Ok(())
        }
    }
    let mut registry = LibraryImporterRegistry::with_defaults();
    assert_eq!(registry.names().len(), 2);
    registry.add(Arc::new(Fake));
    let lib = memory_library();
    let mut report = ImportReport::default();
    import_library_file(&lib, &registry, &fixture("rekordbox_small.xml"), &mut report).unwrap();
    assert!(lib.tracks().find_by_path("/fake").unwrap().is_some());
    assert_eq!(lib.tracks().count().unwrap(), 1);
}

#[test]
fn hotcue_wins_over_earlier_memory_cue_at_same_position() {
    let lib = memory_library();
    let mut t = ImportedTrack::default();
    t.record.path = "/x".into();
    t.cues.push(ImportedCue {
        kind: CueKind::Cue,
        num: -1,
        start_sec: 5.0,
        label: "memory".into(),
        ..ImportedCue::default()
    });
    t.cues.push(ImportedCue {
        kind: CueKind::Cue,
        num: 3,
        start_sec: 5.0,
        label: "hot".into(),
        ..ImportedCue::default()
    });
    let id = lib.tracks().insert(&t.record).unwrap();
    let mut r = ImportReport::default();
    import_cues(&lib.cues(), id, &t, &mut r).unwrap();
    let cues = lib.cues().cues(id).unwrap();
    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].slot, 3);
    assert_eq!(cues[0].label, "hot");
}

#[test]
fn playlists_merge_into_same_named_sibling() {
    let lib = memory_library();
    let mine = lib
        .tracks()
        .insert(&TrackRecord {
            path: "/mine.wav".into(),
            title: "Mine".into(),
            ..TrackRecord::default()
        })
        .unwrap();
    let fav = lib.playlists().create("Favourites", None, false).unwrap();
    lib.playlists().add_track(fav, mine, -1).unwrap();
    let mut report = ImportReport::default();
    run_import(&lib, "itunes_small.xml", &mut report).unwrap();
    assert_eq!(report.playlists_created, 2); // Sets + Warm Up; Favourites reused
    assert_eq!(lib.playlists().children(None).unwrap().len(), 2);
    assert_eq!(
        entry_titles(&lib, fav),
        vec!["Mine".to_owned(), "Night Drive & Dawn".to_owned()]
    );
}

#[test]
fn analysis_and_imported_tags_survive_the_first_scan() {
    let tmp = TempDir::new().unwrap();
    let wav = write_wav(&tmp.path().join("Pulse.wav"), 1.0, &Tags::default(), 440.0);
    let wav_str = wav.to_str().unwrap().to_owned();
    let xml = tmp.path().join("rb.xml");
    std::fs::write(
        &xml,
        format!(
            "<?xml version=\"1.0\"?><DJ_PLAYLISTS Version=\"1.0.0\"><COLLECTION Entries=\"1\">\
             <TRACK TrackID=\"1\" Name=\"Imported Title\" Artist=\"Imported Artist\" Album=\"Imported Album\" \
             Genre=\"Techno\" AverageBpm=\"128.00\" Tonality=\"Am\" Location=\"file://localhost{}\"/>\
             </COLLECTION></DJ_PLAYLISTS>",
            wav_str.replace(' ', "%20")
        ),
    )
    .unwrap();
    let lib = memory_library();
    let mut report = ImportReport::default();
    import_library_file(&lib, &LibraryImporterRegistry::with_defaults(), &xml, &mut report).unwrap();

    let readers = MetadataReaderRegistry::with_defaults();
    let ingestor = Ingestor::new(&lib, &readers);
    let mut summary = IngestSummary::default();
    ingestor
        .ingest_folder(tmp.path(), &mut summary, &|| false)
        .unwrap();
    assert_eq!(summary.failed, 0);

    let t = lib.tracks().find_by_path(&wav_str).unwrap().expect("track imported");
    assert_eq!(t.content_hash.len(), 64);
    assert_eq!(t.bpm, Some(128.0));
    assert_eq!(t.key, "Am");
    assert_eq!(t.camelot, "8A");
    assert_eq!(t.title, "Imported Title"); // the filename fallback must not replace it
    assert_eq!(t.artist, "Imported Artist");
    assert_eq!(t.album, "Imported Album");
    assert_eq!(t.genre, "Techno");

    let search = SearchEngine::new(lib.database());
    assert_eq!(search.search_text("key:8A").unwrap().len(), 1);
    assert_eq!(search.search_text("key:Am").unwrap().len(), 1);
}

#[test]
fn merge_derives_camelot_and_normalise_handles_both_forms() {
    let mut local = TrackRecord::default();
    let imported = TrackRecord {
        key: "Dbm".into(),
        ..TrackRecord::default()
    };
    assert!(merge_track_record(&mut local, &imported));
    assert_eq!(local.camelot, "12A");

    let mut by_code = TrackRecord {
        camelot: "8B".into(),
        ..TrackRecord::default()
    };
    normalise_key(&mut by_code);
    assert_eq!(by_code.key, "C");
    let mut junk = TrackRecord {
        key: "??".into(),
        ..TrackRecord::default()
    };
    normalise_key(&mut junk);
    assert_eq!(junk.key, "??");
    assert!(junk.camelot.is_empty());
}

#[test]
fn dao_update_analysis_writes_only_set_values() {
    let lib = memory_library();
    let t = TrackRecord {
        path: "/a.wav".into(),
        title: "A".into(),
        bpm: Some(100.0),
        key: "Am".into(),
        camelot: "8A".into(),
        ..TrackRecord::default()
    };
    let id = lib.tracks().insert(&t).unwrap();
    lib.tracks().update_analysis(id, Some(124.0), None).unwrap();
    let mut r = lib.tracks().find_by_id(id).unwrap().unwrap();
    assert_eq!(r.bpm, Some(124.0));
    assert_eq!(r.camelot, "8A");
    lib.tracks()
        .update_analysis(id, None, Some(&MusicalKey::new(0, false)))
        .unwrap();
    r = lib.tracks().find_by_id(id).unwrap().unwrap();
    assert_eq!(r.bpm, Some(124.0));
    assert_eq!(r.key, "C");
    assert_eq!(r.camelot, "8B");
    assert!(lib.tracks().update_analysis(999, Some(1.0), None).is_err());
}
