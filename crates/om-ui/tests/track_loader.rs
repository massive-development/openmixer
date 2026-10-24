//! Load-to-deck, metadata, cached or queued analysis. The
//! header labels are checked through the loader's track info, which the deck
//! widget reads.

mod library_common;

use std::time::Duration;

use library_common::*;
use om_analysis::hash_file_content;
use om_engine::DjEngine;
use om_library::{PrepStatus, TrackRecord};
use om_testing::wait::{wait_until, DEFAULT_POLL};
use om_ui::library::{LibraryServices, LoadError, TrackLoader};
use om_ui::widgets::deck_header::{format_key, KeyNotation};

struct LoaderRig {
    dir: tempfile::TempDir,
    services: LibraryServices,
    engine: DjEngine,
    // Keeps the audio side alive so the engine's decks stay connected.
    _audio: om_engine::DjEngineAudio,
    loader: TrackLoader,
    counter: u32,
}

impl LoaderRig {
    fn new() -> Self {
        Self::with_options(memory_options())
    }

    fn with_options(options: om_ui::library::Options) -> Self {
        let mut services = make_services(options);
        let (engine, audio) = DjEngine::new(2);
        let loader = TrackLoader::new(&mut services, 2);
        Self {
            dir: tempfile::tempdir().unwrap(),
            services,
            engine,
            _audio: audio,
            loader,
            counter: 0,
        }
    }

    /// A real WAV in the temp dir, registered with the content hash the analysis cache uses.
    fn add_file(&mut self, name: &str, camelot: &str, exists: bool) -> i64 {
        let file = self.dir.path().join(name);
        if exists {
            self.counter += 1;
            write_wav(&file, 2.0, 220.0 + self.counter as f32 * 40.0);
        }
        let track = TrackRecord {
            path: file.to_string_lossy().into_owned(),
            content_hash: if exists {
                hash_file_content(&file).unwrap_or_default()
            } else {
                String::new()
            },
            title: name.to_owned(),
            artist: "Artist".to_owned(),
            camelot: camelot.to_owned(),
            key: "Am".to_owned(),
            ..TrackRecord::default()
        };
        self.services.library().tracks().insert(&track).unwrap()
    }

    /// A normal load: refused while the deck is playing.
    fn load(&mut self, deck: usize, track_id: i64) -> Result<(), LoadError> {
        self.loader
            .load(&self.engine, &mut self.services, deck, track_id, false)
    }

    /// Pumps the services and the loader until the deck has a waveform, or the timeout passes.
    fn wait_for_waveform(&mut self, deck: usize) -> bool {
        wait_until(Duration::from_secs(8), DEFAULT_POLL, || {
            self.services.pump();
            self.loader.pump(&self.engine, &self.services);
            self.loader.waveform(deck).is_some()
        })
    }

    fn loader_hash(&self, track_id: i64) -> String {
        let track = self.services.library().tracks().find_by_id(track_id);
        track.unwrap().unwrap().content_hash
    }

    fn prep_entries(&self) -> Vec<om_library::PrepEntry> {
        self.services.prep_queue().list().unwrap()
    }
}

#[test]
fn loads_the_file_into_the_deck_and_shows_metadata() {
    let mut rig = LoaderRig::new();
    let id = rig.add_file("one.wav", "8A", true);
    assert!(rig.load(0, id).is_ok());
    assert!(rig.engine.deck(0).is_loaded());
    assert!(!rig.engine.deck(1).is_loaded());

    let info = rig.loader.track_info(0);
    assert_eq!(info.title, "one.wav");
    assert_eq!(info.artist, "Artist");
    assert!(format_key(&info, KeyNotation::Camelot) == "8A");
    assert_eq!(rig.loader.loaded_track(0).map(|t| t.id), Some(id));
    assert!(rig.loader.loaded_track(1).is_none());
}

