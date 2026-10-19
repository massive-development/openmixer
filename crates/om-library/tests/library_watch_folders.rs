//! Port of tests/unit/LibraryWatchFoldersTest.cpp: persisted folder list and background rescans.

mod common;

use common::{write_wav, Tags, TempDir};
use om_library::{
    core_migrations, Database, IngestSummary, Library, Migration, MetadataReaderRegistry,
    OpenMode, WatchFolderOptions, WatchFolders,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(10);

struct Fixture {
    /// Owns the folder tree; dropping it deletes the fixtures.
    _tmp: TempDir,
    db_file: PathBuf,
    music: PathBuf,
    fake_now: Arc<AtomicI64>,
}

impl Fixture {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let music = tmp.path().join("music");
        std::fs::create_dir_all(&music).unwrap();
        Self {
            db_file: tmp.path().join("library.db"),
            music,
            _tmp: tmp,
            fake_now: Arc::new(AtomicI64::new(0)),
        }
    }

    fn db(&self) -> String {
        self.db_file.to_string_lossy().into_owned()
    }

    fn clock(&self) -> om_library::Clock {
        let now = Arc::clone(&self.fake_now);
        Arc::new(move || now.load(Ordering::SeqCst))
    }

    fn options(&self, interval_ms: i64, scan_on_start: bool) -> WatchFolderOptions {
        WatchFolderOptions {
            interval_ms,
            poll_ms: 20,
            scan_on_start,
            clock: Some(self.clock()),
            ..WatchFolderOptions::default()
        }
    }

    fn make(&self, interval_ms: i64, scan_on_start: bool) -> WatchFolders {
        WatchFolders::create(
            &self.db(),
            MetadataReaderRegistry::with_defaults(),
            self.options(interval_ms, scan_on_start),
        )
        .expect("create watch folders")
    }

    fn track_count(&self) -> i64 {
        Library::open(&self.db(), OpenMode::ReadOnly, core_migrations())
            .unwrap()
            .tracks()
            .count()
            .unwrap()
    }
}

#[test]
fn folder_list_persists_across_instances() {
    let f = Fixture::new();
    {
        let w = f.make(1000, false);
        w.add(&f.music).unwrap();
        w.add(&f.music).unwrap();
        assert_eq!(w.list().unwrap().len(), 1);
    }
    let again = f.make(1000, false);
    let list = again.list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0], f.music.to_string_lossy());
    again.remove(&f.music).unwrap();
    assert!(again.list().unwrap().is_empty());
}

#[test]
fn add_rejects_non_folder() {
    let f = Fixture::new();
    let w = f.make(1000, false);
    assert!(w.add(&f.music.join("missing")).is_err());
    assert!(w.list().unwrap().is_empty());
}

#[test]
fn adding_a_folder_triggers_a_scan_on_the_background_thread() {
    let f = Fixture::new();
    write_wav(&f.music.join("a.wav"), 0.5, &Tags::default(), 440.0);
    let w = f.make(1000, false);
    w.add(&f.music).unwrap();
    assert!(w.wait_for_scans(1, WAIT));
    assert_eq!(f.track_count(), 1);
}

#[test]
fn nothing_scans_until_the_fake_clock_reaches_the_interval() {
    let f = Fixture::new();
    let w = f.make(1000, false);
    w.add(&f.music).unwrap();
    assert!(w.wait_for_scans(1, WAIT)); // the add itself triggers one pass

    write_wav(&f.music.join("late.wav"), 0.5, &Tags::default(), 440.0);
    f.fake_now.store(999, Ordering::SeqCst);
    w.poke();
    std::thread::sleep(Duration::from_millis(150)); // several poll periods: still no pass
    assert_eq!(w.scan_count(), 1);
    assert_eq!(f.track_count(), 0);

    f.fake_now.store(1000, Ordering::SeqCst);
    w.poke();
    assert!(w.wait_for_scans(2, WAIT));
    assert_eq!(f.track_count(), 1);
}

#[test]
fn manual_trigger_scans_immediately_and_reports_summary() {
    let f = Fixture::new();
    let added = Arc::new(AtomicUsize::new(0));
    let mut options = f.options(1_000_000, false);
    let sink = Arc::clone(&added);
    options.on_scan_done = Some(Arc::new(move |s: &IngestSummary| {
        sink.fetch_add(s.added, Ordering::SeqCst);
    }));
    let w = WatchFolders::create(&f.db(), MetadataReaderRegistry::with_defaults(), options).unwrap();
    w.add(&f.music).unwrap();
    assert!(w.wait_for_scans(1, WAIT));

    write_wav(&f.music.join("b.wav"), 0.5, &Tags::default(), 440.0);
    w.trigger_scan();
    assert!(w.wait_for_scans(2, WAIT));
    assert_eq!(added.load(Ordering::SeqCst), 1);
}

