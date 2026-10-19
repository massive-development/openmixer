//! The on-disk stem cache.
//!
//! Layout: `<root>/<content_hash>-<sep_id>-v<version>/{drums,bass,vocals,other}.wav` plus
//! `meta.json`. `<root>` is `<library folder>/stems` (the app passes it in). Each stem is 16-bit
//! stereo PCM at the track's original sample rate, so frame positions match the deck.
//!
//! Writes are atomic: a separation is streamed into `<root>/.tmp-<dir>-<pid>-<n>/` and renamed to
//! its final name only after `meta.json` is complete. A crash or a cancel leaves at most a
//! `.tmp-*` directory, which [`StemCache::open`] removes. A final directory without a readable
//! `meta.json` is not a hit and is treated as garbage by eviction.
//!
//! The directory is the source of truth: a lookup needs no database. The library's `track_stems`
//! table is an index that the stem service keeps in step (see `service.rs`). Recency for the LRU
//! cap is the modification time of `meta.json`, which every hit touches.
//!
//! Single process: two applications sharing one root would each remove the other's `.tmp-*`
//! directories at open.

use crate::separator::{StemError, StemKind, StemSink};
use crate::wav::{WavReader, WavWriter};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Name of the metadata file inside an entry.
pub const META_FILE: &str = "meta.json";
/// Prefix of in-progress directories.
pub const TMP_PREFIX: &str = ".tmp-";
/// Bump when the on-disk layout changes; entries with another format are misses.
pub const FORMAT: &str = "pcm16-stereo-wav-v1";

#[derive(Debug, Error)]
pub enum CacheError {
    #[error("stem cache I/O at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("invalid stem cache entry: {0}")]
    Invalid(String),
}

fn io_at(path: &Path) -> impl FnOnce(io::Error) -> CacheError + '_ {
    move |source| CacheError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// What identifies a cached separation. Any change to one part is a miss.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StemKey {
    /// Lowercase hex SHA-256 of the audio file (or of the decoded clip).
    pub content_hash: String,
    /// `StemSeparator::id()`; letters, digits, `.`, `_` and `-` only.
    pub sep_id: String,
    /// `StemSeparator::version()`.
    pub version: i32,
}

impl StemKey {
    pub fn new(content_hash: impl Into<String>, sep_id: impl Into<String>, version: i32) -> Self {
        Self {
            content_hash: content_hash.into(),
            sep_id: sep_id.into(),
            version,
        }
    }

    /// The entry's directory name.
    pub fn dir_name(&self) -> String {
        format!("{}-{}-v{}", self.content_hash, self.sep_id, self.version)
    }

    fn is_valid(&self) -> bool {
        let hash_ok = self.content_hash.len() == 64
            && self
                .content_hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        let sep_ok = !self.sep_id.is_empty()
            && !self.sep_id.starts_with('.')
            && self
                .sep_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        hash_ok && sep_ok && self.version >= 0
    }
}

/// The contents of `meta.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct StemMeta {
    pub key: StemKey,
    /// The rate the stems were written at (the track's rate).
    pub source_rate: f64,
    /// Frames per stem.
    pub frames: u64,
    /// Total bytes of the four WAV files.
    pub bytes: u64,
    /// Milliseconds since the Unix epoch when the entry was committed.
    pub created_ms: i64,
}

impl StemMeta {
    pub fn to_json(&self) -> String {
        json!({
            "format": FORMAT,
            "contentHash": self.key.content_hash,
            "sepId": self.key.sep_id,
            "sepVersion": self.key.version,
            "sourceRate": self.source_rate,
            "frames": self.frames,
            "bytes": self.bytes,
            "created": self.created_ms,
            "stems": StemKind::ALL.map(StemKind::as_str),
        })
        .to_string()
    }

