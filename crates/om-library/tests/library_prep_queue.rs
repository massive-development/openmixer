//! The persisted prep queue feeding the analysis
//! service.

mod common;

use common::{write_wav, Tags, TempDir};
use om_analysis::{
    AnalysisContext, AnalysisService, Analyzer, AnalyzerRegistry, AudioClip, BeatGridModel,
    BpmResult, ClipSource, KeyResult, MemoryClipSource, MusicalKey,
};
use om_library::{
    core_migrations, IngestSummary, Ingestor, Library, MetadataReaderRegistry, OpenMode, PrepEntry,
    PrepOptions, PrepQueue, PrepStatus, TrackRecord,
};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, ThreadId};
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(10);

/// Returns a fixed payload and records the clip length (= track id * 1000) in run order.
struct FakeAnalyzer {
    name: String,
    payload: String,
    delay_ms: AtomicU64,
    order: Mutex<Vec<i64>>,
    running: AtomicI32,
    peak: AtomicI32,
}

impl FakeAnalyzer {
    fn new(name: &str, payload: String) -> Self {
        Self {
            name: name.to_owned(),
            payload,
            delay_ms: AtomicU64::new(0),
            order: Mutex::new(Vec::new()),
            running: AtomicI32::new(0),
            peak: AtomicI32::new(0),
        }
    }
}

impl Analyzer for FakeAnalyzer {
    fn id(&self) -> &str {
        &self.name
    }

    fn version(&self) -> i32 {
        1
    }

    fn analyze(&self, clip: &AudioClip, _context: &dyn AnalysisContext) -> Option<String> {
        let now = self.running.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        if self.name == "bpm" {
            self.order
                .lock()
                .unwrap()
                .push(clip.samples.len() as i64 / 1000);
        }
        let delay = self.delay_ms.load(Ordering::SeqCst);
        if delay > 0 {
            thread::sleep(Duration::from_millis(delay));
        }
        self.running.fetch_sub(1, Ordering::SeqCst);
        Some(self.payload.clone())
    }
}

/// Spins until cancelled.
#[derive(Default)]
struct GateAnalyzer {
    started: AtomicBool,
}

