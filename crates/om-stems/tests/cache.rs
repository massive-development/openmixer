//! Stem cache: layout, 16-bit round trip, atomic writes and crash cleanup, keys, LRU eviction.
//! Uses no separator; the sink is driven directly.

use om_analysis::NullAnalysisContext;
use om_stems::StemKind;
use om_stems::{MockSeparator, SliceSource, StemCache, StemKey, StemSeparator, StemSink};
use std::fs::{self, File};
use std::path::Path;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

const HASH: &str = "ab00cd11ef22ab00cd11ef22ab00cd11ef22ab00cd11ef22ab00cd11ef22ab00";

fn key() -> StemKey {
    StemKey::new(HASH, "htdemucs", 1)
}

fn other_hash_key(n: u8) -> StemKey {
    StemKey::new(format!("{:064x}", u64::from(n) + 1), "htdemucs", 1)
}

fn open(dir: &TempDir) -> StemCache {
    StemCache::open(dir.path().join("stems")).expect("open cache")
}

/// Commits one entry of `frames` frames; stem `k` is a constant `0.1 * (k + 1)` on the left and
/// its negative on the right, so each stem is distinguishable.
fn commit(cache: &StemCache, key: &StemKey, frames: usize) -> om_stems::StemEntry {
    let mut sink = cache.begin_write(key, 44_100.0).expect("begin");
    let left: Vec<f32> = (0..frames).map(|i| (i % 7) as f32 * 0.01).collect();
    let right: Vec<f32> = left.iter().map(|s| -s).collect();
    for kind in StemKind::ALL {
        let scale = 0.1 * (kind as usize + 1) as f32;
        let l: Vec<f32> = left.iter().map(|s| s * scale).collect();
        let r: Vec<f32> = right.iter().map(|s| s * scale).collect();
        sink.write(kind, &l, &r).expect("write");
    }
    sink.finish().expect("finish")
}

fn set_mtime(path: &Path, when: SystemTime) {
    File::options()
        .write(true)
        .open(path)
        .expect("open meta")
        .set_modified(when)
        .expect("set mtime");
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .expect("read dir")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn layout_is_hash_sep_version_with_four_wavs_and_meta() {
    let dir = TempDir::new().unwrap();
    let cache = open(&dir);
    let entry = commit(&cache, &key(), 1000);

    assert_eq!(entry.dir_name, format!("{HASH}-htdemucs-v1"));
    assert_eq!(
        names(&cache.root().join(&entry.dir_name)),
        vec![
            "bass.wav",
            "drums.wav",
            "meta.json",
            "other.wav",
            "vocals.wav"
        ]
    );
    assert_eq!(entry.meta.frames, 1000);
    assert_eq!(entry.meta.source_rate, 44_100.0);
    assert_eq!(entry.meta.bytes, 4 * (44 + 4 * 1000));
    // The temporary directory is gone after a commit.
    assert_eq!(names(cache.root()), vec![entry.dir_name.clone()]);
}

#[test]
fn write_then_read_back_is_within_16_bit_quantisation() {
    let dir = TempDir::new().unwrap();
    let cache = open(&dir);
    let frames = 3000;
    commit(&cache, &key(), frames);

    let mut reader = cache.open_reader(&key()).expect("reader");
    assert_eq!(reader.frames(), frames as u64);
    assert_eq!(reader.rate(), 44_100);
    let tolerance = 0.5 / 32767.0 + 1e-7;
    for kind in StemKind::ALL {
        let scale = 0.1 * (kind as usize + 1) as f32;
        let mut left = vec![0.0f32; frames];
        let mut right = vec![0.0f32; frames];
        // Read in two calls to exercise the seek.
        let first = reader
            .read(kind, 0, &mut left[..1000], &mut right[..1000])
            .unwrap();
        let second = reader
            .read(kind, 1000, &mut left[1000..], &mut right[1000..])
            .unwrap();
        assert_eq!(first + second, frames);
        for i in 0..frames {
            let want_l = ((i % 7) as f32 * 0.01) * scale;
            assert!((left[i] - want_l).abs() <= tolerance, "{kind} frame {i}");
            assert!((right[i] + want_l).abs() <= tolerance, "{kind} frame {i}");
        }
        assert_eq!(
            reader
                .read(kind, frames as u64, &mut left[..4], &mut right[..4])
                .unwrap(),
            0,
            "end of stem"
        );
    }
}

#[test]
fn an_interrupted_write_leaves_nothing_behind() {
    let dir = TempDir::new().unwrap();
    let cache = open(&dir);
    {
        let mut sink = cache.begin_write(&key(), 48_000.0).expect("begin");
        sink.write(StemKind::Drums, &[0.5; 100], &[0.5; 100])
            .expect("write");
        // Dropped without finish: a cancel or an error.
    }
    assert!(names(cache.root()).is_empty(), "no tmp or final directory");
    assert!(cache.lookup(&key()).is_none());
}

#[test]
fn crash_leftovers_are_removed_at_open_and_are_not_hits() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("stems");
    fs::create_dir_all(root.join(".tmp-crashed-1-0")).unwrap();
    fs::write(root.join(".tmp-crashed-1-0").join("drums.wav"), b"partial").unwrap();

    // A final directory without a readable meta.json is a miss and is swept by eviction.
    let broken = root.join(key().dir_name());
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("drums.wav"), b"x").unwrap();

    let cache = StemCache::open(&root).expect("open");
    assert_eq!(names(&root), vec![key().dir_name()]);
    assert!(cache.lookup(&key()).is_none());
    assert!(cache.entries().unwrap().is_empty());
    assert_eq!(cache.evict_lru(u64::MAX).unwrap(), vec![key().dir_name()]);
    assert!(names(&root).is_empty());
}

