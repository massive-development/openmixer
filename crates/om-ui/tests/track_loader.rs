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
use om_ui::widgets::deck::format_key;

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
        let mut services = make_services(memory_options());
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
    assert!(format_key(&info).contains("8A"));
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
    assert!(rig.services.library().tracks().plays_for(id).unwrap().is_empty());

    rig.loader
        .observe_playback(&rig.engine, rig.services.library(), Duration::from_secs(1));
    rig.loader
        .observe_playback(&rig.engine, rig.services.library(), Duration::from_secs(5));
    let plays = rig.services.library().tracks().plays_for(id).unwrap();
    assert_eq!(plays.len(), 1, "one play per load");
    assert_eq!(plays[0].deck, Some(0));
    let track = rig.services.library().tracks().find_by_id(id).unwrap().unwrap();
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
    assert!(rig.services.library().tracks().plays_for(id).unwrap().is_empty());
}
