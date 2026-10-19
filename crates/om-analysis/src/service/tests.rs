//! Port of `tests/unit/AnalysisServiceTest.cpp` (real threads; the cases are fast on short clips).

use super::*;
use crate::test_signals::{silence, write_float_wav, TempFile, RATE};
use crate::{
    hash_file_content, AnalysisContext, Analyzer, AudioClip, FileClipSource, JobStatus,
    MemoryClipSource, SqliteResultCache,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(10);

/// Reports the clip length and counts how often it actually ran.
struct CountingAnalyzer {
    name: &'static str,
    calls: AtomicUsize,
}

impl CountingAnalyzer {
    fn new(name: &'static str) -> Arc<Self> {
        Arc::new(Self {
            name,
            calls: AtomicUsize::new(0),
        })
    }
}

impl Analyzer for CountingAnalyzer {
    fn id(&self) -> &str {
        self.name
    }

    fn version(&self) -> i32 {
        1
    }

    fn analyze(&self, clip: &AudioClip, context: &dyn AnalysisContext) -> Option<String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        context.set_progress(0.5);
        Some(clip.samples.len().to_string())
    }
}

/// Spins until cancelled, so tests control exactly when it ends.
#[derive(Default)]
struct BlockingAnalyzer {
    started: AtomicBool,
}

impl Analyzer for BlockingAnalyzer {
    fn id(&self) -> &str {
        "block"
    }

    fn version(&self) -> i32 {
        1
    }

    fn analyze(&self, _clip: &AudioClip, context: &dyn AnalysisContext) -> Option<String> {
        self.started.store(true, Ordering::SeqCst);
        while !context.is_cancelled() {
            thread::sleep(Duration::from_millis(1));
        }
        None
    }
}

struct FailingAnalyzer;

impl Analyzer for FailingAnalyzer {
    fn id(&self) -> &str {
        "fail"
    }

    fn version(&self) -> i32 {
        1
    }

    fn analyze(&self, _clip: &AudioClip, _context: &dyn AnalysisContext) -> Option<String> {
        None
    }
}

fn clip_source(seconds: f64) -> Option<Box<dyn ClipSource>> {
    Some(Box::new(MemoryClipSource::new(
        silence(seconds, RATE),
        "silence",
    )))
}

fn ids(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| (*s).to_owned()).collect()
}

struct Fixture {
    registry: Arc<AnalyzerRegistry>,
    counting: Arc<CountingAnalyzer>,
    other: Arc<CountingAnalyzer>,
    blocking: Arc<BlockingAnalyzer>,
    cache: Arc<dyn ResultCache>,
}

impl Fixture {
    fn new() -> Self {
        let registry = AnalyzerRegistry::new();
        let counting = CountingAnalyzer::new("count");
        let other = CountingAnalyzer::new("other");
        let blocking = Arc::new(BlockingAnalyzer::default());
        registry.add(counting.clone()).expect("count");
        registry.add(other.clone()).expect("other");
        registry.add(blocking.clone()).expect("block");
        registry.add(Arc::new(FailingAnalyzer)).expect("fail");
        Self {
            registry: Arc::new(registry),
            counting,
            other,
            blocking,
            cache: Arc::new(SqliteResultCache::open(":memory:").expect("cache")),
        }
    }
}