#[test]
fn a_changed_hash_separator_or_version_is_a_miss() {
    let dir = TempDir::new().unwrap();
    let cache = open(&dir);
    commit(&cache, &key(), 100);

    assert!(cache.lookup(&key()).is_some());
    assert!(cache.lookup(&other_hash_key(0)).is_none(), "other content");
    assert!(
        cache.lookup(&StemKey::new(HASH, "other-sep", 1)).is_none(),
        "other separator"
    );
    assert!(
        cache.lookup(&StemKey::new(HASH, "htdemucs", 2)).is_none(),
        "other version"
    );
    assert!(cache
        .lookup(&StemKey::new("not-a-hash", "htdemucs", 1))
        .is_none());
    assert!(cache.lookup(&StemKey::new(HASH, "../escape", 1)).is_none());
}

#[test]
fn committing_an_existing_key_keeps_the_first_entry() {
    let dir = TempDir::new().unwrap();
    let cache = open(&dir);
    let first = commit(&cache, &key(), 100);
    let second = commit(&cache, &key(), 100);
    assert_eq!(first.dir_name, second.dir_name);
    assert_eq!(names(cache.root()), vec![first.dir_name]);
}

#[test]
fn a_hit_touches_the_entry_and_eviction_takes_the_least_recently_used() {
    let dir = TempDir::new().unwrap();
    let cache = open(&dir);
    let a = commit(&cache, &other_hash_key(1), 500);
    let b = commit(&cache, &other_hash_key(2), 500);
    let c = commit(&cache, &other_hash_key(3), 500);
    let base = SystemTime::now() - Duration::from_secs(10_000);
    set_mtime(&a.path.join("meta.json"), base);
    set_mtime(&b.path.join("meta.json"), base + Duration::from_secs(100));
    set_mtime(&c.path.join("meta.json"), base + Duration::from_secs(200));

    // Using A makes it the most recent.
    assert!(cache.lookup(&other_hash_key(1)).is_some());

    let each = a.meta.bytes;
    let total = cache.total_bytes().unwrap();
    assert_eq!(total, 3 * each);
    let removed = cache.evict_lru(total - 1).unwrap();
    assert_eq!(
        removed,
        vec![b.dir_name.clone()],
        "B was the least recently used"
    );
    assert!(cache.lookup(&other_hash_key(1)).is_some());
    assert!(cache.lookup(&other_hash_key(3)).is_some());
    assert!(cache.lookup(&other_hash_key(2)).is_none());
}

#[test]
fn eviction_respects_the_cap_and_zero_empties_the_cache() {
    let dir = TempDir::new().unwrap();
    let cache = open(&dir);
    commit(&cache, &other_hash_key(1), 200);
    commit(&cache, &other_hash_key(2), 200);
    let total = cache.total_bytes().unwrap();

    assert!(
        cache.evict_lru(total).unwrap().is_empty(),
        "at the cap is allowed"
    );
    assert_eq!(cache.entries().unwrap().len(), 2);

    let removed = cache.evict_lru(0).unwrap();
    assert_eq!(removed.len(), 2);
    assert_eq!(cache.total_bytes().unwrap(), 0);
    assert!(names(cache.root()).is_empty());
}

