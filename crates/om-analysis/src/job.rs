//! Shared state of one submitted analysis job. The service writes it from a worker thread; any
//! thread may read it, cancel it or wait for it.

use om_core::atomic::AtomicF32;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum JobStatus {
    #[default]
    Queued,
    Running,
    Done,
    Cancelled,
    Failed,
}

impl JobStatus {
    fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Cancelled | Self::Failed)
    }
}

#[derive(Debug, Default)]
struct JobState {
    status: JobStatus,
    /// Analyser id -> JSON payload; complete once the status is `Done`.
    results: BTreeMap<String, String>,
    /// Ids whose results came from the cache.
    cached_ids: Vec<String>,
    content_hash: String,
    error: String,
}

#[derive(Debug)]
pub struct AnalysisJob {
    id: i64,
    name: String,
    progress: AtomicF32,
    cancelled: AtomicBool,
    state: Mutex<JobState>,
    finished: Condvar,
}

impl AnalysisJob {
    pub(crate) fn new(id: i64, source_name: String) -> Self {
        Self {
            id,
            name: source_name,
            progress: AtomicF32::new(0.0),
            cancelled: AtomicBool::new(false),
            state: Mutex::new(JobState::default()),
            finished: Condvar::new(),
        }
    }

    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn source_name(&self) -> &str {
        &self.name
    }

    pub fn status(&self) -> JobStatus {
        self.lock().status
    }

    pub fn is_finished(&self) -> bool {
        self.status().is_terminal()
    }

    /// Progress 0..1.
    pub fn progress(&self) -> f32 {
        self.progress.load()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Analyser id -> JSON payload.
    pub fn results(&self) -> BTreeMap<String, String> {
        self.lock().results.clone()
    }

    /// Results that came from the cache.
    pub fn cached_ids(&self) -> Vec<String> {
        self.lock().cached_ids.clone()
    }

    /// Hex SHA-256 of the source bytes; empty until hashed.
    pub fn content_hash(&self) -> String {
        self.lock().content_hash.clone()
    }

    pub fn error(&self) -> String {
        self.lock().error.clone()
    }

    /// Blocks until the job reaches a final status or `timeout` passes. True when finished.
    pub fn wait_until_finished(&self, timeout: Duration) -> bool {
        let guard = self.lock();
        let (guard, _) = self
            .finished
            .wait_timeout_while(guard, timeout, |s| !s.status.is_terminal())
            .unwrap_or_else(PoisonError::into_inner);
        guard.status.is_terminal()
    }

    pub(crate) fn set_progress(&self, value: f32) {
        self.progress.store(value);
    }

    pub(crate) fn set_status(&self, status: JobStatus) {
        self.lock().status = status;
        if status.is_terminal() {
            self.finished.notify_all();
        }
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

    pub(crate) fn fail(&self, message: String) {
        {
            let mut state = self.lock();
            state.error = message;
            state.status = JobStatus::Failed;
        }
        self.finished.notify_all();
    }

    fn lock(&self) -> MutexGuard<'_, JobState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
