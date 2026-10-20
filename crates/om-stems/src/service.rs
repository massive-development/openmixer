//! The stem service: separates a track in the background and records the result.
//!
//! One job per track, run by a single worker (`om_core::jobs::WorkerPool`, FIFO). A job:
//!
//! 1. cache hit (`StemCache::lookup`): re-records the library row and finishes at once;
//! 2. model gate: when a [`ModelGate`] is given, the model must be installed and its licence
//!    accepted, both checked on disk. Nothing is downloaded here, so a refused job does no network
//!    I/O. Without a gate (the mock separator) nothing is checked;
//! 3. decodes the track at its own rate ([`FileStereoSource`]) and streams the separator into a
//!    [`StemCacheSink`]; a cancel is honoured between chunks and before the commit;
//! 4. commits the cache entry (atomic rename), records `track_stems`, then enforces the cache cap.
//!
//! Nothing starts a job by itself: callers submit explicitly. Loading a track never does.
//!
//! Threading: the worker opens its own library connection per job (SQLite WAL allows it beside the
//! owner thread's connection), so no `Library` crosses threads. The worker's CPU budget is the
//! separator's thread count (`OrtInference` uses at most half the cores by default); between
//! chunks the sink yields the core, which is the sleep-free part of "low priority".
//!
//! A panic inside the separator fails that job only (the pool catches it). The partial cache
//! directory is removed by the sink's `Drop`. A separator that keeps state behind a mutex may then
//! refuse later jobs with an error, not a crash.

use crate::cache::{CacheError, StemCache, StemEntry, StemKey};
use crate::manifest::ModelManifest;
use crate::separator::{FileStereoSource, StemError, StemSeparator, StereoSource};
use crate::weights::WeightManager;
use om_analysis::AnalysisContext;
use om_core::jobs::{JobCtx, JobHandle, WorkerPool};
use om_core::sync::LockExt;
use om_library::{core_migrations, Library, LibraryError, OpenMode, TrackStemsRecord};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// One track to separate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StemRequest {
    pub track_id: i64,
    pub path: PathBuf,
    /// Lowercase hex SHA-256 of the file (`om_analysis::hash_file_content`).
    pub content_hash: String,
}

/// The model the gate checks: its manifest and the folder the weights live in.
pub struct ModelGate {
    pub manifest: ModelManifest,
    pub weights: Mutex<WeightManager>,
}

impl ModelGate {
    pub fn new(manifest: ModelManifest, weights: WeightManager) -> Self {
        Self {
            manifest,
            weights: Mutex::new(weights),
        }
    }

    /// Ok when the model is installed and its licence accepted. Never downloads.
    fn check(&self) -> Result<(), String> {
        let weights = self.weights.lock_unpoisoned();
        let id = &self.manifest.id;
        if !weights.is_installed(&self.manifest) {
            return Err(format!(
                "stem model {id} is not installed; install it in Settings > Library"
            ));
        }
        if !weights.has_accepted_licence(&self.manifest) {
            return Err(format!("licence for stem model {id} is not accepted"));
        }
        Ok(())
    }
}

pub struct StemServiceConfig {
    pub separator: Arc<dyn StemSeparator>,
    /// Cache root, for example `<library folder>/stems`.
    pub cache_root: PathBuf,
    /// The library file (must be a file, not `:memory:`).
    pub library_path: String,
    /// Cache cap in bytes; `None` for no cap.
    pub max_cache_bytes: Option<u64>,
    /// `None` skips the model check (the mock separator needs no model).
    pub gate: Option<ModelGate>,
}

#[derive(Debug, Error)]
pub enum StemServiceError {
    #[error(transparent)]
    Cache(#[from] CacheError),
    #[error(transparent)]
    Library(#[from] LibraryError),
}

/// Bridges the job's cancel and progress to the separator's analysis context.
struct JobProgress<'a>(&'a JobCtx);

impl AnalysisContext for JobProgress<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    fn set_progress(&self, fraction: f32) {
        self.0.set_progress(fraction);
    }
}

/// Everything a job needs, shared with the worker.
struct Inner {
    separator: Arc<dyn StemSeparator>,
    cache: StemCache,
    library_path: String,
    /// Cap in bytes; 0 = no cap. Changed by [`StemService::set_max_cache_bytes`].
    max_cache_bytes: AtomicU64,
    gate: Option<ModelGate>,
}

impl Inner {
    fn key(&self, req: &StemRequest) -> StemKey {
        StemKey::new(
            req.content_hash.clone(),
            self.separator.id(),
            self.separator.version(),
        )
    }

    fn record(&self, track_id: i64, entry: &StemEntry) -> Result<(), StemServiceError> {
        let library = Library::open(&self.library_path, OpenMode::ReadWrite, core_migrations())?;
        library.track_stems().set(&TrackStemsRecord {
            track_id,
            sep_id: entry.meta.key.sep_id.clone(),
            version: entry.meta.key.version,
            dir: entry.dir_name.clone(),
            bytes: i64::try_from(entry.meta.bytes).unwrap_or(i64::MAX),
            created_ms: entry.meta.created_ms,
        })?;
        let cap = self.max_cache_bytes.load(Ordering::SeqCst);
        if cap > 0 {
            for dir in self.cache.evict_lru(cap)? {
                library.track_stems().delete_by_dir(&dir)?;
            }
        }
        Ok(())
    }

