//! Port of tests/unit/LibraryIngestTest.cpp, IngestTest group, plus WatchFolderDaoTest and
//! LibraryMigrationV2Test (watch folders and the file-state cascade).

mod common;

use common::{stored, touch, write_wav, Tags, TempDir};
use om_analysis::hash_file_content;
use om_library::{
    core_migrations, current_schema_version, migrate, Database, IngestSummary, Ingestor, Library,
    MetadataReaderRegistry, OpenMode, TrackDao, TrackRecord, WatchFolderDao,
};
use std::path::{Path, PathBuf};

fn memory_library() -> Library {
    Library::open(":memory:", OpenMode::ReadWrite, core_migrations()).expect("open :memory:")
}

struct Fixture {
    lib: Library,
    readers: MetadataReaderRegistry,
    tmp: TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            lib: memory_library(),
            readers: MetadataReaderRegistry::with_defaults(),
            tmp: TempDir::new().unwrap(),
        }
    }

    fn dir(&self) -> PathBuf {
        self.tmp.path().to_path_buf()
    }

    fn scan(&self, folder: &Path) -> IngestSummary {
        self.try_scan(folder).0
    }

    /// Summary plus whether the scan reported success.
    fn try_scan(&self, folder: &Path) -> (IngestSummary, bool) {
        let mut s = IngestSummary::default();
        let ok = Ingestor::new(&self.lib, &self.readers)
            .ingest_folder(folder, &mut s, &|| false)
            .is_ok();
        (s, ok)
    }

    fn id_of(&self, path: &Path) -> i64 {
        self.lib
            .tracks()
            .find_by_path(&stored(path))
            .unwrap()
            .expect("track exists")
            .id
    }
}

#[test]
fn adds_tracks_recursively_with_tags_and_hash() {
    let f = Fixture::new();
    let a = write_wav(&f.dir().join("a.wav"), 1.0, &Tags::night_drive(), 440.0);
    write_wav(
        &f.dir().join("sub/deeper/b.wav"),
        1.0,
        &Tags::default(),
        880.0,
    );
    std::fs::write(f.dir().join("readme.txt"), "ignore me").unwrap();

    let s = f.scan(&f.dir());
    assert_eq!(s.scanned, 2);
    assert_eq!(s.added, 2);
    assert_eq!(f.lib.tracks().count().unwrap(), 2);

    let t = f.lib.tracks().find_by_path(&stored(&a)).unwrap().unwrap();
    assert_eq!(t.title, "Night Drive");
    assert_eq!(t.artist, "The Artist");
    assert_eq!(t.content_hash, hash_file_content(&a).unwrap());
    assert!(t.date_added_ms > 0);

    let state = f.lib.track_files().find(t.id).unwrap().unwrap();
    assert_eq!(
        state.size_bytes,
        std::fs::metadata(&a).unwrap().len() as i64
    );
    assert!(!state.missing);
}

#[test]
fn rescan_skips_unchanged_files() {
    let f = Fixture::new();
    write_wav(&f.dir().join("a.wav"), 1.0, &Tags::default(), 440.0);
    f.scan(&f.dir());
    let s = f.scan(&f.dir());
    assert_eq!(s.unchanged, 1);
    assert_eq!(s.added + s.updated, 0);
}

#[test]
fn changed_file_is_re_read_and_keeps_user_data() {
    let f = Fixture::new();
    let file = write_wav(&f.dir().join("a.wav"), 1.0, &Tags::default(), 440.0);
    touch(&file, 1_600_000_000_000);
    f.scan(&f.dir());
    let id = f.id_of(&file);
    f.lib.tracks().set_rating(id, 5).unwrap();

    write_wav(&file, 2.0, &Tags::night_drive(), 440.0);
    touch(&file, 1_700_000_000_000);
    let s = f.scan(&f.dir());
    assert_eq!(s.updated, 1);

    let t = f.lib.tracks().find_by_id(id).unwrap().unwrap();
    assert_eq!(t.title, "Night Drive");
    assert!((t.duration_sec - 2.0).abs() < 0.01);
    assert_eq!(t.rating, 5);
    assert_eq!(f.lib.tracks().count().unwrap(), 1);
}

#[test]
fn same_size_different_mtime_is_re_read() {
    let f = Fixture::new();
    let file = write_wav(&f.dir().join("a.wav"), 1.0, &Tags::default(), 440.0);
    touch(&file, 1_600_000_000_000);
    f.scan(&f.dir());
    touch(&file, 1_600_000_005_000);
    assert_eq!(f.scan(&f.dir()).updated, 1);
}