impl Analyzer for GateAnalyzer {
    fn id(&self) -> &str {
        "waveform"
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

/// Always fails.
struct NullAnalyzer;

impl Analyzer for NullAnalyzer {
    fn id(&self) -> &str {
        "structure"
    }

    fn version(&self) -> i32 {
        1
    }

    fn analyze(&self, _clip: &AudioClip, _context: &dyn AnalysisContext) -> Option<String> {
        None
    }
}

fn bpm_json(bpm: f64) -> String {
    BpmResult {
        bpm,
        confidence: 0.0,
        grid: BeatGridModel::default(),
    }
    .to_json()
}

fn key_json(tonic: i32, minor: bool, tonal: bool) -> String {
    KeyResult {
        key: MusicalKey::new(tonic, minor),
        confidence: 0.0,
        chroma: [0.0; 12],
        tonal,
    }
    .to_json()
}

struct Fixture {
    bpm: Arc<FakeAnalyzer>,
    registry: Arc<AnalyzerRegistry>,
    lib: Library,
    service: AnalysisService,
    events: Arc<Mutex<Vec<PrepEntry>>>,
    event_threads: Arc<Mutex<Vec<ThreadId>>>,
    errors: Arc<Mutex<Vec<String>>>,
    counter: i32,
}

impl Fixture {
    fn new(workers: usize) -> Self {
        Self::open(workers, ":memory:")
    }

    fn open(workers: usize, path: &str) -> Self {
        let bpm = Arc::new(FakeAnalyzer::new("bpm", bpm_json(128.0)));
        let registry = Arc::new(AnalyzerRegistry::new());
        registry.add(bpm.clone()).unwrap();
        registry
            .add(Arc::new(FakeAnalyzer::new("key", key_json(9, true, true))))
            .unwrap(); // Am = 8A
        registry
            .add(Arc::new(FakeAnalyzer::new("waveform", "{}".into())))
            .unwrap();
        registry
            .add(Arc::new(FakeAnalyzer::new("structure", "{}".into())))
            .unwrap();
        let lib = Library::open(path, OpenMode::ReadWrite, core_migrations()).unwrap();
        let service = AnalysisService::new(Arc::clone(&registry), None, workers);
        Self {
            bpm,
            registry,
            lib,
            service,
            events: Arc::default(),
            event_threads: Arc::default(),
            errors: Arc::default(),
            counter: 0,
        }
    }

    fn add_track(&mut self, path: Option<&str>) -> i64 {
        self.counter += 1;
        let path = match path {
            Some(p) => p.to_owned(),
            None => format!("/music/t{}.wav", self.counter),
        };
        let record = TrackRecord {
            path,
            ..TrackRecord::default()
        };
        self.lib.tracks().insert(&record).unwrap()
    }

    /// Clip length encodes the track id so analysers can report run order.
    fn options(&self, max_in_flight: usize) -> PrepOptions {
        let events = Arc::clone(&self.events);
        let threads = Arc::clone(&self.event_threads);
        let errors = Arc::clone(&self.errors);
        PrepOptions {
            max_in_flight,
            source_factory: Some(Box::new(|t: &TrackRecord| -> Box<dyn ClipSource> {
                let clip = AudioClip::new(vec![0.0; (t.id * 1000) as usize], 44_100.0);
                Box::new(MemoryClipSource::new(clip, t.path.clone()))
            })),
            observer: Some(Box::new(move |e: &PrepEntry| {
                events.lock().unwrap().push(e.clone());
                threads.lock().unwrap().push(thread::current().id());
            })),
            on_error: Some(Box::new(move |m: &str| {
                errors.lock().unwrap().push(m.to_owned())
            })),
            ..PrepOptions::default()
        }
    }

    fn replace(&self, analyzer: Arc<dyn Analyzer>) {
        self.registry.remove(analyzer.id());
        self.registry.add(analyzer).unwrap();
    }

    fn status_of(&self, id: i64) -> PrepStatus {
        self.lib.prep_queue().find(id).unwrap().unwrap().status
    }
}

fn wait_started(gate: &GateAnalyzer) {
    om_testing::assert_eventually!(Duration::from_secs(10), || gate
        .started
        .load(Ordering::SeqCst));
}

#[test]
fn prep_queue_dao_enqueue_keeps_order_and_move_rewrites_positions() {
    let mut f = Fixture::new(2);
    let (a, b, c) = (f.add_track(None), f.add_track(None), f.add_track(None));
    let dao = f.lib.prep_queue();
    for id in [a, b, c] {
        dao.enqueue(id).unwrap();
    }
    dao.move_to(c, 0).unwrap();
    let all = dao.all().unwrap();
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].track_id, c);
    assert_eq!(all[1].track_id, a);
    assert_eq!(all[2].track_id, b);
    assert_eq!(all[2].position, 2);
    dao.move_to(a, 99).unwrap(); // clamps to the end
    assert_eq!(dao.all().unwrap().last().unwrap().track_id, a);
    assert!(dao.move_to(9999, 0).is_err());
}

#[test]
fn prep_queue_dao_enqueues_only_unanalysed_tracks_that_were_never_queued() {
    let mut f = Fixture::new(2);
    let (a, b, c, analysed, failed) = (
        f.add_track(None),
        f.add_track(None),
        f.add_track(None),
        f.add_track(None),
        f.add_track(None),
    );
    f.lib
        .tracks()
        .set_analysis(analysed, 128.0, "", "")
        .unwrap();
    let dao = f.lib.prep_queue();
    dao.enqueue(failed).unwrap();
    dao.set_status(failed, PrepStatus::Failed, "boom").unwrap();

    assert_eq!(dao.enqueue_unanalysed().unwrap(), 3);
    let all = dao.all().unwrap();
    let ids: Vec<i64> = all.iter().map(|e| e.track_id).collect();
    assert_eq!(
        ids,
        [failed, a, b, c],
        "appended in track order, behind the old row"
    );
    assert_eq!(
        dao.find(failed).unwrap().unwrap().status,
        PrepStatus::Failed
    );
    assert_eq!(dao.find(a).unwrap().unwrap().status, PrepStatus::Queued);
    assert!(dao.find(analysed).unwrap().is_none());

    assert_eq!(
        dao.enqueue_unanalysed().unwrap(),
        0,
        "a second pass adds nothing"
    );
}