#[test]
fn remove_refuses_paths_outside_the_root() {
    let dir = TempDir::new().unwrap();
    let cache = open(&dir);
    commit(&cache, &key(), 10);
    assert!(cache.remove("../x").is_err());
    assert!(cache.remove("").is_err());
    assert!(cache.remove(".tmp-x").is_err());
    cache.remove(&key().dir_name()).unwrap();
    cache.remove(&key().dir_name()).unwrap(); // absent: no-op
    assert!(cache.lookup(&key()).is_none());
}

#[test]
fn mock_separator_output_round_trips_through_the_cache() {
    // The whole stem path without the network: a source, the separator and the cache sink.
    let dir = TempDir::new().unwrap();
    let cache = open(&dir);
    let input: Vec<f32> = (0..5000).map(|i| ((i as f32) * 0.01).sin() * 0.8).collect();
    let mut source = SliceSource::mono(&input, 44_100.0);
    let mut sink = cache.begin_write(&key(), 44_100.0).unwrap();
    MockSeparator::new()
        .separate_to(&mut source, &mut sink, &NullAnalysisContext)
        .expect("separate");
    let entry = sink.finish().unwrap();
    assert_eq!(entry.meta.frames, 5000);

    let mut reader = cache.open_reader(&key()).unwrap();
    let mut l = vec![0.0; 5000];
    let mut r = vec![0.0; 5000];
    reader.read(StemKind::Vocals, 0, &mut l, &mut r).unwrap();
    for i in 0..5000 {
        // The mock quarters the input.
        assert!((l[i] - input[i] * 0.25).abs() <= 0.5 / 32767.0 + 1e-6);
    }
}

#[test]
fn reopening_without_sweep_keeps_a_live_write() {
    let dir = TempDir::new().unwrap();
    let cache = open(&dir);
    let mut sink = cache.begin_write(&key(), 44_100.0).expect("begin");
    let samples = vec![0.25f32; 16];
    for kind in StemKind::ALL {
        sink.write(kind, &samples, &samples).expect("write");
    }
    // UI reads (cache_bytes, clear) open the cache while a separation is running.
    let _ui = StemCache::open_no_sweep(dir.path().join("stems")).expect("ui open");
    assert!(_ui.total_bytes().is_ok());
    let entry = sink.finish().expect("live write must survive a UI open");
    assert_eq!(entry.meta.frames, 16);
}

#[test]
fn service_open_sweeps_only_foreign_or_stale_tmp_dirs() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("stems");
    fs::create_dir_all(&root).unwrap();
    // A pid that cannot be this process (u32::MAX is never a live pid) and one from this process.
    let foreign = format!(".tmp-{HASH}-htdemucs-v1-{}-0", u32::MAX);
    let mine = format!(".tmp-{HASH}-htdemucs-v1-{}-0", std::process::id());
    fs::create_dir_all(root.join(&foreign)).unwrap();
    fs::create_dir_all(root.join(&mine)).unwrap();
    let _ = StemCache::open_no_sweep(&root).unwrap();
    let mut both = vec![foreign.clone(), mine.clone()];
    both.sort();
    assert_eq!(names(&root), both);
    let _ = StemCache::open(&root).unwrap();
    assert_eq!(names(&root), vec![mine]);
}

#[test]
fn eviction_keeping_the_new_entry_removes_others_even_when_it_is_the_oldest() {
    let dir = TempDir::new().unwrap();
    let cache = open(&dir);
    let old = commit(&cache, &other_hash_key(1), 200);
    let other = commit(&cache, &other_hash_key(2), 200);
    set_mtime(
        &old.path.join("meta.json"),
        SystemTime::now() - Duration::from_secs(3600),
    );
    // The cap is smaller than one entry: the kept one stays, the rest go.
    let removed = cache
        .evict_lru_keeping(1, Some(&old.dir_name))
        .expect("evict");
    assert_eq!(removed, vec![other.dir_name.clone()]);
    assert!(cache.lookup(&other_hash_key(1)).is_some());
    assert!(cache.lookup(&other_hash_key(2)).is_none());
    // Only the kept entry is left, though it exceeds the cap.
    assert!(cache
        .evict_lru_keeping(0, Some(&old.dir_name))
        .unwrap()
        .is_empty());
    assert_eq!(cache.entries().unwrap().len(), 1);
}