#[test]
fn an_unanalysed_track_is_queued_with_priority_and_filled_in_when_done() {
    let mut rig = LoaderRig::new();
    let first = rig.add_file("a.wav", "8A", true);
    let second = rig.add_file("b.wav", "8A", true);
    rig.services.prep_queue_mut().enqueue(first, false).unwrap(); // a normal item already waiting
    assert!(rig.load(0, second).is_ok());
    let entries = rig.prep_entries();
    assert!(entries.len() >= 2);
    assert!(rig.loader.waveform(0).is_none(), "not there yet");

    assert!(rig.wait_for_waveform(0));
    let grid = rig.engine.deck_control(0).grid().expect("grid applied");
    assert!((grid.bpm() - 128.0).abs() < 1e-6);
    assert_eq!(
        rig.loader.loaded_track(0).and_then(|t| t.bpm),
        Some(128.0),
        "written back"
    );
}

#[test]
fn an_already_analysed_track_loads_immediately_without_queueing() {
    let mut rig = LoaderRig::new();
    let id = rig.add_file("cached.wav", "8A", true);
    assert!(rig.load(0, id).is_ok());
    assert!(rig.wait_for_waveform(0));
    assert!(pump_until(&mut rig.services, |s| {
        s.prep_queue()
            .list()
            .map(|l| l.first().is_some_and(|e| e.status == PrepStatus::Done))
            .unwrap_or(false)
    }));

    // Same track on the other deck: results come from the cache.
    assert!(rig.load(1, id).is_ok());
    assert!(rig.loader.waveform(1).is_some());
    assert!(rig.engine.deck_control(1).grid().is_some());
    assert_eq!(
        rig.prep_entries()[0].status,
        PrepStatus::Done,
        "not re-queued"
    );
}

#[test]
fn loading_another_track_clears_the_old_waveform_and_grid() {
    let mut rig = LoaderRig::new();
    let a = rig.add_file("x.wav", "8A", true);
    assert!(rig.load(0, a).is_ok());
    assert!(rig.wait_for_waveform(0));
    rig.services.prep_queue_mut().cancel_all();
    let b = rig.add_file("y.wav", "8A", true);
    rig.services.prep_queue_mut().cancel_all();

    // y is not analysed: its load must not show x's data.
    assert!(rig.load(0, b).is_ok());
    assert!(rig.loader.waveform(0).is_none());
    assert!(rig.engine.deck_control(0).grid().is_none());
    assert!(rig.wait_for_waveform(0), "and then it does arrive");
}

#[test]
fn missing_files_and_bad_decks_fail_without_touching_the_deck() {
    let mut rig = LoaderRig::new();
    let missing = rig.add_file("gone.wav", "8A", false);
    let error = rig.load(0, missing).expect_err("missing file fails");
    assert!(error.to_string().contains("missing"), "{error}");
    assert!(!rig.engine.deck(0).is_loaded());

    let ok = rig.add_file("ok.wav", "8A", true);
    assert!(rig.load(5, ok).is_err(), "no such deck");
    assert!(rig.load(0, 99_999).is_err(), "unknown track id");
    assert!(rig.prep_entries().is_empty());
}

#[test]
fn suggested_deck_prefers_an_empty_idle_deck() {
    let mut rig = LoaderRig::new();
    assert_eq!(rig.loader.suggested_deck(&rig.engine), 0);
    let p = rig.add_file("p.wav", "8A", true);
    assert!(rig.load(0, p).is_ok());
    assert_eq!(rig.loader.suggested_deck(&rig.engine), 1);
    let q = rig.add_file("q.wav", "9A", true);
    assert!(rig.load(1, q).is_ok());
    assert_eq!(
        rig.loader.suggested_deck(&rig.engine),
        0,
        "both loaded and idle: the first"
    );
    rig.engine.deck(0).set_playing(true);
    assert_eq!(rig.loader.suggested_deck(&rig.engine), 1);
}

