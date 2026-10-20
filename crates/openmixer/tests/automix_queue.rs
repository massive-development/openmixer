//! The library-backed automix queue over an in-memory library with generated tracks.

use om_automix::queue::{AutomixQueue, QueueConfig, QueueTrack, SkipReason};
use om_library::{core_migrations, Library, OpenMode, TrackRecord};
use openmixer::automix_queue::{LibraryQueue, QueueSource};

const HOUR: i64 = 3_600_000;
const NOW: i64 = 1_000_000_000_000;

fn library() -> Library {
    Library::open(":memory:", OpenMode::ReadWrite, core_migrations()).expect("open")
}

fn add(lib: &Library, n: i32, bpm: Option<f64>, camelot: &str) -> i64 {
    lib.tracks()
        .insert(&TrackRecord {
            path: format!("/music/t{n}.flac"),
            title: format!("Track {n}"),
            artist: "Gen".into(),
            bpm,
            camelot: camelot.into(),
            ..TrackRecord::default()
        })
        .expect("insert")
}

fn queue(lib: Library, source: QueueSource, cfg: QueueConfig, seed: u64) -> LibraryQueue {
    LibraryQueue::new(lib, source, cfg, seed).with_clock(|| NOW)
}

fn ids(q: &mut LibraryQueue, n: usize, cur: Option<&QueueTrack>) -> Vec<i64> {
    (0..n)
        .map_while(|_| q.next_track(cur).map(|t| t.id))
        .collect()
}

#[test]
fn a_playlist_plays_in_order_and_wraps() {
    let lib = library();
    let a = add(&lib, 1, Some(124.0), "8A");
    let b = add(&lib, 2, Some(125.0), "8A");
    let c = add(&lib, 3, Some(126.0), "8A");
    let pl = lib.playlists().create("Set", None, false).unwrap();
    for (i, t) in [b, c, a].into_iter().enumerate() {
        lib.playlists().add_track(pl, t, i as i32).unwrap();
    }
    let mut q = queue(lib, QueueSource::Playlist(pl), QueueConfig::default(), 1);
    assert_eq!(ids(&mut q, 5, None), vec![b, c, a, b, c]);
}

#[test]
fn recent_plays_and_unanalysed_tracks_are_skipped_with_reasons() {
    let lib = library();
    let a = add(&lib, 1, Some(124.0), "8A");
    let b = add(&lib, 2, None, "");
    let c = add(&lib, 3, Some(126.0), "8A");
    let d = add(&lib, 4, Some(127.0), "8A");
    let pl = lib.playlists().create("Set", None, false).unwrap();
    for (i, t) in [a, b, c, d].into_iter().enumerate() {
        lib.playlists().add_track(pl, t, i as i32).unwrap();
    }
    lib.tracks().record_play(a, Some(0), NOW - HOUR).unwrap();
    let mut q = queue(lib, QueueSource::Playlist(pl), QueueConfig::default(), 1);
    assert_eq!(q.next_track(None).unwrap().id, c);
    let skips = q.last_skips();
    assert!(skips.contains(&(a, SkipReason::Recent)));
    assert!(skips.contains(&(b, SkipReason::NotAnalysed)));
    assert_eq!(q.next_track(None).unwrap().id, d);
    // Only c, d are playable: the cursor wraps to c.
    assert_eq!(q.next_track(None).unwrap().id, c);
}

#[test]
fn a_bpm_without_a_grid_is_not_analysed() {
    let lib = library();
    let a = add(&lib, 1, Some(124.0), "8A");
    let b = add(&lib, 2, Some(125.0), "8A");
    let pl = lib.playlists().create("Set", None, false).unwrap();
    lib.playlists().add_track(pl, a, 0).unwrap();
    lib.playlists().add_track(pl, b, 1).unwrap();
    let mut q = queue(lib, QueueSource::Playlist(pl), QueueConfig::default(), 1)
        .with_grid_check(move |t| t.id == b);
    assert_eq!(ids(&mut q, 3, None), vec![b, b, b]);
    assert!(q.last_skips().contains(&(a, SkipReason::NotAnalysed)));
}