#[test]
fn removed_folder_is_no_longer_scanned() {
    let f = Fixture::new();
    let w = f.make(1000, false);
    w.add(&f.music).unwrap();
    assert!(w.wait_for_scans(1, WAIT));
    w.remove(&f.music).unwrap();
    write_wav(&f.music.join("c.wav"), 0.5, &Tags::default(), 440.0);
    w.trigger_scan();
    assert!(w.wait_for_scans(2, WAIT));
    assert_eq!(f.track_count(), 0);
}

#[test]
fn deleted_file_becomes_missing_on_next_pass() {
    let f = Fixture::new();
    let file = write_wav(&f.music.join("d.wav"), 0.5, &Tags::default(), 440.0);
    let w = f.make(1000, false);
    w.add(&f.music).unwrap();
    assert!(w.wait_for_scans(1, WAIT));
    std::fs::remove_file(&file).unwrap();
    w.trigger_scan();
    assert!(w.wait_for_scans(2, WAIT));
    let lib = Library::open(&f.db(), OpenMode::ReadOnly, core_migrations()).unwrap();
    assert_eq!(lib.track_files().missing_count().unwrap(), 1);
}

#[test]
fn scan_on_start_runs_without_a_trigger() {
    let f = Fixture::new();
    {
        let w = f.make(1000, false);
        w.add(&f.music).unwrap();
        assert!(w.wait_for_scans(1, WAIT));
    }
    write_wav(&f.music.join("e.wav"), 0.5, &Tags::default(), 440.0);
    let w = f.make(1_000_000, true);
    assert!(w.wait_for_scans(1, WAIT));
    assert_eq!(f.track_count(), 1);
}

#[test]
fn destructor_stops_promptly_with_pending_work() {
    let f = Fixture::new();
    for i in 0..20 {
        write_wav(
            &f.music.join(format!("t{i}.wav")),
            0.2,
            &Tags::default(),
            200.0 + i as f32,
        );
    }
    let start = std::time::Instant::now();
    {
        let w = f.make(1000, false);
        w.add(&f.music).unwrap();
    }
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn scan_thread_uses_the_supplied_migration_list() {
    let f = Fixture::new();
    let mut migrations = core_migrations().to_vec();
    let last = migrations.last().unwrap().version;
    // a later subsystem's table
    migrations.push(Migration {
        version: last + 1,
        name: "extra",
        sql: "CREATE TABLE extra_table (id INTEGER PRIMARY KEY);",
    });
    write_wav(&f.music.join("a.wav"), 0.5, &Tags::default(), 440.0);

    let failed = Arc::new(AtomicUsize::new(0));
    let mut options = f.options(1_000_000, false);
    options.migrations = migrations;
    let sink = Arc::clone(&failed);
    options.on_scan_done = Some(Arc::new(move |s: &IngestSummary| {
        sink.fetch_add(s.failed, Ordering::SeqCst);
    }));
    let w = WatchFolders::create(&f.db(), MetadataReaderRegistry::with_defaults(), options).unwrap();
    w.add(&f.music).unwrap();
    assert!(w.wait_for_scans(1, WAIT));
    assert_eq!(failed.load(Ordering::SeqCst), 0);
    assert_eq!(f.track_count(), 1);
}

#[test]
fn scan_thread_open_failure_is_reported() {
    let f = Fixture::new();
    let reported = Arc::new(Mutex::new(String::new()));
    let failed = Arc::new(AtomicUsize::new(0));
    let mut options = f.options(1_000_000, false);
    let sink = Arc::clone(&reported);
    options.on_error = Some(Arc::new(move |e: &str| {
        *sink.lock().unwrap() = e.to_owned();
    }));
    let count = Arc::clone(&failed);
    options.on_scan_done = Some(Arc::new(move |s: &IngestSummary| {
        count.fetch_add(s.failed, Ordering::SeqCst);
    }));
    let w = WatchFolders::create(&f.db(), MetadataReaderRegistry::with_defaults(), options).unwrap();

    {
        // a newer app migrated the file: the scan thread's open is now refused
        let db = Database::open(&f.db(), OpenMode::ReadWrite).unwrap();
        db.exec("INSERT INTO schema_version (version, name, applied_at) VALUES (99, 'future', 0);")
            .unwrap();
    }
    w.trigger_scan();
    assert!(w.wait_for_scans(1, WAIT));
    assert_eq!(failed.load(Ordering::SeqCst), 1);
    assert!(reported.lock().unwrap().contains("newer"));
}