#[test]
fn prep_queue_dao_prioritise_jumps_ahead_and_latest_wins() {
    let mut f = Fixture::new(2);
    let (a, b, c) = (f.add_track(None), f.add_track(None), f.add_track(None));
    let dao = f.lib.prep_queue();
    for id in [a, b, c] {
        dao.enqueue(id).unwrap();
    }
    dao.prioritise(b).unwrap();
    assert_eq!(dao.next_queued().unwrap().unwrap().track_id, b);
    dao.prioritise(c).unwrap();
    assert_eq!(dao.next_queued().unwrap().unwrap().track_id, c);
    assert_eq!(dao.all().unwrap()[0].track_id, a); // user order is untouched
}

#[test]
fn prep_queue_dao_requeue_resets_finished_but_not_running() {
    let mut f = Fixture::new(2);
    let (a, b) = (f.add_track(None), f.add_track(None));
    let dao = f.lib.prep_queue();
    dao.enqueue(a).unwrap();
    dao.enqueue(b).unwrap();
    dao.set_status(a, PrepStatus::Failed, "boom").unwrap();
    dao.set_status(b, PrepStatus::Running, "").unwrap();
    dao.enqueue(a).unwrap();
    dao.enqueue(b).unwrap();
    let a_row = dao.find(a).unwrap().unwrap();
    assert_eq!(a_row.status, PrepStatus::Queued);
    assert!(a_row.error.is_empty());
    assert_eq!(a_row.position, 2);
    assert_eq!(dao.find(b).unwrap().unwrap().status, PrepStatus::Running);
}

#[test]
fn prep_queue_dao_missing_track_fails_and_deleted_track_cascades() {
    let mut f = Fixture::new(2);
    let a = f.add_track(None);
    let dao = f.lib.prep_queue();
    assert!(dao.enqueue(12345).is_err());
    dao.enqueue(a).unwrap();
    f.lib.tracks().remove(a).unwrap();
    assert!(dao.all().unwrap().is_empty());
}

#[test]
fn prep_queue_dao_reset_running_and_clear_finished() {
    let mut f = Fixture::new(2);
    let (a, b, c) = (f.add_track(None), f.add_track(None), f.add_track(None));
    let dao = f.lib.prep_queue();
    for id in [a, b, c] {
        dao.enqueue(id).unwrap();
    }
    dao.set_status(a, PrepStatus::Running, "").unwrap();
    dao.set_status(b, PrepStatus::Done, "").unwrap();
    dao.reset_running().unwrap();
    assert_eq!(dao.find(a).unwrap().unwrap().status, PrepStatus::Queued);
    dao.clear_finished().unwrap();
    assert!(dao.find(b).unwrap().is_none());
    assert!(dao.find(c).unwrap().is_some());
}

#[test]
fn prep_queue_dao_failed_reads_are_reported_not_empty() {
    let f = Fixture::new(2);
    f.lib.database().exec("DROP TABLE prep_queue;").unwrap();
    let dao = f.lib.prep_queue();
    assert!(dao.all().is_err());
    assert!(dao.next_queued().is_err());
    assert!(dao.find(1).is_err());
}

#[test]
fn prep_queue_processes_all_tracks_and_writes_back_bpm_and_key() {
    let mut f = Fixture::new(2);
    let (a, b) = (f.add_track(None), f.add_track(None));
    let mut q = PrepQueue::new(&f.lib, &f.service, f.options(2));
    q.enqueue(a, false).unwrap();
    q.enqueue(b, false).unwrap();
    assert!(q.pump_until_idle(WAIT));

    for id in [a, b] {
        let t = f.lib.tracks().find_by_id(id).unwrap().unwrap();
        assert_eq!(t.bpm.unwrap_or(0.0), 128.0);
        assert_eq!(t.key, "Am");
        assert_eq!(t.camelot, "8A");
        assert_eq!(f.status_of(id), PrepStatus::Done);
    }
    for e in q.list().unwrap() {
        assert_eq!(e.progress, 1.0);
    }
    assert!(f.errors.lock().unwrap().is_empty());
}