#[test]
fn the_history_window_is_hours_and_count() {
    let lib = library();
    let old = add(&lib, 1, Some(124.0), "8A");
    let fresh = add(&lib, 2, Some(124.0), "8A");
    let ok = add(&lib, 3, Some(124.0), "8A");
    lib.tracks().record_play(old, None, NOW - 5 * HOUR).unwrap();
    lib.tracks().record_play(fresh, None, NOW - HOUR).unwrap();
    let cfg = QueueConfig {
        smart: false,
        ..QueueConfig::default()
    };
    let mut q = queue(lib, QueueSource::Search(String::new()), cfg, 5);
    for _ in 0..30 {
        let id = q.next_track(None).unwrap().id;
        assert_ne!(id, fresh, "played an hour ago");
        assert!(id == old || id == ok);
    }
}

#[test]
fn only_the_last_n_plays_count() {
    let lib = library();
    let a = add(&lib, 1, Some(124.0), "8A");
    let b = add(&lib, 2, Some(124.0), "8A");
    lib.tracks().record_play(a, None, NOW - 3 * HOUR).unwrap();
    lib.tracks().record_play(b, None, NOW - HOUR).unwrap();
    let cfg = QueueConfig {
        recent_count: 1,
        smart: false,
        ..QueueConfig::default()
    };
    let mut q = queue(lib, QueueSource::Search(String::new()), cfg, 5);
    // Only the newest play (b) is recent.
    assert!((0..20).all(|_| q.next_track(None).unwrap().id == a));
}

#[test]
fn a_search_source_is_shuffled_and_deterministic_per_seed() {
    let mk = |seed| {
        let lib = library();
        for n in 1..=20 {
            add(&lib, n, Some(124.0), "8A");
        }
        queue(
            lib,
            QueueSource::Search(String::new()),
            QueueConfig {
                smart: false,
                ..QueueConfig::default()
            },
            seed,
        )
    };
    let a = ids(&mut mk(3), 8, None);
    let b = ids(&mut mk(3), 8, None);
    let c = ids(&mut mk(4), 8, None);
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_ne!(a, (1..=8).collect::<Vec<_>>(), "not in library order");
}

#[test]
fn the_search_text_narrows_the_pool() {
    let lib = library();
    let _ = add(&lib, 1, Some(124.0), "8A");
    let hit = lib
        .tracks()
        .insert(&TrackRecord {
            path: "/music/special.flac".into(),
            title: "Moonlight".into(),
            bpm: Some(120.0),
            camelot: "8A".into(),
            ..TrackRecord::default()
        })
        .unwrap();
    let mut q = queue(
        lib,
        QueueSource::Search("Moonlight".into()),
        QueueConfig::default(),
        1,
    );
    assert_eq!(ids(&mut q, 4, None), vec![hit, hit, hit, hit]);
}

#[test]
fn smart_prefers_a_compatible_key_within_the_bpm_window() {
    let current = QueueTrack {
        id: 999,
        title: String::new(),
        artist: String::new(),
        path: String::new(),
        bpm: Some(124.0),
        camelot: "8A".into(),
        has_grid: true,
    };
    for seed in 0..25 {
        let lib2 = library();
        add(&lib2, 1, Some(125.0), "3B");
        add(&lib2, 2, Some(150.0), "8A");
        let best2 = add(&lib2, 3, Some(126.0), "9A");
        let mut q = queue(
            lib2,
            QueueSource::Search(String::new()),
            QueueConfig::default(),
            seed,
        );
        assert_eq!(
            q.next_track(Some(&current)).unwrap().id,
            best2,
            "seed {seed}"
        );
    }
}

#[test]
fn an_empty_or_exhausted_library_gives_nothing() {
    let mut q = queue(
        library(),
        QueueSource::Search(String::new()),
        QueueConfig::default(),
        1,
    );
    assert!(q.next_track(None).is_none());
    let lib = library();
    add(&lib, 1, None, "");
    let mut q = queue(
        lib,
        QueueSource::Search(String::new()),
        QueueConfig::default(),
        1,
    );
    assert!(q.next_track(None).is_none());
    assert!(q
        .last_skips()
        .iter()
        .all(|(_, r)| *r == SkipReason::NotAnalysed));
}

#[test]
fn an_unknown_playlist_gives_nothing() {
    let lib = library();
    add(&lib, 1, Some(124.0), "8A");
    let mut q = queue(lib, QueueSource::Playlist(12345), QueueConfig::default(), 1);
    assert!(q.next_track(None).is_none());
}
