//! Background analysis: a FIFO of jobs served by worker threads, with per-job progress and
//! cancellation and a shared result cache. Port of `src/analysis/AnalysisService.{h,cpp}`.
//! Listeners run on a worker thread after the job reaches its final status; keep them short and
//! hop to the UI thread for UI work. `submit` and `cancel_all` may be called from any thread.

use crate::job_runner::run_analysis_job;
use crate::{AnalysisJob, AnalyzerRegistry, ClipSource, ResultCache};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};

/// Called once with the finished job, on the worker thread.
pub type Listener = Box<dyn FnOnce(&AnalysisJob) + Send>;

struct Task {
    job: Arc<AnalysisJob>,
    source: Box<dyn ClipSource>,
    analyzer_ids: Vec<String>,
    listener: Option<Listener>,
}

#[derive(Default)]
struct State {
    queue: VecDeque<Task>,
    running: Vec<Arc<AnalysisJob>>,
    stopping: bool,
    next_id: i64,
}

struct Shared {
    registry: Arc<AnalyzerRegistry>,
    cache: Option<Arc<dyn ResultCache>>,
    state: Mutex<State>,
    wake: Condvar,
}

pub struct AnalysisService {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

impl AnalysisService {
    /// Starts `workers` (at least one) worker threads.
    pub fn new(
        registry: Arc<AnalyzerRegistry>,
        cache: Option<Arc<dyn ResultCache>>,
        workers: usize,
    ) -> Self {
        let shared = Arc::new(Shared {
            registry,
            cache,
            state: Mutex::new(State {
                next_id: 1,
                ..State::default()
            }),
            wake: Condvar::new(),
        });
        let workers = (0..workers.max(1))
            .map(|_| {
                let shared = Arc::clone(&shared);
                thread::spawn(move || worker_loop(&shared))
            })
            .collect();
        Self { shared, workers }
    }

    /// Queues a job. A `None` source, or a service that is shutting down, fails the job at once
    /// (its listener is then never called).
    pub fn submit(
        &self,
        source: Option<Box<dyn ClipSource>>,
        analyzer_ids: Vec<String>,
        listener: Option<Listener>,
    ) -> Arc<AnalysisJob> {
        let mut state = self.lock();
        let id = state.next_id;
        state.next_id += 1;
        let name = source.as_ref().map(|s| s.describe()).unwrap_or_default();
        let job = Arc::new(AnalysisJob::new(id, name));
        match source {
            None => job.fail("no source".to_owned()),
            Some(_) if state.stopping => job.fail("service is shutting down".to_owned()),
            Some(source) => {
                state.queue.push_back(Task {
                    job: Arc::clone(&job),
                    source,
                    analyzer_ids,
                    listener,
                });
                self.shared.wake.notify_one();
            }
        }
        job
    }

    /// Cancels every queued and running job.
    pub fn cancel_all(&self) {
        cancel_all(&self.lock());
    }

    pub fn queued_count(&self) -> usize {
        self.lock().queue.len()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for AnalysisService {
    /// Cancels queued and running jobs, then joins the workers.
    fn drop(&mut self) {
        {
            let mut state = self.lock();
            state.stopping = true;
            cancel_all(&state);
        }
        self.shared.wake.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn cancel_all(state: &State) {
    for task in &state.queue {
        task.job.cancel();
    }
    for job in &state.running {
        job.cancel();
    }
}

fn worker_loop(shared: &Shared) {
    while let Some(mut task) = wait_for_task(shared) {
        run_analysis_job(
            &task.job,
            task.source.as_mut(),
            &task.analyzer_ids,
            &shared.registry,
            shared.cache.as_deref(),
        );
        if let Some(listener) = task.listener.take() {
            listener(&task.job);
        }
        let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.running.retain(|job| !Arc::ptr_eq(job, &task.job));
    }
}

/// Next queued task, moved to the running list. `None` once stopping with nothing queued.
fn wait_for_task(shared: &Shared) -> Option<Task> {
    let state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
    let mut state = shared
        .wake
        .wait_while(state, |s| !s.stopping && s.queue.is_empty())
        .unwrap_or_else(PoisonError::into_inner);
    let task = state.queue.pop_front()?;
    state.running.push(Arc::clone(&task.job));
    Some(task)
}

#[cfg(test)]
mod tests;