#[test]
fn reference_key_follows_the_playing_deck() {
    let mut rig = LoaderRig::new();
    assert_eq!(rig.loader.reference_camelot(&rig.engine), "");
    let p = rig.add_file("p.wav", "8A", true);
    assert!(rig.load(0, p).is_ok());
    let q = rig.add_file("q.wav", "9A", true);
    assert!(rig.load(1, q).is_ok());
    assert_eq!(
        rig.loader.reference_camelot(&rig.engine),
        "8A",
        "nothing playing: first loaded"
    );
    rig.engine.deck(1).set_playing(true);
    assert_eq!(rig.loader.reference_camelot(&rig.engine), "9A");
}

#[test]
fn a_playing_deck_refuses_a_load_unless_forced() {
    let mut rig = LoaderRig::new();
    let first = rig.add_file("first.wav", "8A", true);
    let second = rig.add_file("second.wav", "9A", true);
    rig.load(0, first).expect("first loads");
    rig.engine.deck(0).play();
    assert!(rig.engine.deck(0).is_playing());

    let refused = rig.load(0, second).expect_err("a playing deck refuses");
    assert_eq!(refused.to_string(), "deck is playing");
    assert!(rig.engine.deck(0).is_playing(), "the deck keeps playing");
    assert_eq!(rig.loader.loaded_track(0).map(|t| t.id), Some(first));

    rig.loader
        .load(&rig.engine, &mut rig.services, 0, second, true)
        .expect("an explicit force loads it");
    assert_eq!(rig.loader.loaded_track(0).map(|t| t.id), Some(second));
}

#[test]
fn a_failed_load_leaves_the_loaded_track_and_its_grid_alone() {
    let mut rig = LoaderRig::new();
    let keep = rig.add_file("keep.wav", "8A", true);
    rig.load(0, keep).expect("loads");
    assert!(rig.wait_for_waveform(0));
    let before = rig.engine.deck_control(0).grid().expect("analysed grid");

    let junk = rig.add_file("junk.wav", "8A", true);
    std::fs::write(rig.dir.path().join("junk.wav"), b"not audio").unwrap();
    assert!(rig.load(0, junk).is_err());

    let after = rig.engine.deck_control(0).grid().expect("grid restored");
    assert!((after.bpm() - before.bpm()).abs() < 1e-9);
    assert_eq!(rig.loader.loaded_track(0).map(|t| t.id), Some(keep));
}

fn saved_cues(rig: &LoaderRig, track_id: i64) -> Vec<(i32, i64)> {
    rig.services
        .library()
        .cues()
        .cues(track_id)
        .expect("cues")
        .into_iter()
        .map(|c| (c.slot, c.position_frames))
        .collect()
}

#[test]
fn a_hotcue_set_on_the_deck_is_saved_and_a_clear_deletes_it() {
    let mut rig = LoaderRig::new();
    let id = rig.add_file("cue.wav", "8A", true);
    assert!(rig.load(0, id).is_ok());

    rig.engine.deck(0).set_hotcue(3, 44_100);
    rig.loader.pump(&rig.engine, &rig.services);
    assert_eq!(saved_cues(&rig, id), vec![(3, 44_100)]);

    rig.engine.deck(0).clear_hotcue(3);
    rig.loader.pump(&rig.engine, &rig.services);
    assert!(saved_cues(&rig, id).is_empty());
}

