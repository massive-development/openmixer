//! Stem cache index (migration 8): the `track_stems` rows, cascade delete with the track, and an
//! upgrade of a schema-7 database copy that keeps its data.

use om_library::{
    core_migrations, current_schema_version, Library, OpenMode, TrackRecord, TrackStemsRecord,
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
            ..Default::default()
        })
        .expect("insert track")
}

fn record(track_id: i64, sep: &str, version: i32, bytes: i64, created_ms: i64) -> TrackStemsRecord {
    TrackStemsRecord {
        track_id,
        sep_id: sep.into(),
        version,
        dir: format!("hash{track_id}-{sep}-v{version}"),
        bytes,
        created_ms,
    }
}

#[test]
fn set_get_replace_and_delete_one_row() {
    let lib = memory_library();
    let id = add_track(&lib, "/m/a.wav");
    let dao = lib.track_stems();

    assert!(dao.get(id, "htdemucs", 1).unwrap().is_none());
    dao.set(&record(id, "htdemucs", 1, 100, 5)).unwrap();
    assert_eq!(
        dao.get(id, "htdemucs", 1).unwrap(),
        Some(record(id, "htdemucs", 1, 100, 5))
    );

    // Same key: replaced, not duplicated.
    dao.set(&record(id, "htdemucs", 1, 250, 9)).unwrap();
    assert_eq!(dao.for_track(id).unwrap().len(), 1);
    assert_eq!(dao.get(id, "htdemucs", 1).unwrap().unwrap().bytes, 250);

    // A different version or separator is a different row.
    dao.set(&record(id, "htdemucs", 2, 10, 9)).unwrap();
    dao.set(&record(id, "other", 1, 10, 9)).unwrap();
    assert_eq!(dao.for_track(id).unwrap().len(), 3);

    assert!(dao.delete(id, "htdemucs", 2).unwrap());
    assert!(!dao.delete(id, "htdemucs", 2).unwrap());
    assert!(dao.get(id, "htdemucs", 2).unwrap().is_none());
}

#[test]
fn totals_and_listing_are_in_creation_order() {
    let lib = memory_library();
    let a = add_track(&lib, "/m/a.wav");
    let b = add_track(&lib, "/m/b.wav");
    let dao = lib.track_stems();
    dao.set(&record(a, "htdemucs", 1, 300, 20)).unwrap();
    dao.set(&record(b, "htdemucs", 1, 100, 10)).unwrap();

    let all = dao.list_all_bytes().unwrap();
    let order: Vec<i64> = all.iter().map(|r| r.track_id).collect();
    assert_eq!(order, vec![b, a], "oldest first");
    assert_eq!(dao.total_bytes().unwrap(), 400);
    assert_eq!(dao.delete_by_dir(&all[0].dir).unwrap(), 1);
    assert_eq!(dao.total_bytes().unwrap(), 300);
}

#[test]
fn deleting_a_track_removes_its_stem_rows() {
    let lib = memory_library();
    let id = add_track(&lib, "/m/a.wav");
    let keep = add_track(&lib, "/m/b.wav");
    lib.track_stems()
        .set(&record(id, "htdemucs", 1, 1, 1))
        .unwrap();
    lib.track_stems()
        .set(&record(keep, "htdemucs", 1, 1, 1))
        .unwrap();

    lib.tracks().remove(id).expect("remove");
    assert_eq!(lib.track_stems().list_all_bytes().unwrap().len(), 1);
    assert_eq!(lib.track_stems().for_track(keep).unwrap().len(), 1);
}

#[test]
fn schema_seven_copy_upgrades_to_eight_and_keeps_its_data() {
    let dir = tempfile::tempdir().expect("tempdir");
    let v7 = dir.path().join("v7.db");
    let v7_path = v7.to_str().expect("utf-8").to_owned();
    {
        let lib =
            Library::open(&v7_path, OpenMode::ReadWrite, &core_migrations()[..7]).expect("open v7");
        let id = add_track(&lib, "/m/kept.wav");
        lib.tracks().set_rating(id, 4).unwrap();
        assert_eq!(current_schema_version(lib.database()).unwrap(), 7);
    }

    let copy = dir.path().join("copy.db");
    std::fs::copy(&v7, &copy).expect("copy");
    let copy_path = copy.to_str().expect("utf-8").to_owned();
    let lib = Library::open(&copy_path, OpenMode::ReadWrite, core_migrations()).expect("upgrade");

    assert_eq!(current_schema_version(lib.database()).unwrap(), 8);
    let kept = lib.tracks().all().unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].path, "/m/kept.wav");
    assert_eq!(kept[0].rating, 4);
    assert!(lib.track_stems().list_all_bytes().unwrap().is_empty());

    lib.track_stems()
        .set(&record(kept[0].id, "htdemucs", 1, 7, 1))
        .expect("new table is usable");
    assert_eq!(lib.track_stems().total_bytes().unwrap(), 7);
}