#[test]
fn prep_queue_observer_fires_on_the_owner_thread_with_status_changes() {
    let mut f = Fixture::new(2);
    let a = f.add_track(None);
    let mut q = PrepQueue::new(&f.lib, &f.service, f.options(2));
    q.enqueue(a, false).unwrap();
    assert!(q.pump_until_idle(WAIT));

    let events = f.events.lock().unwrap();
    assert!(events.len() >= 2);
    assert_eq!(events.first().unwrap().status, PrepStatus::Queued);
    assert_eq!(events.last().unwrap().status, PrepStatus::Done);
    let me = thread::current().id();
    assert!(f.event_threads.lock().unwrap().iter().all(|id| *id == me));
}

#[test]
fn prep_queue_urgent_track_runs_before_earlier_queued_ones() {
    let mut f = Fixture::new(1);
    let (a, b, c, d) = (
        f.add_track(None),
        f.add_track(None),
        f.add_track(None),
        f.add_track(None),
    );
    let mut q = PrepQueue::new(&f.lib, &f.service, f.options(1));
    q.enqueue(a, false).unwrap();
    q.enqueue(b, false).unwrap();
    q.enqueue(c, false).unwrap();
    q.pump(); // a is in flight
    q.enqueue(d, true).unwrap(); // jumps ahead of b and c
    assert!(q.pump_until_idle(WAIT));
    assert_eq!(*f.bpm.order.lock().unwrap(), vec![a, d, b, c]);
}

#[test]
fn prep_queue_move_changes_processing_order_of_waiting_items() {
    let mut f = Fixture::new(1);
    let (a, b, c) = (f.add_track(None), f.add_track(None), f.add_track(None));
    let mut q = PrepQueue::new(&f.lib, &f.service, f.options(1));
    q.enqueue(a, false).unwrap();
    q.enqueue(b, false).unwrap();
    q.enqueue(c, false).unwrap();
    q.pump();
    q.move_to(c, 0).unwrap();
    assert!(q.pump_until_idle(WAIT));
    assert_eq!(*f.bpm.order.lock().unwrap(), vec![a, c, b]);
}

#[test]
fn prep_queue_never_submits_more_than_max_in_flight() {
    let mut f = Fixture::new(4);
    f.bpm.delay_ms.store(20, Ordering::SeqCst);
    let ids: Vec<i64> = (0..6).map(|_| f.add_track(None)).collect();
    let mut q = PrepQueue::new(&f.lib, &f.service, f.options(2));
    for id in ids {
        q.enqueue(id, false).unwrap();
    }
    q.pump();
    assert!(f.service.queued_count() <= 2);
    assert!(q.pump_until_idle(WAIT));
    assert!(f.bpm.peak.load(Ordering::SeqCst) <= 2);
    assert!(f.bpm.peak.load(Ordering::SeqCst) >= 1);
    assert_eq!(f.bpm.order.lock().unwrap().len(), 6);
}

#[test]
fn prep_queue_cancel_queued_item_skips_it() {
    let mut f = Fixture::new(1);
    let (a, b) = (f.add_track(None), f.add_track(None));
    let mut q = PrepQueue::new(&f.lib, &f.service, f.options(1));
    q.enqueue(a, false).unwrap();
    q.enqueue(b, false).unwrap();
    q.pump();
    q.cancel(b);
    assert!(q.pump_until_idle(WAIT));
    assert_eq!(f.status_of(b), PrepStatus::Cancelled);
    assert!(f.lib.tracks().find_by_id(b).unwrap().unwrap().bpm.is_none());
    assert_eq!(f.bpm.order.lock().unwrap().len(), 1);
}