#[test]
fn saved_cues_are_applied_on_load_and_the_outgoing_track_keeps_its_edits() {
    let mut rig = LoaderRig::new();
    let a = rig.add_file("a.wav", "8A", true);
    let b = rig.add_file("b.wav", "8A", true);
    rig.services
        .library()
        .cues()
        .set_cue(&om_library::CuePoint {
            track_id: b,
            slot: 5,
            position_frames: 9_000,
            ..Default::default()
        })
        .unwrap();

    assert!(rig.load(0, a).is_ok());
    assert_eq!(rig.engine.deck(0).hotcue(5), None, "a has no cue 5");
    rig.engine.deck(0).set_hotcue(1, 500);
    // Loading b before any pump: a's edit is written first, then b's cues go in.
    assert!(rig.load(0, b).is_ok());
    assert_eq!(rig.engine.deck(0).hotcue(5), Some(9_000));
    assert_eq!(
        rig.engine.deck(0).hotcue(1),
        None,
        "the bank was cleared by the load"
    );
    assert_eq!(saved_cues(&rig, a), vec![(1, 500)]);

    // And a's cue comes back when a is loaded again.
    assert!(rig.load(0, a).is_ok());
    assert_eq!(rig.engine.deck(0).hotcue(1), Some(500));
    rig.loader.pump(&rig.engine, &rig.services);
    assert_eq!(
        saved_cues(&rig, a),
        vec![(1, 500)],
        "restored, not rewritten"
    );
    assert_eq!(saved_cues(&rig, b), vec![(5, 9_000)], "b untouched");
}

#[test]
fn a_loaded_track_is_played_once_after_thirty_seconds_of_playing() {
    let mut rig = LoaderRig::new();
    let id = rig.add_file("played.wav", "8A", true);
    assert!(rig.load(0, id).is_ok());
    rig.engine.deck(0).set_playing(true);

    for _ in 0..29 {
        rig.loader
            .observe_playback(&rig.engine, rig.services.library(), Duration::from_secs(1));
    }
    assert!(rig
        .services
        .library()
        .tracks()
        .plays_for(id)
        .unwrap()
        .is_empty());

    rig.loader
        .observe_playback(&rig.engine, rig.services.library(), Duration::from_secs(1));
    rig.loader
        .observe_playback(&rig.engine, rig.services.library(), Duration::from_secs(5));
    let plays = rig.services.library().tracks().plays_for(id).unwrap();
    assert_eq!(plays.len(), 1, "one play per load");
    assert_eq!(plays[0].deck, Some(0));
    let track = rig
        .services
        .library()
        .tracks()
        .find_by_id(id)
        .unwrap()
        .unwrap();
    assert_eq!(track.play_count, 1);
    assert!(track.last_played_ms.is_some());
}

#[test]
fn paused_decks_record_no_play() {
    let mut rig = LoaderRig::new();
    let id = rig.add_file("paused.wav", "8A", true);
    assert!(rig.load(1, id).is_ok());
    for _ in 0..60 {
        rig.loader
            .observe_playback(&rig.engine, rig.services.library(), Duration::from_secs(1));
    }
    assert!(rig
        .services
        .library()
        .tracks()
        .plays_for(id)
        .unwrap()
        .is_empty());
}

#[test]
fn rekordbox_imported_hotcues_appear_on_load() {
    let mut rig = LoaderRig::new();
    let wav = rig.dir.path().join("imported.wav");
    write_wav(&wav, 2.0, 220.0);
    let xml = rig.dir.path().join("collection.xml");
    let collection = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
  <PRODUCT Name="test" Version="0" Company="none"/>
  <COLLECTION Entries="1">
    <TRACK TrackID="1" Name="Imported" Artist="Artist" TotalTime="2" SampleRate="44100"
           Location="file://localhost{}">
      <POSITION_MARK Name="Intro" Type="0" Start="1.000" Num="0"/>
      <POSITION_MARK Name="Drop" Type="0" Start="1.500" Num="3"/>
    </TRACK>
  </COLLECTION>
  <PLAYLISTS/>
</DJ_PLAYLISTS>"#,
        wav.display()
    );
    std::fs::write(&xml, collection).unwrap();
    rig.services.import_file(&xml).expect("import");
    let id = rig
        .services
        .library()
        .tracks()
        .find_by_path(&wav.to_string_lossy())
        .unwrap()
        .expect("imported row")
        .id;

    assert!(rig.load(0, id).is_ok());
    assert_eq!(
        rig.engine.deck(0).hotcue(0),
        Some(44_100),
        "1.0 s at 44.1 kHz"
    );
    assert_eq!(
        rig.engine.deck(0).hotcue(3),
        Some(66_150),
        "1.5 s at 44.1 kHz"
    );
    assert_eq!(rig.engine.deck(0).hotcue(1), None);
}