    fn from_json(text: &str) -> Result<Self, CacheError> {
        let v: Value = serde_json::from_str(text)
            .map_err(|e| CacheError::Invalid(format!("meta.json: {e}")))?;
        let bad = |field: &str| CacheError::Invalid(format!("meta.json: bad {field}"));
        if v["format"].as_str() != Some(FORMAT) {
            return Err(bad("format"));
        }
        Ok(Self {
            key: StemKey {
                content_hash: v["contentHash"]
                    .as_str()
                    .ok_or_else(|| bad("contentHash"))?
                    .into(),
                sep_id: v["sepId"].as_str().ok_or_else(|| bad("sepId"))?.into(),
                version: v["sepVersion"]
                    .as_i64()
                    .and_then(|n| i32::try_from(n).ok())
                    .ok_or_else(|| bad("sepVersion"))?,
            },
            source_rate: v["sourceRate"].as_f64().ok_or_else(|| bad("sourceRate"))?,
            frames: v["frames"].as_u64().ok_or_else(|| bad("frames"))?,
            bytes: v["bytes"].as_u64().ok_or_else(|| bad("bytes"))?,
            created_ms: v["created"].as_i64().ok_or_else(|| bad("created"))?,
        })
    }
}

/// A committed entry.
#[derive(Debug, Clone, PartialEq)]
pub struct StemEntry {
    /// Directory name under the cache root.
    pub dir_name: String,
    pub path: PathBuf,
    pub meta: StemMeta,
}

/// The four stem files of one entry, opened for reading.
pub struct StemReader {
    readers: Vec<WavReader>,
    frames: u64,
    rate: u32,
}

