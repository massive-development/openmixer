//! Pool cases on real threads. Every wait is on a condition (or a handle), never a fixed sleep.

use super::*;
use om_testing::wait::{wait_until, DEFAULT_POLL};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;

const WAIT: Duration = Duration::from_secs(10);

/// Blocks until cancelled, so a test decides exactly when the job ends.
fn spin_until_cancelled(ctx: &JobCtx) {
    wait_until(WAIT, DEFAULT_POLL, || ctx.is_cancelled());
}

fn flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

#[test]
fn one_worker_runs_jobs_in_fifo_order() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&order);
    let pool = WorkerPool::new(1, "fifo", move |_ctx: &JobCtx, n: u32| {
        seen.lock_unpoisoned().push(n);
    });

    let handles: Vec<_> = (0..5).map(|n| pool.submit(n)).collect();
    for handle in &handles {
        assert!(handle.wait(WAIT));
        assert_eq!(handle.status(), JobStatus::Done);
    }
    assert_eq!(*order.lock_unpoisoned(), vec![0, 1, 2, 3, 4]);
}

#[test]
fn two_workers_run_jobs_in_parallel() {
    let inside = Arc::new(AtomicUsize::new(0));
    let met = Arc::new(AtomicBool::new(false));
    let (i, m) = (Arc::clone(&inside), Arc::clone(&met));
    let pool = WorkerPool::new(2, "par", move |_ctx: &JobCtx, _n: u32| {
        i.fetch_add(1, Ordering::SeqCst);
        // Both jobs must be inside the handler at once for this to hold.
        if wait_until(WAIT, DEFAULT_POLL, || i.load(Ordering::SeqCst) == 2) {
            m.store(true, Ordering::SeqCst);
        }
    });

    let a = pool.submit(1);
    let b = pool.submit(2);
    assert!(a.wait(WAIT));
    assert!(b.wait(WAIT));
    assert!(met.load(Ordering::SeqCst), "two workers never overlapped");
}

#[test]
fn cancelling_a_queued_job_skips_it() {
    let started = flag();
    let ran = Arc::new(AtomicUsize::new(0));
    let (s, r) = (Arc::clone(&started), Arc::clone(&ran));
    let pool = WorkerPool::new(1, "queued", move |ctx: &JobCtx, n: u32| {
        if n == 0 {
            s.store(true, Ordering::SeqCst);
            spin_until_cancelled(ctx);
        } else {
            r.fetch_add(1, Ordering::SeqCst);
        }
    });

    let blocker = pool.submit(0);
    let queued = pool.submit(1);
    assert!(wait_until(WAIT, DEFAULT_POLL, || started.load(Ordering::SeqCst)));
    assert_eq!(pool.queued_len(), 1);

    queued.cancel();
    blocker.cancel();
    assert!(queued.wait(WAIT));
    assert_eq!(queued.status(), JobStatus::Cancelled);
    assert!(blocker.wait(WAIT));
    assert_eq!(blocker.status(), JobStatus::Cancelled);
    assert_eq!(
        ran.load(Ordering::SeqCst),
        0,
        "a cancelled queued job must not run"
    );
    assert_eq!(pool.queued_len(), 0);
}

#[test]
fn cancelling_a_running_job_stops_it_cooperatively() {
    let started = flag();
    let exited = flag();
    let (s, e) = (Arc::clone(&started), Arc::clone(&exited));
    let pool = WorkerPool::new(1, "running", move |ctx: &JobCtx, _n: u32| {
        s.store(true, Ordering::SeqCst);
        spin_until_cancelled(ctx);
        e.store(true, Ordering::SeqCst);
    });

    let job = pool.submit(0);
    assert!(wait_until(WAIT, DEFAULT_POLL, || started.load(Ordering::SeqCst)));
    assert_eq!(job.status(), JobStatus::Running);

    job.cancel();
    assert!(job.wait(WAIT));
    assert_eq!(job.status(), JobStatus::Cancelled);
    assert!(
        exited.load(Ordering::SeqCst),
        "the handler returned on its own"
    );
}

#[test]
fn a_panicking_handler_fails_its_job_and_the_worker_serves_the_next() {
    let pool = WorkerPool::new(1, "panic", |_ctx: &JobCtx, n: u32| {
        if n == 0 {
            panic!("handler exploded");
        }
    });
    let (tx, rx) = mpsc::channel();
    let listener: OnFinished = Arc::new(move |handle: &JobHandle| {
        let _ = tx.send((handle.status(), handle.error()));
    });

    let first = pool.submit_with_listener(0, listener);
    assert!(first.wait(WAIT));
    assert_eq!(first.status(), JobStatus::Failed);
    assert!(
        first.error().contains("handler exploded"),
        "{}",
        first.error()
    );
    let (status, error) = rx.recv_timeout(WAIT).expect("listener fires after a panic");
    assert_eq!(status, JobStatus::Failed);
    assert!(error.contains("handler exploded"));

    let second = pool.submit(1);
    assert!(second.wait(WAIT), "the worker must survive the panic");
    assert_eq!(second.status(), JobStatus::Done);
}