#[test]
fn a_library_file_loaded_by_path_loads_as_its_track() {
    let mut rig = LoaderRig::new();
    let id = rig.add_file("known.wav", "8A", true);
    let path = rig.dir.path().join("known.wav");
    rig.loader
        .load_path(&rig.engine, &mut rig.services, 0, &path, false)
        .expect("a library file loads by path");
    assert!(rig.engine.deck(0).is_loaded());
    assert_eq!(rig.loader.loaded_track(0).map(|t| t.id), Some(id));
    assert_eq!(rig.loader.track_info(0).artist, "Artist");
}

#[test]
fn a_file_outside_the_library_plays_without_metadata_and_clears_the_old_track() {
    let mut rig = LoaderRig::new();
    let first = rig.add_file("first.wav", "8A", true);
    rig.load(0, first).expect("loads");
    assert!(rig.loader.loaded_track(0).is_some());

    let loose = rig.dir.path().join("loose.wav");
    write_wav(&loose, 1.0, 330.0);
    rig.loader
        .load_path(&rig.engine, &mut rig.services, 0, &loose, false)
        .expect("a loose file loads");
    assert!(rig.engine.deck(0).is_loaded());
    assert!(rig.loader.loaded_track(0).is_none(), "no library metadata");
    assert!(rig.loader.waveform(0).is_none());
    assert!(rig.engine.deck_control(0).grid().is_none());
}

#[test]
fn load_by_path_refuses_a_playing_deck_unless_forced() {
    let mut rig = LoaderRig::new();
    let first = rig.add_file("first.wav", "8A", true);
    rig.load(0, first).expect("loads");
    rig.engine.deck(0).play();

    let loose = rig.dir.path().join("loose.wav");
    write_wav(&loose, 1.0, 330.0);
    let refused = rig
        .loader
        .load_path(&rig.engine, &mut rig.services, 0, &loose, false)
        .expect_err("a playing deck refuses");
    assert_eq!(refused.to_string(), "deck is playing");
    assert!(rig.engine.deck(0).is_playing());
    assert_eq!(rig.loader.loaded_track(0).map(|t| t.id), Some(first));

    rig.loader
        .load_path(&rig.engine, &mut rig.services, 0, &loose, true)
        .expect("a forced load by path goes through");
    assert!(rig.loader.loaded_track(0).is_none());
}

#[test]
fn load_by_path_reports_missing_files_and_bad_decks() {
    let mut rig = LoaderRig::new();
    let missing = rig.dir.path().join("nope.wav");
    let err = rig
        .loader
        .load_path(&rig.engine, &mut rig.services, 0, &missing, false)
        .expect_err("missing file");
    assert_eq!(err.to_string(), "file is missing");

    let loose = rig.dir.path().join("loose.wav");
    write_wav(&loose, 1.0, 330.0);
    let err = rig
        .loader
        .load_path(&rig.engine, &mut rig.services, 9, &loose, false)
        .expect_err("no such deck");
    assert_eq!(err.to_string(), "no such deck");
    assert!(!rig.engine.deck(0).is_loaded());
}