#[test]
fn prep_queue_cancel_running_job_ends_cancelled_and_writes_nothing() {
    let mut f = Fixture::new(1);
    let gate = Arc::new(GateAnalyzer::default());
    f.replace(gate.clone());
    let a = f.add_track(None);
    let mut q = PrepQueue::new(&f.lib, &f.service, f.options(1));
    q.enqueue(a, false).unwrap();
    q.pump();
    wait_started(&gate);

    q.cancel(a);
    assert!(q.pump_until_idle(WAIT));
    assert_eq!(f.status_of(a), PrepStatus::Cancelled);
    assert!(f.lib.tracks().find_by_id(a).unwrap().unwrap().bpm.is_none());
}

#[test]
fn prep_queue_cancel_all_stops_everything() {
    let mut f = Fixture::new(1);
    f.replace(Arc::new(GateAnalyzer::default()));
    let (a, b) = (f.add_track(None), f.add_track(None));
    let mut q = PrepQueue::new(&f.lib, &f.service, f.options(1));
    q.enqueue(a, false).unwrap();
    q.enqueue(b, false).unwrap();
    q.pump();
    q.cancel_all();
    assert!(q.pump_until_idle(WAIT));
    assert_eq!(f.status_of(a), PrepStatus::Cancelled);
    assert_eq!(f.status_of(b), PrepStatus::Cancelled);
}

#[test]
fn prep_queue_analyser_failure_marks_item_failed_and_queue_continues() {
    let mut f = Fixture::new(1);
    f.replace(Arc::new(NullAnalyzer));
    let a = f.add_track(None);
    let mut q = PrepQueue::new(&f.lib, &f.service, f.options(1));
    q.enqueue(a, false).unwrap();
    assert!(q.pump_until_idle(WAIT));
    assert_eq!(f.status_of(a), PrepStatus::Failed);
    assert!(!f
        .lib
        .prep_queue()
        .find(a)
        .unwrap()
        .unwrap()
        .error
        .is_empty());
}

#[test]
fn prep_queue_unreadable_file_fails_with_default_source_factory() {
    let mut f = Fixture::new(1);
    let a = f.add_track(Some("/nonexistent/none.wav"));
    let mut options = f.options(1);
    options.source_factory = None;
    let mut q = PrepQueue::new(&f.lib, &f.service, options);
    q.enqueue(a, false).unwrap();
    assert!(q.pump_until_idle(WAIT));
    assert_eq!(f.status_of(a), PrepStatus::Failed);
}

#[test]
fn prep_queue_atonal_key_and_unusable_bpm_leave_stored_values_alone() {
    let mut f = Fixture::new(1);
    f.replace(Arc::new(FakeAnalyzer::new("bpm", "{}".into())));
    f.replace(Arc::new(FakeAnalyzer::new(
        "key",
        key_json(0, false, false),
    )));
    let a = f.add_track(None);
    let mut t = f.lib.tracks().find_by_id(a).unwrap().unwrap();
    t.key = "Dm".into();
    t.camelot = "7A".into();
    f.lib.tracks().update(&t).unwrap();

    let mut q = PrepQueue::new(&f.lib, &f.service, f.options(1));
    q.enqueue(a, false).unwrap();
    assert!(q.pump_until_idle(WAIT));
    let after = f.lib.tracks().find_by_id(a).unwrap().unwrap();
    assert_eq!(after.key, "Dm");
    assert!(after.bpm.is_none());
    assert_eq!(f.status_of(a), PrepStatus::Done);
}

#[test]
fn prep_queue_new_results_overwrite_old_analysis() {
    let mut f = Fixture::new(1);
    let a = f.add_track(None);
    f.lib.tracks().set_analysis(a, 90.0, "C", "8B").unwrap();
    let mut q = PrepQueue::new(&f.lib, &f.service, f.options(1));
    q.enqueue(a, false).unwrap();
    assert!(q.pump_until_idle(WAIT));
    let t = f.lib.tracks().find_by_id(a).unwrap().unwrap();
    assert_eq!(t.bpm.unwrap_or(0.0), 128.0);
    assert_eq!(t.camelot, "8A");
}