#[test]
fn a_panicking_listener_does_not_stop_the_worker() {
    let pool = WorkerPool::new(1, "listener", |_ctx: &JobCtx, _n: u32| {});
    let panicking: OnFinished = Arc::new(|_| panic!("listener exploded"));
    let first = pool.submit_with_listener(0, panicking);
    assert!(first.wait(WAIT));

    let (tx, rx) = mpsc::channel();
    let listener: OnFinished = Arc::new(move |handle: &JobHandle| {
        let _ = tx.send(handle.status());
    });
    let second = pool.submit_with_listener(1, listener);
    assert!(second.wait(WAIT));
    assert_eq!(
        rx.recv_timeout(WAIT).expect("second listener fires"),
        JobStatus::Done
    );
}

#[test]
fn ctx_fail_marks_the_job_failed_and_fires_the_listener() {
    let pool = WorkerPool::new(1, "fail", |ctx: &JobCtx, _n: u32| {
        ctx.fail("bad input");
        assert!(!ctx.is_cancelled());
    });
    let (tx, rx) = mpsc::channel();
    let listener: OnFinished = Arc::new(move |handle: &JobHandle| {
        let _ = tx.send(handle.status());
    });
    let job = pool.submit_with_listener(0, listener);
    assert!(job.wait(WAIT));
    assert_eq!(job.status(), JobStatus::Failed);
    assert_eq!(job.error(), "bad input");
    assert_eq!(
        rx.recv_timeout(WAIT).expect("listener fires"),
        JobStatus::Failed
    );
}

#[test]
fn dropping_the_pool_cancels_and_joins_the_workers() {
    let started = flag();
    let token = Arc::new(());
    let (s, held) = (Arc::clone(&started), Arc::clone(&token));
    let pool = WorkerPool::new(1, "drop", move |ctx: &JobCtx, _n: u32| {
        let _keep = &held;
        s.store(true, Ordering::SeqCst);
        spin_until_cancelled(ctx);
    });
    let running = pool.submit(0);
    let queued = pool.submit(1);
    assert!(wait_until(WAIT, DEFAULT_POLL, || started.load(Ordering::SeqCst)));

    drop(pool);

    // Drop returned only after the workers were joined, so both jobs are final already.
    assert_eq!(running.status(), JobStatus::Cancelled);
    assert_eq!(queued.status(), JobStatus::Cancelled);
    assert_eq!(
        Arc::strong_count(&token),
        1,
        "the handler (and what it captured) is released by the time drop returns"
    );
}

#[test]
fn progress_is_visible_while_the_job_runs() {
    let reported = flag();
    let release = flag();
    let (r, g) = (Arc::clone(&reported), Arc::clone(&release));
    let pool = WorkerPool::new(1, "progress", move |ctx: &JobCtx, _n: u32| {
        ctx.set_progress(0.5);
        r.store(true, Ordering::SeqCst);
        wait_until(WAIT, DEFAULT_POLL, || g.load(Ordering::SeqCst));
    });

    let job = pool.submit(0);
    assert!(wait_until(WAIT, DEFAULT_POLL, || reported.load(Ordering::SeqCst)));
    assert_eq!(job.progress(), 0.5);
    assert_eq!(job.status(), JobStatus::Running);

    release.store(true, Ordering::SeqCst);
    assert!(job.wait(WAIT));
    assert_eq!(job.progress(), 1.0, "a finished job reads complete");
}

#[test]
fn cancel_all_cancels_running_and_queued_jobs() {
    let started = flag();
    let s = Arc::clone(&started);
    let pool = WorkerPool::new(1, "cancel-all", move |ctx: &JobCtx, _n: u32| {
        s.store(true, Ordering::SeqCst);
        spin_until_cancelled(ctx);
    });
    let running = pool.submit(0);
    let queued_a = pool.submit(1);
    let queued_b = pool.submit(2);
    assert!(wait_until(WAIT, DEFAULT_POLL, || started.load(Ordering::SeqCst)));

    pool.cancel_all();
    for job in [&running, &queued_a, &queued_b] {
        assert!(job.wait(WAIT));
        assert_eq!(job.status(), JobStatus::Cancelled);
    }
}

#[test]
fn submit_failed_is_final_and_queues_nothing() {
    let pool = WorkerPool::new(1, "rejected", |_ctx: &JobCtx, _n: u32| {
        panic!("must never run");
    });
    let job = pool.submit_failed("no source");
    assert_eq!(job.status(), JobStatus::Failed);
    assert_eq!(job.error(), "no source");
    assert!(job.wait(Duration::ZERO));
    assert_eq!(pool.queued_len(), 0);
}
