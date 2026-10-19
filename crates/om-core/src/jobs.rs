//! A generic FIFO job queue served by a fixed set of worker threads.
//!
//! `submit` queues a job and returns a [`JobHandle`]: any thread may read its status and progress,
//! cancel it or wait for it. Handlers poll [`JobCtx::is_cancelled`] to stop early; a running job is
//! never killed. A panicking handler fails its job and the worker carries on. Dropping the pool
//! cancels everything outstanding and joins the workers.
//!
//! Listeners (`submit_with_listener`) run on the worker thread once the job is final; keep them
//! short and hop to another thread for real work.

use crate::atomic::AtomicF32;
use crate::sync::LockExt;
use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum JobStatus {
    #[default]
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }
}

/// Called once per job, on the worker thread, after the job reaches its final status.
pub type OnFinished = Arc<dyn Fn(&JobHandle) + Send + Sync>;

#[derive(Debug, Default)]
struct Outcome {
    status: JobStatus,
    error: String,
}

/// State shared by a job's handle, its context and the pool.
#[derive(Debug)]
struct Shared {
    id: u64,
    progress: AtomicF32,
    cancelled: AtomicBool,
    outcome: Mutex<Outcome>,
    finished: Condvar,
}

impl Shared {
    fn new(id: u64) -> Arc<Self> {
        Arc::new(Self {
            id,
            progress: AtomicF32::new(0.0),
            cancelled: AtomicBool::new(false),
            outcome: Mutex::new(Outcome::default()),
            finished: Condvar::new(),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Outcome> {
        self.outcome.lock_unpoisoned()
    }

    fn status(&self) -> JobStatus {
        self.lock().status
    }

    fn set_status(&self, status: JobStatus) {
        self.lock().status = status;
        if status.is_terminal() {
            self.finished.notify_all();
        }
    }

    fn fail(&self, message: String) {
        {
            let mut outcome = self.lock();
            outcome.error = message;
            outcome.status = JobStatus::Failed;
        }
        self.finished.notify_all();
    }

    /// Moves a job that is not yet final to `Done` or `Cancelled`. A job already `Failed` keeps
    /// its failure even when it was also cancelled.
    fn finish(&self) {
        if self.status().is_terminal() {
            return;
        }
        if self.cancelled.load(Ordering::Acquire) {
            self.set_status(JobStatus::Cancelled);
        } else {
            self.progress.store(1.0);
            self.set_status(JobStatus::Done);
        }
    }
}

/// Read and cancel access to one submitted job. Cheap to clone; any thread may hold one.
#[derive(Debug, Clone)]
pub struct JobHandle {
    shared: Arc<Shared>,
}

impl JobHandle {
    pub fn id(&self) -> u64 {
        self.shared.id
    }

    pub fn status(&self) -> JobStatus {
        self.shared.status()
    }

    pub fn is_finished(&self) -> bool {
        self.status().is_terminal()
    }

    /// Progress 0..1. Reads 1.0 once the job is `Done`.
    pub fn progress(&self) -> f32 {
        self.shared.progress.load()
    }

    /// The failure message; empty unless the job is `Failed`.
    pub fn error(&self) -> String {
        self.shared.lock().error.clone()
    }

    /// Requests cancellation. A queued job is then skipped; a running job stops when its handler
    /// next checks [`JobCtx::is_cancelled`].
    pub fn cancel(&self) {
        self.shared.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.shared.cancelled.load(Ordering::Acquire)
    }

    /// Blocks until the job is final or `timeout` passes. True when finished.
    pub fn wait(&self, timeout: Duration) -> bool {
        let guard = self.shared.lock();
        let (guard, _) = self
            .shared
            .finished
            .wait_timeout_while(guard, timeout, |o| !o.status.is_terminal())
            .unwrap_or_else(PoisonError::into_inner);
        guard.status.is_terminal()
    }
}

/// What a handler sees while it runs its job.
#[derive(Debug)]
pub struct JobCtx {
    shared: Arc<Shared>,
}

impl JobCtx {
    pub fn id(&self) -> u64 {
        self.shared.id
    }

    /// True once the job was cancelled; a handler should return soon after seeing it.
    pub fn is_cancelled(&self) -> bool {
        self.shared.cancelled.load(Ordering::Acquire)
    }

    /// Reports progress; values outside 0..1 are clamped.
    pub fn set_progress(&self, fraction: f32) {
        self.shared.progress.store(fraction.clamp(0.0, 1.0));
    }

    /// Ends the job as `Failed` with `message`. The handler should return afterwards; the
    /// listener then fires as for any other final status.
    pub fn fail(&self, message: impl Into<String>) {
        self.shared.fail(message.into());
    }
}

type Handler<J> = dyn Fn(&JobCtx, J) + Send + Sync;

struct Queued<J> {
    shared: Arc<Shared>,
    job: J,
    on_finished: Option<OnFinished>,
}

struct State<J> {
    queue: VecDeque<Queued<J>>,
    running: Vec<Arc<Shared>>,
    stopping: bool,
    next_id: u64,
}

struct PoolShared<J> {
    handler: Box<Handler<J>>,
    state: Mutex<State<J>>,
    wake: Condvar,
}

impl<J> PoolShared<J> {
    fn lock(&self) -> MutexGuard<'_, State<J>> {
        self.state.lock_unpoisoned()
    }
}

/// `workers` threads take jobs from a FIFO queue and pass each to `handler`.
pub struct WorkerPool<J: Send + 'static> {
    shared: Arc<PoolShared<J>>,
    workers: Vec<JoinHandle<()>>,
}

impl<J: Send + 'static> WorkerPool<J> {
    /// Starts `workers` (at least one) threads named `name-<index>`.
    pub fn new(
        workers: usize,
        name: &str,
        handler: impl Fn(&JobCtx, J) + Send + Sync + 'static,
    ) -> Self {
        let shared = Arc::new(PoolShared {
            handler: Box::new(handler),
            state: Mutex::new(State {
                queue: VecDeque::new(),
                running: Vec::new(),
                stopping: false,
                next_id: 1,
            }),
            wake: Condvar::new(),
        });
        let workers = (0..workers.max(1))
            .map(|index| {
                let shared = Arc::clone(&shared);
                thread::Builder::new()
                    .name(format!("{name}-{index}"))
                    .spawn(move || worker_loop(&shared))
                    .expect("spawn job worker")
            })
            .collect();
        Self { shared, workers }
    }