#[test]
fn prep_queue_track_removed_while_queued_does_not_stall_the_queue() {
    let mut f = Fixture::new(1);
    let (a, b) = (f.add_track(None), f.add_track(None));
    let mut q = PrepQueue::new(&f.lib, &f.service, f.options(1));
    q.enqueue(a, false).unwrap();
    q.enqueue(b, false).unwrap();
    f.lib.tracks().remove(a).unwrap(); // cascades the queue row away
    assert!(q.pump_until_idle(WAIT));
    assert_eq!(f.status_of(b), PrepStatus::Done);
}

#[test]
fn prep_queue_persists_across_restart_and_running_resets() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("lib.db").to_str().unwrap().to_owned();
    let (a, b);
    {
        let mut f = Fixture::open(1, &path);
        a = f.add_track(None);
        b = f.add_track(None);
        let dao = f.lib.prep_queue();
        dao.enqueue(a).unwrap();
        dao.enqueue(b).unwrap();
        dao.set_status(a, PrepStatus::Running, "").unwrap(); // as if the app died mid-job
    }
    let f = Fixture::open(1, &path);
    let mut q = PrepQueue::new(&f.lib, &f.service, f.options(1));
    assert_eq!(f.status_of(a), PrepStatus::Queued);
    assert!(q.pump_until_idle(WAIT));
    assert_eq!(f.status_of(a), PrepStatus::Done);
    assert_eq!(f.status_of(b), PrepStatus::Done);
}

#[test]
fn prep_queue_remove_drops_row_and_cancels_running_job() {
    let mut f = Fixture::new(1);
    let gate = Arc::new(GateAnalyzer::default());
    f.replace(gate.clone());
    let a = f.add_track(None);
    let mut q = PrepQueue::new(&f.lib, &f.service, f.options(1));
    q.enqueue(a, false).unwrap();
    q.pump();
    wait_started(&gate);
    q.remove(a).unwrap();
    assert!(q.pump_until_idle(WAIT));
    assert!(f.lib.prep_queue().find(a).unwrap().is_none());
}

#[test]
fn prep_queue_destroying_with_jobs_in_flight_is_safe() {
    let mut f = Fixture::new(2);
    f.replace(Arc::new(GateAnalyzer::default()));
    let (a, b) = (f.add_track(None), f.add_track(None));
    {
        let mut q = PrepQueue::new(&f.lib, &f.service, f.options(2));
        q.enqueue(a, false).unwrap();
        q.enqueue(b, false).unwrap();
        q.pump();
    }
    f.service.cancel_all(); // late listener callbacks must not touch the dead queue
}

#[test]
fn prep_queue_database_read_failures_are_reported_not_treated_as_idle() {
    let mut f = Fixture::new(1);
    let a = f.add_track(None);
    let mut q = PrepQueue::new(&f.lib, &f.service, f.options(1));
    f.lib.database().exec("DROP TABLE prep_queue;").unwrap();

    q.cancel(a);
    assert!(!f.errors.lock().unwrap().is_empty()); // cancel's lookup failed and said so
    f.errors.lock().unwrap().clear();

    assert!(q.pump()); // unreadable queue is not "idle"
    assert!(!f.errors.lock().unwrap().is_empty());
    assert!(!q.pump_until_idle(Duration::from_millis(50)));
}

#[test]
fn prep_queue_remove_then_reenqueue_while_running_runs_a_fresh_job() {
    let mut f = Fixture::new(2);
    let gate = Arc::new(GateAnalyzer::default());
    f.replace(gate.clone());
    let a = f.add_track(None);
    let mut q = PrepQueue::new(&f.lib, &f.service, f.options(2));
    q.enqueue(a, false).unwrap();
    q.pump();
    wait_started(&gate);

    q.remove(a).unwrap();
    q.enqueue(a, false).unwrap();
    f.replace(Arc::new(FakeAnalyzer::new("waveform", "{}".into()))); // later jobs finish normally
    q.pump();
    assert!(q.pump_until_idle(WAIT)); // old job's completion must not clobber the new row
    assert_eq!(f.status_of(a), PrepStatus::Done);
    assert_eq!(
        f.lib
            .tracks()
            .find_by_id(a)
            .unwrap()
            .unwrap()
            .bpm
            .unwrap_or(0.0),
        128.0
    );
}

