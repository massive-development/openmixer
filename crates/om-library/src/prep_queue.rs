//! Track prep queue: feeds the persisted queue (`PrepQueueDao`) to the shared `AnalysisService`
//! and writes BPM and key results back into the library.
//!
//! Threading: owned by the thread that owns the read-write `Library` (the owner thread). Every
//! method, every library access and every observer call happens on that thread. Analysis worker
//! threads only push finished jobs into a mutex-guarded inbox, so a SQLite connection is never
//! shared. The owner calls `pump()` regularly (a UI timer, or `pump_until_idle()` in tests):
//! `pump()` applies finished results, then tops up the service. Only `max_in_flight` jobs are
//! ever submitted at once, so the order of the rest stays editable (move, prioritise, cancel).

use crate::error::Result;
use crate::library::Library;
use crate::prep_queue_dao::{PrepEntry, PrepStatus};
use crate::prep_results::apply_prep_results;
use crate::track_record::TrackRecord;
use om_analysis::{AnalysisJob, AnalysisService, ClipSource, FileClipSource, JobStatus};
use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Builds the clip source for a track. Default: the track's file.
pub type SourceFactory = Box<dyn Fn(&TrackRecord) -> Box<dyn ClipSource>>;
/// Owner thread, from `pump()` or a mutating call: an item changed status or progress.
pub type Observer = Box<dyn FnMut(&PrepEntry)>;
/// Database or write-back failures.
pub type ErrorHandler = Box<dyn Fn(&str)>;

/// Queue configuration.
pub struct PrepOptions {
    pub analyzer_ids: Vec<String>,
    /// Jobs handed to the service at once (at least 1).
    pub max_in_flight: usize,
    pub source_factory: Option<SourceFactory>,
    pub observer: Option<Observer>,
    pub on_error: Option<ErrorHandler>,
}

impl Default for PrepOptions {
    fn default() -> Self {
        Self {
            analyzer_ids: ["bpm", "key", "waveform", "structure"]
                .map(str::to_owned)
                .to_vec(),
            max_in_flight: 2,
            source_factory: None,
            observer: None,
            on_error: None,
        }
    }
}

/// A finished job, snapshotted on the worker thread; the owner thread applies it.
struct Finished {
    track_id: i64,
    generation: i64,
    status: JobStatus,
    results: BTreeMap<String, String>,
    error: String,
}

#[derive(Default)]
struct Inbox {
    done: Mutex<Vec<Finished>>,
    wake: Condvar,
}

struct InFlight {
    job: Arc<AnalysisJob>,
    generation: i64,
    reported: f32,
}

pub struct PrepQueue<L: Borrow<Library>, S: Borrow<AnalysisService>> {
    lib: L,
    service: S,
    opts: PrepOptions,
    /// Shared with listeners that may outlive the queue.
    inbox: Arc<Inbox>,
    /// Owner thread only.
    in_flight: BTreeMap<i64, InFlight>,
    /// Cancelled jobs still unwinding; their results are ignored.
    draining: Vec<InFlight>,
    next_generation: i64,
}

impl<L: Borrow<Library>, S: Borrow<AnalysisService>> PrepQueue<L, S> {
    /// Library and service must outlive the queue. Entries left Running by a crash go back to
    /// Queued.
    pub fn new(library: L, service: S, mut options: PrepOptions) -> Self {
        options.max_in_flight = options.max_in_flight.max(1);
        let reset = library.borrow().prep_queue().reset_running().err();
        let queue = Self {
            lib: library,
            service,
            opts: options,
            inbox: Arc::new(Inbox::default()),
            in_flight: BTreeMap::new(),
            draining: Vec::new(),
            next_generation: 1,
        };
        if let Some(e) = reset {
            queue.error(&e.to_string());
        }
        queue
    }

    /// Queues a track. `urgent` runs it before everything queued.
    pub fn enqueue(&mut self, track_id: i64, urgent: bool) -> Result<()> {
        let dao = self.lib.borrow().prep_queue();
        dao.enqueue(track_id)?;
        if urgent {
            dao.prioritise(track_id)?;
        }
        self.notify(track_id);
        Ok(())
    }

    pub fn prioritise(&mut self, track_id: i64) -> Result<()> {
        let result = self.lib.borrow().prep_queue().prioritise(track_id);
        self.notify(track_id);
        result
    }

