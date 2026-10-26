//! The library-backed automix queue over an in-memory library with generated tracks.

use om_automix::queue::{AutomixQueue, QueueConfig, QueueTrack, SkipReason};
use om_library::{core_migrations, Library, OpenMode, PrepStatus, TrackRecord};
use openmixer::automix_queue::{LibraryQueue, QueueSource};
use std::sync::Arc;

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

#[test]
fn has_grid_is_checked_lazily_and_stops_at_the_first_qualifying_track() {
    use std::cell::Cell;
    use std::rc::Rc;
    let lib = library();
    for n in 1..=200 {
        add(&lib, n, Some(124.0), "8A");
    }
    let checks = Rc::new(Cell::new(0_usize));
    let counter = checks.clone();
    let mut q = queue(
        lib,
        QueueSource::Search(String::new()),
        QueueConfig {
            smart: false,
            ..QueueConfig::default()
        },
        5,
    )
    .with_grid_check(move |_| {
        counter.set(counter.get() + 1);
        true
    });
    assert!(q.next_track(None).is_some());
    assert_eq!(
        checks.get(),
        1,
        "a pick needs one grid check, not one per track"
    );
}

#[test]
fn grid_checks_are_capped_at_fifty_candidates() {
    use std::cell::Cell;
    use std::rc::Rc;
    let lib = library();
    for n in 1..=200 {
        add(&lib, n, Some(124.0), "8A");
    }
    let checks = Rc::new(Cell::new(0_usize));
    let counter = checks.clone();
    let mut q = queue(
        lib,
        QueueSource::Search(String::new()),
        QueueConfig::default(),
        9,
    )
    .with_grid_check(move |_| {
        counter.set(counter.get() + 1);
        false
    });
    assert!(q.next_track(None).is_none(), "no track has a grid");
    assert!(checks.get() <= 50, "checked {} candidates", checks.get());
}

#[test]
fn the_recent_window_is_at_most_half_the_pool() {
    let lib = library();
    let ids: Vec<i64> = (1..=4).map(|n| add(&lib, n, Some(124.0), "8A")).collect();
    // Oldest first: the two newest plays (c, d) are the recent window, so only a and b are eligible.
    for (i, id) in ids.iter().enumerate() {
        lib.tracks()
            .record_play(*id, None, NOW - (4 - i as i64) * 60_000)
            .unwrap();
    }
    let cfg = QueueConfig {
        smart: false,
        ..QueueConfig::default()
    };
    let mut q = queue(lib, QueueSource::Search(String::new()), cfg, 2);
    let picks = ids_drawn(&mut q, 30);
    assert!(
        picks
            .iter()
            .all(|id| *id == Some(ids[0]) || *id == Some(ids[1])),
        "picked a recent track: {picks:?}"
    );
}

#[test]
fn when_every_analysed_track_is_recent_the_least_recent_plays() {
    let lib = library();
    let a = add(&lib, 1, Some(124.0), "8A");
    add(&lib, 2, None, "");
    add(&lib, 3, None, "");
    lib.tracks().record_play(a, None, NOW - 60_000).unwrap();
    let mut q = queue(
        lib,
        QueueSource::Search(String::new()),
        QueueConfig::default(),
        4,
    );
    assert_eq!(
        q.next_track(None).map(|t| t.id),
        Some(a),
        "nothing else is eligible, so the relaxed filter picks it"
    );
}

#[test]
fn a_fifteen_track_playlist_keeps_playing_when_it_wraps() {
    let lib = library();
    let tracks: Vec<i64> = (1..=15).map(|n| add(&lib, n, Some(124.0), "8A")).collect();
    let pl = lib.playlists().create("Long", None, false).unwrap();
    for (i, t) in tracks.iter().enumerate() {
        lib.playlists().add_track(pl, *t, i as i32).unwrap();
        lib.tracks()
            .record_play(*t, None, NOW - (16 - i as i64) * 60_000)
            .unwrap();
    }
    let mut q = queue(lib, QueueSource::Playlist(pl), QueueConfig::default(), 6);
    let picks = ids_drawn(&mut q, 30);
    assert!(
        picks.iter().all(Option::is_some),
        "stopped at a wrap: {picks:?}"
    );
}