#[test]
fn vanished_file_is_marked_missing_and_restored_when_it_returns() {
    let f = Fixture::new();
    let keep = write_wav(&f.dir().join("keep.wav"), 1.0, &Tags::default(), 440.0);
    let gone = write_wav(&f.dir().join("gone.wav"), 1.0, &Tags::default(), 300.0);
    f.scan(&f.dir());
    let id = f.id_of(&gone);
    f.lib.tracks().set_rating(id, 4).unwrap();

    // Outside the scanned folder, as the C++ getSiblingFile places it.
    let backup = f.tmp.path().parent().unwrap().join(format!(
        "om_backup_{}.wav",
        f.tmp.path().file_name().unwrap().to_string_lossy()
    ));
    std::fs::rename(&gone, &backup).unwrap();
    let mut s = f.scan(&f.dir());
    assert_eq!(s.missing, 1);
    assert!(f.lib.track_files().find(id).unwrap().unwrap().missing);
    assert!(
        !f.lib
            .track_files()
            .find(f.id_of(&keep))
            .unwrap()
            .unwrap()
            .missing
    );
    assert_eq!(f.lib.tracks().count().unwrap(), 2); // row kept, not deleted

    s = f.scan(&f.dir());
    assert_eq!(s.missing, 0); // already flagged: not counted twice

    std::fs::rename(&backup, &gone).unwrap();
    s = f.scan(&f.dir());
    assert_eq!(s.restored, 1);
    assert!(!f.lib.track_files().find(id).unwrap().unwrap().missing);
    assert_eq!(f.lib.tracks().find_by_id(id).unwrap().unwrap().rating, 4);
    assert_eq!(f.lib.track_files().missing_count().unwrap(), 0);
}

#[test]
fn missing_folder_fails_and_flags_nothing() {
    let f = Fixture::new();
    write_wav(&f.dir().join("a.wav"), 1.0, &Tags::default(), 440.0);
    f.scan(&f.dir());

    let (_, ok) = f.try_scan(&f.dir().join("nope"));
    assert!(!ok);
    assert_eq!(f.lib.track_files().missing_count().unwrap(), 0);
}

#[test]
fn only_flags_tracks_under_the_scanned_folder() {
    let f = Fixture::new();
    let other = TempDir::new().unwrap();
    write_wav(&f.dir().join("a.wav"), 1.0, &Tags::default(), 440.0);
    let o = write_wav(&other.path().join("o.wav"), 1.0, &Tags::default(), 500.0);
    f.scan(&f.dir());
    f.scan(other.path());
    std::fs::remove_file(&o).unwrap();
    f.scan(&f.dir()); // the other folder's file is gone, but this scan does not cover it
    assert_eq!(f.lib.track_files().missing_count().unwrap(), 0);
}

#[test]
fn corrupt_file_counts_as_failed_and_does_not_flag_existing_row_missing() {
    let f = Fixture::new();
    let file = write_wav(&f.dir().join("a.wav"), 1.0, &Tags::default(), 440.0);
    touch(&file, 1_600_000_000_000);
    f.scan(&f.dir());

    std::fs::write(&file, "garbage").unwrap();
    touch(&file, 1_700_000_000_000);
    let s = f.scan(&f.dir());
    assert_eq!(s.failed, 1);
    assert_eq!(s.missing, 0);
    assert_eq!(f.lib.tracks().count().unwrap(), 1);
}

#[test]
fn stop_check_aborts_without_flagging_missing() {
    let f = Fixture::new();
    write_wav(&f.dir().join("a.wav"), 1.0, &Tags::default(), 440.0);
    f.scan(&f.dir());

    let mut s = IngestSummary::default();
    let r = Ingestor::new(&f.lib, &f.readers).ingest_folder(&f.dir(), &mut s, &|| true);
    assert!(r.is_err());
    assert_eq!(f.lib.track_files().missing_count().unwrap(), 0);
}

#[test]
fn failed_file_state_lookup_is_not_treated_as_new_file() {
    let f = Fixture::new();
    write_wav(&f.dir().join("a.wav"), 1.0, &Tags::default(), 440.0);
    f.scan(&f.dir()); // row exists, so the next scan reaches the track_files lookup
    f.lib.database().exec("DROP TABLE track_files;").unwrap();
    let s = f.scan(&f.dir());
    assert_eq!(s.failed, 1);
    assert_eq!(s.added + s.updated, 0);
    assert_eq!(f.lib.tracks().count().unwrap(), 1);
}

#[test]
fn failed_path_lookup_does_not_insert_duplicate() {
    let f = Fixture::new();
    write_wav(&f.dir().join("a.wav"), 1.0, &Tags::default(), 440.0);
    f.scan(&f.dir());
    f.lib
        .database()
        .exec("ALTER TABLE tracks RENAME TO tracks_hidden;")
        .unwrap(); // find_by_path now fails
    let s = f.scan(&f.dir());
    assert_eq!(s.failed, 1);
    assert_eq!(s.added + s.updated, 0);
    f.lib
        .database()
        .exec("ALTER TABLE tracks_hidden RENAME TO tracks;")
        .unwrap();
    assert_eq!(f.lib.tracks().count().unwrap(), 1);
}