    /// Moves within the user order.
    pub fn move_to(&mut self, track_id: i64, index: i32) -> Result<()> {
        self.lib.borrow().prep_queue().move_to(track_id, index)
    }

    /// Cancels the track first when it is running, then removes its row.
    pub fn remove(&mut self, track_id: i64) -> Result<()> {
        self.cancel(track_id);
        self.lib.borrow().prep_queue().remove(track_id)
    }

    /// Queued or Running -> Cancelled.
    pub fn cancel(&mut self, track_id: i64) {
        if self.in_flight.contains_key(&track_id) {
            return self.cancel_running(track_id);
        }
        match self.lib.borrow().prep_queue().find(track_id) {
            Err(e) => self.error(&e.to_string()),
            Ok(Some(entry)) if entry.status == PrepStatus::Queued => {
                if let Err(e) =
                    self.lib
                        .borrow()
                        .prep_queue()
                        .set_status(track_id, PrepStatus::Cancelled, "")
                {
                    self.error(&e.to_string());
                }
                self.notify(track_id);
            }
            Ok(_) => {}
        }
    }

    pub fn cancel_all(&mut self) {
        match self.lib.borrow().prep_queue().all() {
            Err(e) => self.error(&e.to_string()),
            Ok(entries) => {
                for e in entries {
                    if matches!(e.status, PrepStatus::Queued | PrepStatus::Running) {
                        self.cancel(e.track_id);
                    }
                }
            }
        }
    }

    pub fn clear_finished(&self) -> Result<()> {
        self.lib.borrow().prep_queue().clear_finished()
    }

    /// Applies finished jobs and submits the next ones. Returns true while work remains.
    pub fn pump(&mut self) -> bool {
        self.drain();
        self.fill();
        self.report_progress();
        if !self.in_flight.is_empty() || !self.draining.is_empty() {
            return true;
        }
        match self.lib.borrow().prep_queue().next_queued() {
            // An unreadable queue is not known to be idle.
            Err(e) => {
                self.error(&e.to_string());
                true
            }
            Ok(next) => next.is_some(),
        }
    }