impl StemReader {
    pub fn frames(&self) -> u64 {
        self.frames
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Reads up to `left.len()` frames of `kind` from frame `start`; 0 at the end.
    pub fn read(
        &mut self,
        kind: StemKind,
        start: u64,
        left: &mut [f32],
        right: &mut [f32],
    ) -> Result<usize, CacheError> {
        let index = stem_index(kind);
        self.readers[index]
            .read(start, left, right)
            .map_err(|e| CacheError::Invalid(format!("{kind}: {e}")))
    }
}

fn stem_index(kind: StemKind) -> usize {
    StemKind::ALL
        .iter()
        .position(|&k| k == kind)
        .unwrap_or_default()
}

fn stem_file(kind: StemKind) -> String {
    format!("{}.wav", kind.as_str())
}

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct StemCache {
    root: PathBuf,
}

impl StemCache {
    /// Opens (creating if needed) the cache at `root` and removes leftover `.tmp-*` directories
    /// from interrupted writes.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, CacheError> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(io_at(&root))?;
        for entry in fs::read_dir(&root).map_err(io_at(&root))?.flatten() {
            if entry.file_name().to_string_lossy().starts_with(TMP_PREFIX) {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn entry_path(&self, key: &StemKey) -> PathBuf {
        self.root.join(key.dir_name())
    }

    /// The committed entry for `key`, or `None` on a miss. A hit touches `meta.json` so the LRU
    /// order follows use. A corrupt or mismatched entry is a miss.
    pub fn lookup(&self, key: &StemKey) -> Option<StemEntry> {
        if !key.is_valid() {
            return None;
        }
        let dir = self.entry_path(key);
        let meta_path = dir.join(META_FILE);
        let meta = StemMeta::from_json(&fs::read_to_string(&meta_path).ok()?).ok()?;
        if &meta.key != key
            || !StemKind::ALL
                .iter()
                .all(|&k| dir.join(stem_file(k)).is_file())
        {
            return None;
        }
        // Recency for the LRU cap. Failure only costs eviction order, so it is ignored.
        if let Ok(file) = File::options().write(true).open(&meta_path) {
            let _ = file.set_modified(SystemTime::now());
        }
        Some(StemEntry {
            dir_name: key.dir_name(),
            path: dir,
            meta,
        })
    }

    /// Opens the four stems of a hit for reading.
    pub fn open_reader(&self, key: &StemKey) -> Result<StemReader, CacheError> {
        let entry = self.lookup(key).ok_or_else(|| {
            CacheError::Invalid(format!("no cached stems for {}", key.dir_name()))
        })?;
        let mut readers = Vec::with_capacity(4);
        let mut rate = 0;
        for kind in StemKind::ALL {
            let path = entry.path.join(stem_file(kind));
            let reader = WavReader::open(&path).map_err(io_at(&path))?;
            if reader.frames() != entry.meta.frames {
                return Err(CacheError::Invalid(format!(
                    "{}: {} frames, meta says {}",
                    path.display(),
                    reader.frames(),
                    entry.meta.frames
                )));
            }
            rate = reader.rate();
            readers.push(reader);
        }
        Ok(StemReader {
            readers,
            frames: entry.meta.frames,
            rate,
        })
    }

    /// Committed entries, unordered. Directories without a valid `meta.json` are skipped.
    pub fn entries(&self) -> Result<Vec<StemEntry>, CacheError> {
        Ok(self
            .scan_all()?
            .into_iter()
            .filter_map(|s| s.entry)
            .collect())
    }

    /// Total recorded bytes of all committed entries.
    pub fn total_bytes(&self) -> Result<u64, CacheError> {
        Ok(self
            .scan_all()?
            .iter()
            .filter_map(|s| s.entry.as_ref().map(|e| e.meta.bytes))
            .sum())
    }

    /// Removes least-recently-used entries until the total is at most `max_bytes`. Returns the
    /// directory names removed, oldest first. Directories that are not valid entries are removed
    /// first and reported too.
    pub fn evict_lru(&self, max_bytes: u64) -> Result<Vec<String>, CacheError> {
        let mut found = self.scan_all()?;
        let mut removed = Vec::new();
        for junk in found.iter().filter(|s| s.entry.is_none()) {
            self.remove_dir(&junk.dir_name)?;
            removed.push(junk.dir_name.clone());
        }
        let mut valid: Vec<ScanItem> = found.drain(..).filter(|s| s.entry.is_some()).collect();
        valid.sort_by_key(|s| s.mtime);
        let mut total: u64 = valid
            .iter()
            .filter_map(|s| s.entry.as_ref().map(|e| e.meta.bytes))
            .sum();
        for item in valid {
            if total <= max_bytes {
                break;
            }
            total -= item.entry.as_ref().map_or(0, |e| e.meta.bytes);
            self.remove_dir(&item.dir_name)?;
            removed.push(item.dir_name);
        }
        Ok(removed)
    }

    /// Removes one entry directory by name (no-op when absent).
    pub fn remove(&self, dir_name: &str) -> Result<(), CacheError> {
        if dir_name.is_empty() || dir_name.contains(['/', '\\']) || dir_name.starts_with('.') {
            return Err(CacheError::Invalid(format!(
                "refusing to remove {dir_name:?}"
            )));
        }
        self.remove_dir(dir_name)
    }

    fn remove_dir(&self, dir_name: &str) -> Result<(), CacheError> {
        let path = self.root.join(dir_name);
        match fs::remove_dir_all(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(CacheError::Io {
                path: path.display().to_string(),
                source: e,
            }),
        }
    }

    /// Starts a write for `key`: creates the temporary directory and the four files. Stems are
    /// fed to the returned sink, which commits on [`StemCacheSink::finish`] and removes the
    /// temporary directory if dropped first.
    pub fn begin_write(
        &self,
        key: &StemKey,
        source_rate: f64,
    ) -> Result<StemCacheSink, CacheError> {
        if !key.is_valid() {
            return Err(CacheError::Invalid(format!("bad key {}", key.dir_name())));
        }
        if !(source_rate.is_finite() && (1000.0..=768_000.0).contains(&source_rate)) {
            return Err(CacheError::Invalid(format!("sample rate {source_rate}")));
        }
        let id = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp = self.root.join(format!(
            "{TMP_PREFIX}{}-{}-{id}",
            key.dir_name(),
            std::process::id()
        ));
        fs::create_dir(&tmp).map_err(io_at(&tmp))?;
        let rate = source_rate.round() as u32;
        let mut writers = Vec::with_capacity(4);
        for kind in StemKind::ALL {
            let path = tmp.join(stem_file(kind));
            writers.push(WavWriter::create(&path, rate).map_err(io_at(&path))?);
        }
        Ok(StemCacheSink {
            cache_root: self.root.clone(),
            key: key.clone(),
            source_rate,
            tmp: Some(tmp),
            writers,
        })
    }

    fn scan_all(&self) -> Result<Vec<ScanItem>, CacheError> {
        let mut out = Vec::new();
        for item in fs::read_dir(&self.root)
            .map_err(io_at(&self.root))?
            .flatten()
        {
            let name = item.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !item.path().is_dir() {
                continue;
            }
            let meta_path = item.path().join(META_FILE);
            let mtime = fs::metadata(&meta_path)
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH);
            let entry = fs::read_to_string(&meta_path)
                .ok()
                .and_then(|t| StemMeta::from_json(&t).ok())
                .filter(|m| m.key.dir_name() == name)
                .map(|meta| StemEntry {
                    dir_name: name.clone(),
                    path: item.path(),
                    meta,
                });
            out.push(ScanItem {
                dir_name: name,
                mtime,
                entry,
            });
        }
        Ok(out)
    }
}

struct ScanItem {
    dir_name: String,
    mtime: SystemTime,
    entry: Option<StemEntry>,
}

/// Receives the separator's stems for one entry and writes them into the temporary directory.
/// Dropping it without [`StemCacheSink::finish`] (a cancel, an error, a panic) removes the
/// directory.
pub struct StemCacheSink {
    cache_root: PathBuf,
    key: StemKey,
    source_rate: f64,
    tmp: Option<PathBuf>,
    writers: Vec<WavWriter>,
}

impl StemCacheSink {
    /// Commits the entry: writes `meta.json`, renames the directory into place and returns it.
    /// If an entry for the same key already exists (another writer won), this one is discarded
    /// and the existing entry is returned.
    pub fn finish(mut self) -> Result<StemEntry, CacheError> {
        let tmp = self
            .tmp
            .take()
            .ok_or_else(|| CacheError::Invalid("sink already finished".into()))?;
        let writers = std::mem::take(&mut self.writers);
        let mut frames = None;
        for writer in writers {
            let n = writer.finish().map_err(io_at(&tmp))?;
            match frames {
                None => frames = Some(n),
                Some(f) if f == n => {}
                Some(f) => {
                    return Err(CacheError::Invalid(format!(
                        "stems differ in length ({f} and {n} frames)"
                    )))
                }
            }
        }
        let frames = frames.unwrap_or(0);
        let mut bytes = 0u64;
        for kind in StemKind::ALL {
            let path = tmp.join(stem_file(kind));
            bytes += fs::metadata(&path).map_err(io_at(&path))?.len();
        }
        let created_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
        let meta = StemMeta {
            key: self.key.clone(),
            source_rate: self.source_rate,
            frames,
            bytes,
            created_ms,
        };
        let meta_path = tmp.join(META_FILE);
        fs::write(&meta_path, meta.to_json()).map_err(io_at(&meta_path))?;

        let final_path = self.cache_root.join(self.key.dir_name());
        if final_path.exists() {
            let _ = fs::remove_dir_all(&tmp);
            return StemCache {
                root: self.cache_root.clone(),
            }
            .lookup(&self.key)
            .ok_or_else(|| CacheError::Invalid("entry exists but is unreadable".into()));
        }
        fs::rename(&tmp, &final_path).map_err(io_at(&final_path))?;
        Ok(StemEntry {
            dir_name: self.key.dir_name(),
            path: final_path,
            meta,
        })
    }
}

impl StemSink for StemCacheSink {
    fn write(&mut self, kind: StemKind, left: &[f32], right: &[f32]) -> Result<(), StemError> {
        // Yield between chunks so a long separation does not hog a core from the audio and UI
        // threads; the real CPU budget is the separator's thread count.
        std::thread::yield_now();
        let index = stem_index(kind);
        self.writers
            .get_mut(index)
            .ok_or_else(|| StemError::Sink("sink already finished".into()))?
            .write(left, right)
            .map_err(|e| StemError::Sink(format!("{kind}: {e}")))
    }
}

impl Drop for StemCacheSink {
    fn drop(&mut self) {
        if let Some(tmp) = self.tmp.take() {
            // Best effort: the writers are still open here, which Unix allows.
            self.writers.clear();
            let _ = fs::remove_dir_all(tmp);
        }
    }
}