#[test]
fn touched_but_identical_file_keeps_analysis() {
    let f = Fixture::new();
    let file = write_wav(&f.dir().join("a.wav"), 1.0, &Tags::default(), 440.0);
    touch(&file, 1_600_000_000_000);
    f.scan(&f.dir());
    let id = f.id_of(&file);
    f.lib.tracks().set_analysis(id, 128.0, "Am", "8A").unwrap();

    touch(&file, 1_700_000_000_000); // same bytes, new mtime
    assert_eq!(f.scan(&f.dir()).updated, 1);
    let t = f.lib.tracks().find_by_id(id).unwrap().unwrap();
    assert_eq!(t.bpm, Some(128.0));
    assert_eq!(t.key, "Am");
    assert_eq!(t.camelot, "8A");
}

#[test]
fn changed_content_drops_stale_analysis() {
    let f = Fixture::new();
    let file = write_wav(&f.dir().join("a.wav"), 1.0, &Tags::default(), 440.0);
    touch(&file, 1_600_000_000_000);
    f.scan(&f.dir());
    let id = f.id_of(&file);
    f.lib.tracks().set_analysis(id, 128.0, "Am", "8A").unwrap();

    write_wav(&file, 1.0, &Tags::default(), 990.0);
    touch(&file, 1_700_000_000_000);
    f.scan(&f.dir());
    assert!(f
        .lib
        .tracks()
        .find_by_id(id)
        .unwrap()
        .unwrap()
        .bpm
        .is_none());
}

#[test]
fn content_hash_is_stable_across_renames() {
    let f = Fixture::new();
    let a = write_wav(&f.dir().join("a.wav"), 1.0, &Tags::default(), 440.0);
    f.scan(&f.dir());
    let hash = f
        .lib
        .tracks()
        .find_by_path(&stored(&a))
        .unwrap()
        .unwrap()
        .content_hash;
    let b = f.dir().join("renamed.wav");
    std::fs::rename(&a, &b).unwrap();
    f.scan(&f.dir());
    assert_eq!(
        f.lib
            .tracks()
            .find_by_path(&stored(&b))
            .unwrap()
            .unwrap()
            .content_hash,
        hash
    );
    assert_eq!(f.lib.tracks().find_by_content_hash(&hash).unwrap().len(), 2); // old row stays, flagged missing
}

#[test]
fn ingest_summary_adds_componentwise() {
    let mut total = IngestSummary::default();
    total += IngestSummary {
        scanned: 2,
        added: 1,
        failed: 1,
        ..IngestSummary::default()
    };
    total += IngestSummary {
        scanned: 3,
        missing: 2,
        ..IngestSummary::default()
    };
    assert_eq!(total.scanned, 5);
    assert_eq!(total.added, 1);
    assert_eq!(total.missing, 2);
    assert_eq!(total.failed, 1);
}

#[test]
fn migration_v2_upgrades_a_v1_database_without_touching_tracks() {
    let db = Database::open(":memory:", OpenMode::ReadWrite).unwrap();
    let v1_only = &core_migrations()[..1];
    migrate(&db, v1_only).unwrap();
    let track = TrackRecord {
        path: "/x.wav".into(),
        ..TrackRecord::default()
    };
    TrackDao::new(&db).insert(&track).unwrap();

    let latest = core_migrations().last().unwrap().version;
    migrate(&db, core_migrations()).unwrap();
    assert_eq!(current_schema_version(&db).unwrap(), i64::from(latest));
    let count: i64 = db
        .connection()
        .query_row("SELECT COUNT(*) FROM tracks;", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    assert!(WatchFolderDao::new(&db).add("/music").is_ok());
}

#[test]
fn watch_folder_dao_add_is_idempotent_and_remove_works() {
    let lib = memory_library();
    let dao = lib.watch_folders();
    dao.add("/music").unwrap();
    dao.add("/music").unwrap();
    dao.add("/more").unwrap();
    assert_eq!(dao.list().unwrap().len(), 2);
    dao.remove("/music").unwrap();
    assert_eq!(dao.list().unwrap(), vec!["/more".to_string()]);
}

#[test]
fn deleting_a_track_cascades_its_file_state() {
    let lib = memory_library();
    let id = lib
        .tracks()
        .insert(&TrackRecord {
            path: "/x.wav".into(),
            ..TrackRecord::default()
        })
        .unwrap();
    lib.track_files().set(id, 1, 2).unwrap();
    lib.tracks().remove(id).unwrap();
    assert!(lib.track_files().find(id).unwrap().is_none());
}
