//! Shared state of one submitted analysis job. The service runs it on a worker thread through
//! `om_core::jobs::WorkerPool`, which owns status, progress and cancellation; this type adds the
//! analysis results. Any thread may read it, cancel it or wait for it.

pub use om_core::jobs::JobStatus;

use om_core::jobs::JobHandle;
use om_core::sync::LockExt;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

/// What a run writes for its job: results, which of them came from the cache, and the content hash.
#[derive(Debug, Default)]
pub struct JobOutput {
    state: Mutex<OutputState>,
}

#[derive(Debug, Default)]
struct OutputState {
    results: BTreeMap<String, String>,
    /// Ids whose results came from the cache.
    cached_ids: Vec<String>,
    content_hash: String,
}

impl JobOutput {
    fn lock(&self) -> MutexGuard<'_, OutputState> {
        self.state.lock_unpoisoned()
    }

    pub(crate) fn set_content_hash(&self, hash: String) {
        self.lock().content_hash = hash;
    }

    pub(crate) fn add_result(&self, analyzer_id: &str, payload: String, from_cache: bool) {
        let mut state = self.lock();
        state.results.insert(analyzer_id.to_owned(), payload);
        if from_cache {
            state.cached_ids.push(analyzer_id.to_owned());
        }
    }
}

#[derive(Debug)]
pub struct AnalysisJob {
    handle: JobHandle,
    name: String,
    output: Arc<JobOutput>,
}

impl AnalysisJob {
    pub(crate) fn new(handle: JobHandle, source_name: String, output: Arc<JobOutput>) -> Self {
        Self {
            handle,
            name: source_name,
            output,
        }
    }

    pub fn id(&self) -> i64 {
        self.handle.id() as i64
    }

    pub fn source_name(&self) -> &str {
        &self.name
    }

    pub fn status(&self) -> JobStatus {
        self.handle.status()
    }

    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    /// Progress 0..1.
    pub fn progress(&self) -> f32 {
        self.handle.progress()
    }

    pub fn cancel(&self) {
        self.handle.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.handle.is_cancelled()
    }

    /// Analyser id -> JSON payload.
    pub fn results(&self) -> BTreeMap<String, String> {
        self.output.lock().results.clone()
    }

    /// Results that came from the cache.
    pub fn cached_ids(&self) -> Vec<String> {
        self.output.lock().cached_ids.clone()
    }

    /// Hex SHA-256 of the source bytes; empty until hashed.
    pub fn content_hash(&self) -> String {
        self.output.lock().content_hash.clone()
    }

    pub fn error(&self) -> String {
        self.handle.error()
    }

    /// Blocks until the job reaches a final status or `timeout` passes. True when finished.
    pub fn wait_until_finished(&self, timeout: Duration) -> bool {
        self.handle.wait(timeout)
    }
}