fn ids_drawn(q: &mut LibraryQueue, n: usize) -> Vec<Option<i64>> {
    (0..n).map(|_| q.next_track(None).map(|t| t.id)).collect()
}

fn playlist_of(lib: &Library, ids: &[i64]) -> i64 {
    let pl = lib.playlists().create("Set", None, false).unwrap();
    for (i, t) in ids.iter().enumerate() {
        lib.playlists().add_track(pl, *t, i as i32).unwrap();
    }
    pl
}

#[allow(clippy::arc_with_non_send_sync)]
fn shared_library() -> Arc<Library> {
    Arc::new(library())
}

fn shared_queue(lib: &Arc<Library>, pl: i64) -> LibraryQueue {
    LibraryQueue::shared(
        Arc::clone(lib),
        QueueSource::Playlist(pl),
        QueueConfig::default(),
        1,
    )
    .with_clock(|| NOW)
}

#[test]
fn queue_submits_unanalysed_tracks_once() {
    let lib = shared_library();
    let a = add(&lib, 1, None, "");
    let b = add(&lib, 2, None, "");
    let pl = playlist_of(&lib, &[a, b]);
    let mut q = shared_queue(&lib, pl);
    for _ in 0..3 {
        assert!(q.next_track(None).is_none());
    }
    let queued: Vec<i64> = lib
        .prep_queue()
        .all()
        .unwrap()
        .iter()
        .map(|e| e.track_id)
        .collect();
    assert_eq!(queued, vec![a, b], "each id once");
    assert_eq!(q.pending_analysis(), &[a, b]);
    // Analysis finished for b: the next call returns it.
    lib.tracks().set_analysis(b, 125.0, "", "8A").unwrap();
    assert_eq!(q.next_track(None).unwrap().id, b);
}

#[test]
fn a_track_that_never_gets_analysed_is_given_up_on_after_three_submissions() {
    let lib = shared_library();
    let a = add(&lib, 1, None, "");
    let pl = playlist_of(&lib, &[a]);
    let mut q = shared_queue(&lib, pl);
    let dao = lib.prep_queue();
    for _ in 0..3 {
        assert!(q.next_track(None).is_none());
        assert_eq!(q.pending_analysis(), &[a]);
        // The analysis fails: the entry settles without a BPM.
        dao.set_status(a, PrepStatus::Failed, "corrupt").unwrap();
    }
    assert!(q.next_track(None).is_none());
    assert!(q.pending_analysis().is_empty(), "no longer waited on");
    assert!(q.last_error().unwrap().contains(&a.to_string()));
    assert_eq!(
        dao.find(a).unwrap().unwrap().status,
        PrepStatus::Failed,
        "not submitted a fourth time"
    );
}

#[test]
fn a_big_unanalysed_playlist_submits_a_capped_batch_per_pick() {
    let lib = shared_library();
    let tracks: Vec<i64> = (0..100).map(|n| add(&lib, n, None, "")).collect();
    let pl = playlist_of(&lib, &tracks);
    let mut q = shared_queue(&lib, pl);
    let cap = openmixer::automix_queue::MAX_SUBMISSIONS_PER_PICK;
    assert!(q.next_track(None).is_none());
    let first: Vec<i64> = lib
        .prep_queue()
        .all()
        .unwrap()
        .iter()
        .map(|e| e.track_id)
        .collect();
    assert_eq!(first, tracks[..cap].to_vec());
    assert!(q.next_track(None).is_none());
    assert_eq!(lib.prep_queue().all().unwrap().len(), cap * 2);
}

#[test]
fn the_give_up_notice_is_capped() {
    let lib = shared_library();
    let tracks: Vec<i64> = (0..6).map(|n| add(&lib, n, None, "")).collect();
    let pl = playlist_of(&lib, &tracks);
    let mut q = shared_queue(&lib, pl);
    let dao = lib.prep_queue();
    for _ in 0..3 {
        assert!(q.next_track(None).is_none());
        for &t in &tracks {
            dao.set_status(t, PrepStatus::Failed, "corrupt").unwrap();
        }
    }
    assert!(q.next_track(None).is_none());
    let msg = q.last_error().unwrap();
    assert!(msg.starts_with("6 tracks could not be analysed"), "{msg}");
    assert!(msg.len() < 80, "{msg}");
}