    /// Queues `job` behind the jobs already waiting.
    pub fn submit(&self, job: J) -> JobHandle {
        self.enqueue(job, None)
    }

    /// Queues `job`; `on_finished` runs on the worker once it is final, whatever its status.
    pub fn submit_with_listener(&self, job: J, on_finished: OnFinished) -> JobHandle {
        self.enqueue(job, Some(on_finished))
    }

    /// A handle that is already `Failed` with `message`. Nothing is queued and no listener runs.
    pub fn submit_failed(&self, message: impl Into<String>) -> JobHandle {
        let shared = {
            let mut state = self.shared.lock();
            let shared = Shared::new(state.next_id);
            state.next_id += 1;
            shared
        };
        shared.fail(message.into());
        JobHandle { shared }
    }

    /// Cancels every queued and running job.
    pub fn cancel_all(&self) {
        cancel_all(&self.shared.lock());
    }

    /// Jobs waiting to start.
    pub fn queued_len(&self) -> usize {
        self.shared.lock().queue.len()
    }

    fn enqueue(&self, job: J, on_finished: Option<OnFinished>) -> JobHandle {
        let mut state = self.shared.lock();
        let shared = Shared::new(state.next_id);
        state.next_id += 1;
        state.queue.push_back(Queued {
            shared: Arc::clone(&shared),
            job,
            on_finished,
        });
        self.shared.wake.notify_one();
        JobHandle { shared }
    }
}

impl<J: Send + 'static> Drop for WorkerPool<J> {
    /// Cancels queued and running jobs, then joins every worker.
    fn drop(&mut self) {
        {
            let mut state = self.shared.lock();
            state.stopping = true;
            cancel_all(&state);
        }
        self.shared.wake.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn cancel_all<J>(state: &State<J>) {
    for queued in &state.queue {
        queued.shared.cancelled.store(true, Ordering::Release);
    }
    for running in &state.running {
        running.cancelled.store(true, Ordering::Release);
    }
}

fn worker_loop<J: Send + 'static>(shared: &PoolShared<J>) {
    while let Some(queued) = next_queued(shared) {
        run(shared, queued);
    }
}

/// Blocks for the next queued job, moving it to the running list. `None` once the pool is
/// stopping and the queue is empty; while stopping, queued jobs are still taken (and skipped as
/// cancelled), so each one still reaches a final status.
fn next_queued<J>(shared: &PoolShared<J>) -> Option<Queued<J>> {
    let state = shared.lock();
    let mut state = shared
        .wake
        .wait_while(state, |s| !s.stopping && s.queue.is_empty())
        .unwrap_or_else(PoisonError::into_inner);
    let queued = state.queue.pop_front()?;
    state.running.push(Arc::clone(&queued.shared));
    Some(queued)
}

fn run<J>(shared: &PoolShared<J>, queued: Queued<J>) {
    let Queued {
        shared: job_shared,
        job,
        on_finished,
    } = queued;
    if !job_shared.cancelled.load(Ordering::Acquire) {
        job_shared.set_status(JobStatus::Running);
        let ctx = JobCtx {
            shared: Arc::clone(&job_shared),
        };
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| (shared.handler)(&ctx, job)));
        if let Err(payload) = outcome {
            job_shared.fail(format!("job panicked: {}", panic_message(payload.as_ref())));
        }
    }
    job_shared.finish();
    shared
        .lock()
        .running
        .retain(|running| !Arc::ptr_eq(running, &job_shared));
    if let Some(listener) = on_finished {
        let handle = JobHandle { shared: job_shared };
        // The job is already final; a listener that panics has nothing left to report to.
        let _ = panic::catch_unwind(AssertUnwindSafe(|| listener(&handle)));
    }
}

/// The text of a panic payload (`panic!` gives a `&str` or a `String`).
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_owned())
}

#[cfg(test)]
mod tests;
