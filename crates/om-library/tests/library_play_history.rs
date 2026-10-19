//! Play history (migration 5): recorded plays, the bump on the track row, cascade delete, and an
//! upgrade of a schema-4 database copy that keeps its data.

use om_library::{
    core_migrations, current_schema_version, CueDao, CuePoint, Library, OpenMode, TrackRecord,
};

fn memory_library() -> Library {
    Library::open(":memory:", OpenMode::ReadWrite, core_migrations()).expect("open :memory:")
}

fn add_track(lib: &Library, path: &str) -> i64 {
    lib.tracks()
        .insert(&TrackRecord {
            path: path.into(),
            content_hash: format!("hash-{path}"),
            title: path.into(),
            play_count: 2,
            date_added_ms: 1000,
            ..Default::default()
        })
        .expect("insert track")
}

fn count(lib: &Library, sql: &str) -> i64 {
    lib.database()
        .connection()
        .query_row(sql, [], |row| row.get(0))
        .expect("count")
}

#[test]
fn record_play_inserts_history_and_bumps_track_columns() {
    let lib = memory_library();
    let id = add_track(&lib, "/m/a.wav");

    lib.tracks().record_play(id, Some(1), 5_000).expect("play");
    lib.tracks().record_play(id, None, 9_000).expect("play");

    let got = lib.tracks().find_by_id(id).unwrap().unwrap();
    assert_eq!(got.play_count, 4, "bumped from 2 by two plays");
    assert_eq!(got.last_played_ms, Some(9_000));

    let plays = lib.tracks().plays_for(id).expect("plays_for");
    assert_eq!(plays.len(), 2);
    assert_eq!(plays[0].played_ms, 9_000, "newest first");
    assert_eq!(plays[0].deck, None);
    assert_eq!(plays[1].played_ms, 5_000);
    assert_eq!(plays[1].deck, Some(1));
}

#[test]
fn recent_plays_are_newest_first_across_tracks_and_respect_limit() {
    let lib = memory_library();
    let a = add_track(&lib, "/m/a.wav");
    let b = add_track(&lib, "/m/b.wav");
    lib.tracks().record_play(a, Some(0), 100).unwrap();
    lib.tracks().record_play(b, Some(1), 300).unwrap();
    lib.tracks().record_play(a, Some(0), 200).unwrap();

    let recent = lib.tracks().recent_plays(2).expect("recent");
    let order: Vec<(i64, i64)> = recent.iter().map(|p| (p.track_id, p.played_ms)).collect();
    assert_eq!(order, vec![(b, 300), (a, 200)]);
    assert_eq!(lib.tracks().recent_plays(10).unwrap().len(), 3);
}

#[test]
fn plays_for_only_returns_that_track() {
    let lib = memory_library();
    let a = add_track(&lib, "/m/a.wav");
    let b = add_track(&lib, "/m/b.wav");
    lib.tracks().record_play(a, None, 1).unwrap();
    assert!(lib.tracks().plays_for(b).unwrap().is_empty());
}

#[test]
fn unknown_track_records_nothing() {
    let lib = memory_library();
    assert!(lib.tracks().record_play(999, Some(0), 1).is_err());
    assert_eq!(count(&lib, "SELECT COUNT(*) FROM play_history;"), 0);
}

#[test]
fn deleting_a_track_removes_its_history() {
    let lib = memory_library();
    let id = add_track(&lib, "/m/a.wav");
    lib.tracks().record_play(id, Some(0), 1).unwrap();
    lib.tracks().remove(id).expect("remove");
    assert_eq!(count(&lib, "SELECT COUNT(*) FROM play_history;"), 0);
}

#[test]
fn schema_four_copy_upgrades_to_five_and_keeps_its_data() {
    let dir = tempfile::tempdir().expect("tempdir");
    let v4 = dir.path().join("v4.db");
    let v4_path = v4.to_str().expect("utf-8").to_owned();
    {
        // A database as the previous build left it: migrations 1 to 4 only.
        let lib = Library::open(&v4_path, OpenMode::ReadWrite, &core_migrations()[..4])
            .expect("open v4");
        let id = add_track(&lib, "/m/kept.wav");
        lib.tracks().set_rating(id, 5).unwrap();
        lib.cues()
            .set_cue(&CuePoint {
                track_id: id,
                slot: 2,
                position_frames: 44_100,
                colour_argb: 0,
                label: "drop".into(),
            })
            .unwrap();
        assert_eq!(current_schema_version(lib.database()).unwrap(), 4);
    }

    // The upgrade runs on a copy, as a user would keep the original file aside.
    let copy = dir.path().join("copy.db");
    std::fs::copy(&v4, &copy).expect("copy");
    let copy_path = copy.to_str().expect("utf-8").to_owned();
    let lib = Library::open(&copy_path, OpenMode::ReadWrite, core_migrations()).expect("upgrade");

    assert_eq!(current_schema_version(lib.database()).unwrap(), 5);
    let kept = lib.tracks().all().unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].path, "/m/kept.wav");
    assert_eq!(kept[0].rating, 5);
    assert_eq!(kept[0].play_count, 2);
    assert_eq!(CueDao::new(lib.database()).cues(kept[0].id).unwrap().len(), 1);
    assert_eq!(count(&lib, "SELECT COUNT(*) FROM play_history;"), 0);

    lib.tracks()
        .record_play(kept[0].id, Some(0), 42)
        .expect("new table is usable");
    assert_eq!(lib.tracks().recent_plays(5).unwrap().len(), 1);
}
