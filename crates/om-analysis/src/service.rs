//! Background analysis: a FIFO of jobs served by worker threads (`om_core::jobs::WorkerPool`), with
//! per-job progress and cancellation and a shared result cache.
//! Listeners run on a worker thread after the job reaches its final status; keep them short and
//! hop to the UI thread for UI work. `submit` and `cancel_all` may be called from any thread.

use crate::job::{AnalysisJob, JobOutput};
use crate::job_runner::run_analysis_job;
use crate::{AnalyzerRegistry, ClipSource, ResultCache};
use om_core::jobs::{JobCtx, JobHandle, OnFinished, WorkerPool};
use om_core::sync::LockExt;
use std::sync::{Arc, Mutex};

/// Called once with the finished job, on the worker thread.
pub type Listener = Box<dyn FnOnce(&AnalysisJob) + Send>;

/// One queued analysis: what to read and which analysers to run on it.
struct Task {
    output: Arc<JobOutput>,
    source: Box<dyn ClipSource>,
    analyzer_ids: Vec<String>,
}

pub struct AnalysisService {
    pool: WorkerPool<Task>,
}

impl AnalysisService {
    /// Starts `workers` (at least one) worker threads.
    pub fn new(
        registry: Arc<AnalyzerRegistry>,
        cache: Option<Arc<dyn ResultCache>>,
        workers: usize,
    ) -> Self {
        let pool = WorkerPool::new(workers, "analysis", move |ctx: &JobCtx, mut task: Task| {
            run_analysis_job(
                ctx,
                &task.output,
                task.source.as_mut(),
                &task.analyzer_ids,
                &registry,
                cache.as_deref(),
            );
        });
        Self { pool }
    }

    /// Queues a job. A `None` source fails the job at once and nothing is queued (its listener is
    /// then never called).
    pub fn submit(
        &self,
        source: Option<Box<dyn ClipSource>>,
        analyzer_ids: Vec<String>,
        listener: Option<Listener>,
    ) -> Arc<AnalysisJob> {
        let Some(source) = source else {
            let handle = self.pool.submit_failed("no source");
            let output = Arc::new(JobOutput::default());
            return Arc::new(AnalysisJob::new(handle, String::new(), output));
        };
        let name = source.describe();
        let output = Arc::new(JobOutput::default());
        let task = Task {
            output: Arc::clone(&output),
            source,
            analyzer_ids,
        };
        let handle = match listener {
            None => self.pool.submit(task),
            Some(listener) => self.pool.submit_with_listener(
                task,
                listener_hook(listener, name.clone(), Arc::clone(&output)),
            ),
        };
        Arc::new(AnalysisJob::new(handle, name, output))
    }

    /// Cancels every queued and running job.
    pub fn cancel_all(&self) {
        self.pool.cancel_all();
    }

    pub fn queued_count(&self) -> usize {
        self.pool.queued_len()
    }
}

/// Adapts a one-shot listener to the pool's completion hook. The listener gets the job as an
/// `AnalysisJob` over the same handle and output the caller holds.
fn listener_hook(listener: Listener, name: String, output: Arc<JobOutput>) -> OnFinished {
    let listener = Mutex::new(Some(listener));
    Arc::new(move |handle: &JobHandle| {
        let taken = listener.lock_unpoisoned().take();
        if let Some(listener) = taken {
            listener(&AnalysisJob::new(
                handle.clone(),
                name.clone(),
                Arc::clone(&output),
            ));
        }
    })
}

#[cfg(test)]
mod tests;