    fn run(&self, ctx: &JobCtx, req: StemRequest) -> Result<(), String> {
        let key = self.key(&req);
        if let Some(entry) = self.cache.lookup(&key) {
            self.record(req.track_id, &entry)
                .map_err(|e| e.to_string())?;
            ctx.set_progress(1.0);
            return Ok(());
        }
        if let Some(gate) = &self.gate {
            gate.check()?;
        }
        if ctx.is_cancelled() {
            return Ok(());
        }
        let mut source = FileStereoSource::open(&req.path).map_err(|e| e.to_string())?;
        let rate = source.sample_rate();
        let mut sink = self
            .cache
            .begin_write(&key, rate)
            .map_err(|e| e.to_string())?;
        let progress = JobProgress(ctx);
        match self
            .separator
            .separate_to(&mut source, &mut sink, &progress)
        {
            Ok(()) => {}
            // A cancel is not a failure; the sink's drop removes the partial directory.
            Err(StemError::Cancelled) => return Ok(()),
            Err(e) => return Err(e.to_string()),
        }
        if ctx.is_cancelled() {
            return Ok(());
        }
        let entry = sink.finish().map_err(|e| e.to_string())?;
        self.record(req.track_id, &entry).map_err(|e| e.to_string())
    }
}

type Task = StemRequest;

pub struct StemService {
    pool: WorkerPool<Task>,
    inner: Arc<Inner>,
}

impl StemService {
    /// Starts the worker. Fails only when the cache root cannot be created.
    pub fn new(config: StemServiceConfig) -> Result<Self, StemServiceError> {
        let inner = Arc::new(Inner {
            separator: config.separator,
            cache: StemCache::open(config.cache_root)?,
            library_path: config.library_path,
            max_cache_bytes: AtomicU64::new(config.max_cache_bytes.unwrap_or(0)),
            gate: config.gate,
        });
        let worker = Arc::clone(&inner);
        let pool = WorkerPool::new(1, "stems", move |ctx: &JobCtx, req: Task| {
            if let Err(message) = worker.run(ctx, req) {
                ctx.fail(message);
            }
        });
        Ok(Self { pool, inner })
    }

    /// Queues a separation behind the jobs already waiting. The handle reports progress, status
    /// and errors; `cancel()` stops it, leaving no partial directory.
    pub fn submit(&self, request: StemRequest) -> JobHandle {
        self.pool.submit(request)
    }

    /// Jobs waiting to start.
    pub fn queued_len(&self) -> usize {
        self.pool.queued_len()
    }

    /// The cache, for reads and for the UI's size readout.
    pub fn cache(&self) -> &StemCache {
        &self.inner.cache
    }

    /// The cache key a request maps to with this service's separator.
    pub fn key_for(&self, content_hash: &str) -> StemKey {
        StemKey::new(
            content_hash,
            self.inner.separator.id(),
            self.inner.separator.version(),
        )
    }

    /// Deletes every cached stem set of `track_id`: the directories, then the index rows. Call it
    /// before the track row is removed (the rows cascade with the track, the directories do not).
    /// Returns the number of directories removed. Not synchronised with a running job for the
    /// same track; cancel that job first.
    pub fn forget_track(&self, track_id: i64) -> Result<usize, StemServiceError> {
        let library = Library::open(
            &self.inner.library_path,
            OpenMode::ReadWrite,
            core_migrations(),
        )?;
        let rows = library.track_stems().for_track(track_id)?;
        for row in &rows {
            self.inner.cache.remove(&row.dir)?;
            library
                .track_stems()
                .delete(track_id, &row.sep_id, row.version)?;
        }
        Ok(rows.len())
    }

    /// Removes every committed stem set and every index row (see [`clear_stem_cache`]). Cancel the
    /// running jobs first.
    pub fn clear_cache(&self) -> Result<usize, StemServiceError> {
        clear_stem_cache(self.inner.cache.root(), &self.inner.library_path)
    }

    /// Changes the cache cap; the next committed job enforces it. `None` (or 0) for no cap.
    pub fn set_max_cache_bytes(&self, cap: Option<u64>) {
        self.inner
            .max_cache_bytes
            .store(cap.unwrap_or(0), Ordering::SeqCst);
    }

    /// Bytes the committed stem sets take up on disk, as recorded in their `meta.json`.
    pub fn cache_bytes(&self) -> Result<u64, StemServiceError> {
        Ok(self.inner.cache.total_bytes()?)
    }

    /// Drops index rows whose directory is gone (removed by hand, or evicted by another run), and
    /// returns how many. Directories without rows are left to the cap's eviction.
    pub fn reconcile(&self) -> Result<usize, StemServiceError> {
        let library = Library::open(
            &self.inner.library_path,
            OpenMode::ReadWrite,
            core_migrations(),
        )?;
        let mut dropped = 0;
        for row in library.track_stems().list_all_bytes()? {
            if !self
                .inner
                .cache
                .root()
                .join(&row.dir)
                .join("meta.json")
                .is_file()
            {
                dropped += library.track_stems().delete_by_dir(&row.dir)?;
            }
        }
        Ok(dropped)
    }
}

/// Removes every committed stem set under `root` and every `track_stems` row of the library at
/// `library_path`, without a running service (the settings page can clear with no model). Returns
/// the number of directories removed. Cancel running jobs first: a job still writing would be left
/// with a directory it cannot commit.
pub fn clear_stem_cache(root: &Path, library_path: &str) -> Result<usize, StemServiceError> {
    let cache = StemCache::open_no_sweep(root)?;
    let library = Library::open(library_path, OpenMode::ReadWrite, core_migrations())?;
    let mut removed = 0;
    for entry in cache.entries()? {
        cache.remove(&entry.dir_name)?;
        removed += 1;
    }
    for row in library.track_stems().list_all_bytes()? {
        library.track_stems().delete_by_dir(&row.dir)?;
    }
    Ok(removed)
}