/// A complete four-stem set for `track_id` in the rig's cache, registered as the track's row.
/// `corrupt` writes a garbage `vocals.wav` instead of a stereo file.
fn cache_stems(rig: &LoaderRig, track_id: i64, corrupt: bool) -> std::path::PathBuf {
    let dir_name = format!("cache-{track_id}-htdemucs-v1");
    let dir = rig.dir.path().join("stems").join(&dir_name);
    std::fs::create_dir_all(&dir).unwrap();
    for (i, name) in ["drums", "bass", "vocals", "other"].iter().enumerate() {
        let file = dir.join(format!("{name}.wav"));
        if corrupt && *name == "vocals" {
            std::fs::write(&file, b"not a wav").unwrap();
        } else {
            stereo_stem(&file, 110.0 * (i + 1) as f64);
        }
    }
    rig.services
        .library()
        .track_stems()
        .set(&om_library::TrackStemsRecord {
            track_id,
            sep_id: "htdemucs".into(),
            version: 1,
            dir: dir_name,
            bytes: 0,
            created_ms: 0,
        })
        .unwrap();
    rig.dir.path().join("stems")
}

/// A stereo file the length of `write_wav`'s two-second clip, so the stems match the main file.
fn stereo_stem(path: &std::path::Path, freq: f64) {
    let frames = 2 * 44_100;
    let mono = om_testing::signals::sine_amp(freq, 0.25, 44_100.0, frames);
    let samples: Vec<i16> = mono
        .iter()
        .flat_map(|s| {
            let v = (s * 32767.0) as i16;
            [v, v]
        })
        .collect();
    om_testing::wav::write_pcm16_wav(path, 2, 44_100, &samples);
}

fn source(root: std::path::PathBuf) -> Option<om_ui::library::StemSource> {
    Some(om_ui::library::StemSource {
        root,
        sep_id: "htdemucs".into(),
        version: 1,
    })
}

#[test]
fn cached_stems_load_with_the_track_when_auto_load_is_on() {
    let mut rig = LoaderRig::new();
    let track = rig.add_file("song.wav", "8A", true);
    let root = cache_stems(&rig, track, false);
    rig.loader.configure_stems(source(root), true, None);
    rig.load(0, track).expect("loads");
    assert!(rig.engine.deck(0).has_stems());
    assert_eq!(rig.loader.stem_note(0), None);
}

#[test]
fn auto_load_off_leaves_the_track_plain_even_with_a_cache() {
    let mut rig = LoaderRig::new();
    let track = rig.add_file("song.wav", "8A", true);
    let root = cache_stems(&rig, track, false);
    rig.loader.configure_stems(source(root), false, None);
    rig.load(0, track).expect("loads");
    assert!(!rig.engine.deck(0).has_stems());
}

#[test]
fn toggle_stems_loads_and_clears_while_stopped_and_refuses_while_playing() {
    let mut rig = LoaderRig::new();
    let track = rig.add_file("song.wav", "8A", true);
    let root = cache_stems(&rig, track, false);
    rig.loader.configure_stems(source(root), false, None);
    rig.load(0, track).expect("loads");

    let on = rig
        .loader
        .toggle_stems(&rig.engine, &rig.services, 0)
        .expect("stopped deck loads stems");
    assert!(on && rig.engine.deck(0).has_stems());

    rig.engine.deck(0).play();
    let refused = rig
        .loader
        .toggle_stems(&rig.engine, &rig.services, 0)
        .expect_err("a playing deck does not switch mode");
    assert_eq!(refused.to_string(), "stop the deck to switch stems");
    assert!(rig.engine.deck(0).has_stems(), "the mode is unchanged");

    rig.engine.deck(0).pause();
    assert!(!rig
        .loader
        .toggle_stems(&rig.engine, &rig.services, 0)
        .unwrap());
    assert!(!rig.engine.deck(0).has_stems());
}

#[test]
fn a_track_without_a_cached_set_has_no_stems_to_toggle() {
    let mut rig = LoaderRig::new();
    let track = rig.add_file("song.wav", "8A", true);
    rig.loader
        .configure_stems(source(rig.dir.path().join("stems")), true, None);
    rig.load(0, track).expect("loads");
    let err = rig
        .loader
        .toggle_stems(&rig.engine, &rig.services, 0)
        .expect_err("no cache row");
    assert_eq!(err.to_string(), "no stems \u{2014} Prepare stems");
    assert!(!rig.engine.deck(0).has_stems());
}