/// Polls `flag` for up to the test timeout.
fn wait_flag(flag: &AtomicBool) {
    let start = Instant::now();
    while !flag.load(Ordering::SeqCst) && start.elapsed() < WAIT {
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn runs_analysers_and_reports_results() {
    let f = Fixture::new();
    let listener_calls = Arc::new(AtomicUsize::new(0));
    let service = AnalysisService::new(f.registry.clone(), Some(f.cache.clone()), 2);

    let counter = Arc::clone(&listener_calls);
    let job = service.submit(
        clip_source(0.1),
        ids(&["count", "other"]),
        Some(Box::new(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        })),
    );
    assert!(job.wait_until_finished(WAIT));

    assert_eq!(job.status(), JobStatus::Done);
    assert_eq!(job.progress(), 1.0);
    let results = job.results();
    assert_eq!(results["count"], "4410");
    assert_eq!(results["other"], "4410");
    assert_eq!(job.content_hash().len(), 64);
    service.cancel_all();
    let start = Instant::now();
    while listener_calls.load(Ordering::SeqCst) == 0 && start.elapsed() < WAIT {
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(listener_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn second_job_for_same_content_is_served_from_cache() {
    let f = Fixture::new();
    let service = AnalysisService::new(f.registry.clone(), Some(f.cache.clone()), 1);

    let first = service.submit(clip_source(0.1), ids(&["count"]), None);
    assert!(first.wait_until_finished(WAIT));
    let second = service.submit(clip_source(0.1), ids(&["count", "other"]), None);
    assert!(second.wait_until_finished(WAIT));

    assert_eq!(f.counting.calls.load(Ordering::SeqCst), 1);
    assert_eq!(f.other.calls.load(Ordering::SeqCst), 1);
    assert_eq!(second.cached_ids(), ids(&["count"]));
    assert_eq!(second.results()["count"], "4410");
}

#[test]
fn works_without_cache() {
    let f = Fixture::new();
    let service = AnalysisService::new(f.registry.clone(), None, 1);
    let job = service.submit(clip_source(0.1), ids(&["count"]), None);
    assert!(job.wait_until_finished(WAIT));
    assert_eq!(job.status(), JobStatus::Done);
}

#[test]
fn cancel_stops_running_and_queued_jobs() {
    let f = Fixture::new();
    let service = AnalysisService::new(f.registry.clone(), Some(f.cache.clone()), 1);

    let running = service.submit(clip_source(0.1), ids(&["block"]), None);
    let queued = service.submit(clip_source(0.2), ids(&["count"]), None);
    wait_flag(&f.blocking.started);
    assert_eq!(service.queued_count(), 1);

    queued.cancel();
    running.cancel();
    assert!(running.wait_until_finished(WAIT));
    assert!(queued.wait_until_finished(WAIT));
    assert_eq!(running.status(), JobStatus::Cancelled);
    assert_eq!(queued.status(), JobStatus::Cancelled);
    assert_eq!(f.counting.calls.load(Ordering::SeqCst), 0);
    assert!(f.cache.find(&running.content_hash(), "block", 1).is_none());
}

#[test]
fn failures_are_reported() {
    let f = Fixture::new();
    let service = AnalysisService::new(f.registry.clone(), Some(f.cache.clone()), 1);

    let unknown = service.submit(clip_source(0.1), ids(&["nope"]), None);
    let failing = service.submit(clip_source(0.1), ids(&["fail"]), None);
    let unreadable = service.submit(
        Some(Box::new(FileClipSource::new("/nonexistent.wav"))),
        ids(&["count"]),
        None,
    );
    let null = service.submit(None, ids(&["count"]), None);

    for job in [&unknown, &failing, &unreadable, &null] {
        assert!(job.wait_until_finished(WAIT));
        assert_eq!(job.status(), JobStatus::Failed);
        assert!(!job.error().is_empty());
    }
}

#[test]
fn destructor_cancels_outstanding_jobs() {
    let f = Fixture::new();
    let (running, queued) = {
        let service = AnalysisService::new(f.registry.clone(), Some(f.cache.clone()), 1);
        let running = service.submit(clip_source(0.1), ids(&["block"]), None);
        let queued = service.submit(clip_source(0.1), ids(&["count"]), None);
        wait_flag(&f.blocking.started);
        (running, queued)
    };
    assert_eq!(running.status(), JobStatus::Cancelled);
    assert_eq!(queued.status(), JobStatus::Cancelled);
}

#[test]
fn analyses_wav_files_with_progress() {
    let f = Fixture::new();
    let temp = TempFile::new("wav");
    write_float_wav(temp.path(), 1, 44_100, &vec![0.0f32; 44_100]);

    let service = AnalysisService::new(f.registry.clone(), Some(f.cache.clone()), 1);
    let job = service.submit(
        Some(Box::new(FileClipSource::new(temp.path()))),
        ids(&["count"]),
        None,
    );
    assert!(job.wait_until_finished(WAIT));
    assert_eq!(job.results()["count"], "44100");
    assert_eq!(
        job.content_hash(),
        hash_file_content(temp.path()).expect("hashable")
    );
}

/// Panics inside the analysis, as a buggy third-party analyser might.
struct PanickingAnalyzer;

impl Analyzer for PanickingAnalyzer {
    fn id(&self) -> &str {
        "boom"
    }

    fn version(&self) -> i32 {
        1
    }

    fn analyze(&self, _clip: &AudioClip, _context: &dyn AnalysisContext) -> Option<String> {
        panic!("analyser exploded");
    }
}

#[test]
fn a_panicking_analyser_fails_its_job_and_the_single_worker_carries_on() {
    let registry = AnalyzerRegistry::new();
    registry.add(Arc::new(PanickingAnalyzer)).expect("boom");
    registry.add(CountingAnalyzer::new("count")).expect("count");
    let service = AnalysisService::new(Arc::new(registry), None, 1);

    let (tx, rx) = mpsc::channel();
    let listener: Listener = Box::new(move |job| {
        let _ = tx.send((job.status(), job.error()));
    });
    let first = service.submit(clip_source(0.1), ids(&["boom"]), Some(listener));
    assert!(first.wait_until_finished(WAIT), "the job must finish");
    assert_eq!(first.status(), JobStatus::Failed);
    assert!(first.error().contains("analyser exploded"), "{}", first.error());
    let (status, error) = rx.recv_timeout(WAIT).expect("listener still fires");
    assert_eq!(status, JobStatus::Failed);
    assert!(error.contains("analyser exploded"));

    let second = service.submit(clip_source(0.1), ids(&["count"]), None);
    assert!(second.wait_until_finished(WAIT), "the worker must survive");
    assert_eq!(second.status(), JobStatus::Done);
}

#[test]
fn a_panicking_listener_does_not_stop_the_worker() {
    let registry = AnalyzerRegistry::new();
    registry.add(CountingAnalyzer::new("count")).expect("count");
    let service = AnalysisService::new(Arc::new(registry), None, 1);

    let panicking: Listener = Box::new(|_| panic!("listener exploded"));
    let first = service.submit(clip_source(0.1), ids(&["count"]), Some(panicking));
    assert!(first.wait_until_finished(WAIT));

    let (tx, rx) = mpsc::channel();
    let listener: Listener = Box::new(move |job| {
        let _ = tx.send(job.status());
    });
    let second = service.submit(clip_source(0.1), ids(&["count"]), Some(listener));
    assert!(second.wait_until_finished(WAIT));
    assert_eq!(rx.recv_timeout(WAIT).expect("second listener fires"), JobStatus::Done);
}