    /// Pumps until the queue is idle (true) or the timeout hits (false). Sleeps on the inbox
    /// between rounds.
    pub fn pump_until_idle(&mut self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while self.pump() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            self.wait_for_inbox(left.min(Duration::from_millis(50)));
        }
        true
    }

    /// The queue in user order, with live progress for running items.
    pub fn list(&self) -> Result<Vec<PrepEntry>> {
        let mut entries = self.lib.borrow().prep_queue().all()?;
        for e in &mut entries {
            if let Some(f) = self.in_flight.get(&e.track_id) {
                e.progress = f.job.progress();
            }
        }
        Ok(entries)
    }

    fn cancel_running(&mut self, track_id: i64) {
        if let Some(job) = self.in_flight.remove(&track_id) {
            job.job.cancel();
            // So a re-enqueue can start a fresh job at once.
            self.draining.push(job);
        }
        if let Err(e) =
            self.lib
                .borrow()
                .prep_queue()
                .set_status(track_id, PrepStatus::Cancelled, "")
        {
            self.error(&e.to_string());
        }
        self.notify(track_id);
    }

    fn drain(&mut self) {
        let finished = {
            let mut done = self
                .inbox
                .done
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            std::mem::take(&mut *done)
        };
        for f in finished {
            self.complete(f);
        }
    }

    fn complete(&mut self, finished: Finished) {
        if let Some(pos) = self
            .draining
            .iter()
            .position(|f| f.generation == finished.generation)
        {
            // Cancelled earlier: the row was already settled.
            self.draining.remove(pos);
            return;
        }
        match self.in_flight.get(&finished.track_id) {
            Some(f) if f.generation == finished.generation => {}
            _ => return,
        }
        self.in_flight.remove(&finished.track_id);
        let dao = self.lib.borrow().prep_queue();
        match finished.status {
            JobStatus::Done => {
                if let Err(e) = apply_prep_results(
                    &self.lib.borrow().tracks(),
                    finished.track_id,
                    &finished.results,
                ) {
                    return self.fail(finished.track_id, &e.to_string());
                }
                if let Err(e) = dao.set_status(finished.track_id, PrepStatus::Done, "") {
                    self.error(&e.to_string());
                }
            }
            JobStatus::Cancelled => {
                if let Err(e) = dao.set_status(finished.track_id, PrepStatus::Cancelled, "") {
                    self.error(&e.to_string());
                }
            }
            _ => return self.fail(finished.track_id, &finished.error),
        }
        self.notify(finished.track_id);
    }

    fn fail(&mut self, track_id: i64, message: &str) {
        if let Err(e) =
            self.lib
                .borrow()
                .prep_queue()
                .set_status(track_id, PrepStatus::Failed, message)
        {
            self.error(&e.to_string());
        }
        self.notify(track_id);
    }

    fn fill(&mut self) {
        while self.in_flight.len() + self.draining.len() < self.opts.max_in_flight
            && self.submit_next()
        {}
    }

    /// Submits the next queued track. False when there is nothing to submit (or the queue is
    /// unreadable, which is retried on the next pump).
    fn submit_next(&mut self) -> bool {
        let next = match self.lib.borrow().prep_queue().next_queued() {
            Ok(Some(next)) => next,
            Ok(None) => return false,
            Err(e) => {
                self.error(&e.to_string());
                return false;
            }
        };
        let id = next.track_id;
        match self.lib.borrow().tracks().find_by_id(id) {
            Err(e) => self.fail(id, &e.to_string()),
            Ok(None) => self.fail(id, "track no longer exists"),
            Ok(Some(track)) => {
                if let Err(e) =
                    self.lib
                        .borrow()
                        .prep_queue()
                        .set_status(id, PrepStatus::Running, "")
                {
                    self.error(&e.to_string());
                    return false;
                }
                let generation = self.next_generation;
                self.next_generation += 1;
                let source = self.make_source(&track);
                let inbox = Arc::clone(&self.inbox);
                let job = self.service.borrow().submit(
                    Some(source),
                    self.opts.analyzer_ids.clone(),
                    Some(Box::new(move |job: &AnalysisJob| {
                        post(&inbox, id, generation, job)
                    })),
                );
                self.in_flight.insert(
                    id,
                    InFlight {
                        job,
                        generation,
                        reported: 0.0,
                    },
                );
            }
        }
        true
    }

    fn make_source(&self, track: &TrackRecord) -> Box<dyn ClipSource> {
        match &self.opts.source_factory {
            Some(factory) => factory(track),
            None => Box::new(FileClipSource::new(track.path.clone())),
        }
    }

    fn report_progress(&mut self) {
        let changed: Vec<i64> = self
            .in_flight
            .iter_mut()
            .filter_map(|(&id, f)| {
                let progress = f.job.progress();
                (progress != f.reported).then(|| {
                    f.reported = progress;
                    id
                })
            })
            .collect();
        for id in changed {
            self.notify(id);
        }
    }

    fn notify(&mut self, track_id: i64) {
        if self.opts.observer.is_none() {
            return;
        }
        match self.lib.borrow().prep_queue().find(track_id) {
            Err(e) => self.error(&e.to_string()),
            Ok(Some(mut entry)) => {
                if let Some(f) = self.in_flight.get(&track_id) {
                    entry.progress = f.job.progress();
                }
                if let Some(observer) = self.opts.observer.as_mut() {
                    observer(&entry);
                }
            }
            Ok(None) => {}
        }
    }

    fn error(&self, message: &str) {
        if let Some(handler) = &self.opts.on_error {
            handler(message);
        }
    }

    fn wait_for_inbox(&self, timeout: Duration) {
        let done = self
            .inbox
            .done
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let _ = self
            .inbox
            .wake
            .wait_timeout_while(done, timeout, |d| d.is_empty())
            .unwrap_or_else(PoisonError::into_inner);
    }
}

impl<L: Borrow<Library>, S: Borrow<AnalysisService>> Drop for PrepQueue<L, S> {
    /// Cancels in-flight jobs. Their rows stay Running until the next start resets them.
    fn drop(&mut self) {
        for f in self.in_flight.values().chain(self.draining.iter()) {
            f.job.cancel();
        }
    }
}

/// Listener body, run on a worker thread: snapshot the finished job into the inbox.
fn post(inbox: &Inbox, track_id: i64, generation: i64, job: &AnalysisJob) {
    let finished = Finished {
        track_id,
        generation,
        status: job.status(),
        results: job.results(),
        error: job.error(),
    };
    inbox
        .done
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(finished);
    inbox.wake.notify_all();
}