#[test]
fn a_cache_row_whose_files_are_gone_loads_plain_without_a_note() {
    let mut rig = LoaderRig::new();
    let track = rig.add_file("song.wav", "8A", true);
    let root = cache_stems(&rig, track, false);
    std::fs::remove_dir_all(root.join(format!("cache-{track}-htdemucs-v1"))).unwrap();
    rig.loader.configure_stems(source(root), true, None);
    rig.load(0, track).expect("the track still loads");
    assert!(!rig.engine.deck(0).has_stems());
    assert_eq!(
        rig.loader.stem_note(0),
        None,
        "missing files are not an error"
    );
}

#[test]
fn a_corrupt_stem_file_leaves_the_deck_plain_with_a_status_note() {
    let mut rig = LoaderRig::new();
    let track = rig.add_file("song.wav", "8A", true);
    let root = cache_stems(&rig, track, true);
    rig.loader.configure_stems(source(root), true, None);
    rig.load(0, track).expect("the track loads");
    assert!(!rig.engine.deck(0).has_stems());
    assert!(rig.engine.deck(0).is_loaded());
    assert!(rig.loader.stem_note(0).is_some_and(
        |note| note.starts_with("stems failed:") && note.ends_with("playing the plain track")
    ));
}

#[test]
fn starting_cached_stems_marks_the_entry_as_recently_used() {
    let mut rig = LoaderRig::new();
    let track = rig.add_file("song.wav", "8A", true);
    let root = cache_stems(&rig, track, false);
    let entry = root.join(format!("cache-{track}-htdemucs-v1"));
    let meta = entry.join("meta.json");
    std::fs::write(&meta, "{}").unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
    std::fs::File::options()
        .write(true)
        .open(&meta)
        .unwrap()
        .set_modified(old)
        .unwrap();
    rig.loader.configure_stems(source(root), true, None);
    rig.load(0, track).expect("loads");
    assert!(rig.engine.deck(0).has_stems());
    let mtime = std::fs::metadata(&meta).unwrap().modified().unwrap();
    let age = std::time::SystemTime::now().duration_since(mtime).unwrap();
    assert!(age < std::time::Duration::from_secs(600), "age {age:?}");
}

fn has_spectrum(rig: &LoaderRig, deck: usize) -> Option<bool> {
    rig.loader.waveform(deck).map(|w| w.has_spectrum())
}

#[test]
fn a_format_one_waveform_shows_a_silhouette_and_rebuilds_with_the_spectrum() {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache_path = cache_dir.path().join("analysis.db");
    let mut options = memory_options();
    options.cache_path = cache_path.to_string_lossy().into_owned();
    options.analyzers = Some(registry_with_waveform(Some(std::sync::Arc::new(
        om_analysis::WaveformAnalyzer::default(),
    ))));
    let mut rig = LoaderRig::with_options(options);
    let id = rig.add_file("old.wav", "8A", true);
    let hash = rig.loader_hash(id);

    // The cache holds what the first release wrote: format 1, analyser version 1.
    let v1 = legacy_waveform_json();
    let cache = om_analysis::SqliteResultCache::open(&cache_path.to_string_lossy()).unwrap();
    assert!(om_analysis::ResultCache::store(
        &cache, &hash, "waveform", 1, &v1
    ));

    assert!(rig.load(0, id).is_ok(), "the track loads at once");
    assert_eq!(
        has_spectrum(&rig, 0),
        Some(false),
        "neutral silhouette while it rebuilds"
    );
    assert!(
        wait_until(Duration::from_secs(8), DEFAULT_POLL, || {
            rig.services.pump();
            rig.loader.pump(&rig.engine, &rig.services);
            has_spectrum(&rig, 0) == Some(true)
        }),
        "the rebuilt waveform replaces it"
    );
}
