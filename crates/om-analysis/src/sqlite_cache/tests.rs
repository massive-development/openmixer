//! Port of `tests/unit/AnalysisCacheTest.cpp` (the SQLite result cache cases).

use super::*;
use crate::test_signals::TempFile;

fn memory_cache() -> SqliteResultCache {
    SqliteResultCache::open(":memory:").expect("in-memory cache opens")
}

#[test]
fn stores_and_finds_by_hash_analyzer_and_version() {
    let cache = memory_cache();
    assert!(cache.store("abc", "bpm", 2, r#"{"bpm":128}"#));

    assert_eq!(
        cache.find("abc", "bpm", 2).as_deref(),
        Some(r#"{"bpm":128}"#)
    );
    assert_eq!(cache.find("abc", "bpm", 1), None);
    assert_eq!(cache.find("abc", "key", 2), None);
    assert_eq!(cache.find("xyz", "bpm", 2), None);
}

#[test]
fn store_replaces_and_erase_removes_all_rows_for_hash() {
    let cache = memory_cache();
    cache.store("abc", "bpm", 1, "old");
    cache.store("abc", "bpm", 2, "new");
    cache.store("abc", "key", 1, "8A");
    cache.store("def", "bpm", 1, "other");

    assert_eq!(cache.find("abc", "bpm", 2).as_deref(), Some("new"));
    assert_eq!(cache.erase("abc"), 2);
    assert_eq!(cache.find("abc", "key", 1), None);
    assert!(cache.find("def", "bpm", 1).is_some());
}

#[test]
fn persists_across_reopen() {
    let temp = TempFile::new("sqlite");
    let path = temp.path().display().to_string();
    {
        let cache = SqliteResultCache::open(&path).expect("opens");
        assert!(cache.store("h", "waveform", 3, "unicode \u{e9}"));
    }
    let reopened = SqliteResultCache::open(&path).expect("reopens");
    assert_eq!(
        reopened.find("h", "waveform", 3).as_deref(),
        Some("unicode \u{e9}")
    );
}

#[test]
fn open_fails_for_unwritable_path() {
    let error = SqliteResultCache::open("/nonexistent-dir/x/y.sqlite")
        .expect_err("unwritable path must fail");
    assert!(!error.to_string().is_empty());
}

#[test]
fn schema_matches_the_cpp_layout() {
    let cache = memory_cache();
    let db = cache.lock();
    let version: i32 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("user_version");
    assert_eq!(version, SCHEMA_VERSION);
    let columns: Vec<String> = db
        .prepare("PRAGMA table_info(analysis_results)")
        .and_then(|mut s| {
            s.query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<_, _>>()
        })
        .expect("table_info");
    assert_eq!(columns, ["content_hash", "analyzer", "version", "payload"]);
}