#[test]
fn prep_queue_cancel_then_reenqueue_while_running_takes_effect() {
    let mut f = Fixture::new(2);
    let gate = Arc::new(GateAnalyzer::default());
    f.replace(gate.clone());
    let a = f.add_track(None);
    let mut q = PrepQueue::new(&f.lib, &f.service, f.options(2));
    q.enqueue(a, false).unwrap();
    q.pump();
    wait_started(&gate);

    q.cancel(a);
    assert_eq!(f.status_of(a), PrepStatus::Cancelled);
    q.enqueue(a, false).unwrap();
    assert_eq!(f.status_of(a), PrepStatus::Queued);
    f.replace(Arc::new(FakeAnalyzer::new("waveform", "{}".into())));
    assert!(q.pump_until_idle(WAIT));
    assert_eq!(f.status_of(a), PrepStatus::Done);
}

fn scan(lib: &Library, folder: &std::path::Path) -> (IngestSummary, bool) {
    let readers = MetadataReaderRegistry::with_defaults();
    let mut summary = IngestSummary::default();
    let ok = Ingestor::new(lib, &readers)
        .ingest_folder(folder, &mut summary, &|| false)
        .is_ok();
    (summary, ok)
}

#[test]
fn scan_enqueues_new_unanalysed_tracks_once() {
    let lib = Library::open(":memory:", OpenMode::ReadWrite, core_migrations()).unwrap();
    let tmp = TempDir::new().unwrap();
    for (i, name) in ["a.wav", "b.wav", "c.wav"].iter().enumerate() {
        write_wav(
            &tmp.path().join(name),
            1.0,
            &Tags::default(),
            220.0 + i as f32 * 110.0,
        );
    }

    let (first, ok) = scan(&lib, tmp.path());
    assert!(ok);
    assert_eq!(first.added, 3);
    assert_eq!(first.enqueued, 3);
    assert_eq!(lib.prep_queue().all().unwrap().len(), 3);

    let (second, ok) = scan(&lib, tmp.path());
    assert!(ok);
    assert_eq!(second.unchanged, 3);
    assert_eq!(second.enqueued, 0);
    assert_eq!(lib.prep_queue().all().unwrap().len(), 3);
}

#[test]
fn scan_does_not_requeue_a_track_that_already_has_a_result() {
    let lib = Library::open(":memory:", OpenMode::ReadWrite, core_migrations()).unwrap();
    let tmp = TempDir::new().unwrap();
    let file = write_wav(&tmp.path().join("a.wav"), 1.0, &Tags::default(), 220.0);
    assert_eq!(scan(&lib, tmp.path()).0.enqueued, 1);
    let id = lib
        .tracks()
        .find_by_path(&file.to_string_lossy())
        .unwrap()
        .unwrap()
        .id;
    lib.prep_queue()
        .set_status(id, PrepStatus::Failed, "undecodable")
        .unwrap();

    // Only the mtime moves: re-read, same content, so the failed entry is left alone.
    common::touch(&file, 1_000_000);
    let (again, _) = scan(&lib, tmp.path());
    assert_eq!(again.updated, 1);
    assert_eq!(again.enqueued, 0);
    assert_eq!(
        lib.prep_queue().find(id).unwrap().unwrap().status,
        PrepStatus::Failed
    );

    // The content changes: the old result no longer applies, so it is queued again.
    let _ = write_wav(&tmp.path().join("a.wav"), 2.0, &Tags::default(), 220.0);
    let (changed, _) = scan(&lib, tmp.path());
    assert_eq!(changed.updated, 1);
    assert_eq!(changed.enqueued, 1);
    assert_eq!(
        lib.prep_queue().find(id).unwrap().unwrap().status,
        PrepStatus::Queued
    );
}

#[test]
fn scan_of_missing_folder_enqueues_nothing() {
    let lib = Library::open(":memory:", OpenMode::ReadWrite, core_migrations()).unwrap();
    let tmp = TempDir::new().unwrap();
    let (summary, ok) = scan(&lib, &tmp.path().join("nope"));
    assert!(!ok);
    assert_eq!(summary.enqueued, 0);
    assert!(lib.prep_queue().all().unwrap().is_empty());
}
