//! Keeps watched folders in step with the library: the folder list lives in the database, and a
//! background thread rescans every folder on an interval or on demand. Plain std threads and polling (no filesystem
//! notifications).
//!
//! The scan thread opens its own `Library` connection (WAL and the busy timeout let it run next to
//! the caller's). Add, remove and list go through a second connection behind a mutex. Open the
//! database read-write (migrated) before any read-only open, and use a file-backed database:
//! `:memory:` is private to one connection.

use crate::database::OpenMode;
use crate::error::{LibraryError, Result};
use crate::ingest::{full_path, IngestSummary, Ingestor};
use crate::library::Library;
use crate::metadata::MetadataReaderRegistry;
use crate::migrations::{core_migrations, Migration};
use om_core::sync::LockExt;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Monotonic milliseconds since an arbitrary start. Injected in tests.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;
/// Called on the scan thread after each pass.
pub type ScanDoneCallback = Arc<dyn Fn(&IngestSummary) + Send + Sync>;
/// Called on the scan thread when a pass cannot run (for example, the database cannot be opened).
pub type ErrorCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Scheduling and callbacks for a [`WatchFolders`].
#[derive(Clone)]
pub struct WatchFolderOptions {
    /// Time between automatic rescans.
    pub interval_ms: i64,
    /// How often the thread re-checks the clock.
    pub poll_ms: i64,
    /// Defaults to a monotonic system clock.
    pub clock: Option<Clock>,
    pub on_scan_done: Option<ScanDoneCallback>,
    /// Run a pass as soon as the thread starts.
    pub scan_on_start: bool,
    /// Must include later subsystems' migrations.
    pub migrations: Vec<Migration>,
    pub on_error: Option<ErrorCallback>,
}

impl Default for WatchFolderOptions {
    fn default() -> Self {
        Self {
            interval_ms: 5 * 60 * 1000,
            poll_ms: 500,
            clock: None,
            on_scan_done: None,
            scan_on_start: true,
            migrations: core_migrations().to_vec(),
            on_error: None,
        }
    }
}

#[derive(Default)]
struct WakeState {
    triggered: bool,
    stopping: bool,
    passes: usize,
}

struct Shared {
    state: Mutex<WakeState>,
    wake: Condvar,
    done: Condvar,
    /// Set on drop; aborts a scan in progress between files.
    stop: AtomicBool,
    /// Time between automatic rescans; the scan thread reads it on every poll.
    interval_ms: AtomicI64,
}

/// Persisted watched-folder list plus a background rescan thread.
pub struct WatchFolders {
    control: Mutex<Library>,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl WatchFolders {
    /// Opens (and migrates) the database at `library_path` and starts the scan thread.
    /// Fails when the database cannot be opened.
    pub fn create(
        library_path: &str,
        readers: MetadataReaderRegistry,
        options: WatchFolderOptions,
    ) -> Result<Self> {
        let control = Library::open(library_path, OpenMode::ReadWrite, &options.migrations)?;
        let shared = Arc::new(Shared {
            state: Mutex::new(WakeState {
                triggered: options.scan_on_start,
                ..WakeState::default()
            }),
            wake: Condvar::new(),
            done: Condvar::new(),
            stop: AtomicBool::new(false),
            interval_ms: AtomicI64::new(options.interval_ms),
        });
        let worker = {
            let shared = Arc::clone(&shared);
            let path = library_path.to_owned();
            std::thread::spawn(move || run(&shared, &path, &readers, &options))
        };
        Ok(Self {
            control: Mutex::new(control),
            shared,
            worker: Some(worker),
        })
    }

    /// Adds a folder and asks for a scan. Fails if `folder` is not a directory. Adding a folder
    /// twice is a no-op success.
    pub fn add(&self, folder: &Path) -> Result<()> {
        if !folder.is_dir() {
            return Err(LibraryError::Ingest(format!(
                "not a folder: {}",
                folder.display()
            )));
        }
        let control = self.lock_control();
        control
            .watch_folders()
            .add(&full_path(folder).to_string_lossy())?;
        self.trigger_scan();
        Ok(())
    }

    pub fn remove(&self, folder: &Path) -> Result<()> {
        self.lock_control()
            .watch_folders()
            .remove(&full_path(folder).to_string_lossy())
    }

    /// Watched folders in insertion order.
    pub fn list(&self) -> Result<Vec<String>> {
        self.lock_control().watch_folders().list()
    }

    /// Changes the time between automatic rescans. Takes effect at the scan thread's next poll,
    /// measured from the last pass.
    pub fn set_interval_ms(&self, interval_ms: i64) {
        self.shared.interval_ms.store(interval_ms, Ordering::SeqCst);
        self.shared.wake.notify_all();
    }

    /// Requests a pass now and returns immediately.
    pub fn trigger_scan(&self) {
        self.lock_state().triggered = true;
        self.shared.wake.notify_all();
    }

    /// Passes finished so far.
    pub fn scan_count(&self) -> usize {
        self.lock_state().passes
    }

    /// Blocks until at least `n` passes have finished (true) or `timeout` passes (false).
    pub fn wait_for_scans(&self, n: usize, timeout: Duration) -> bool {
        let state = self.lock_state();
        let (state, _) = self
            .shared
            .done
            .wait_timeout_while(state, timeout, |s| s.passes < n)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.passes >= n
    }

    /// Wakes the scan thread to re-read its clock (tests call this after advancing a fake clock).
    pub fn poke(&self) {
        self.shared.wake.notify_all();
    }

    fn lock_state(&self) -> MutexGuard<'_, WakeState> {
        self.shared.state.lock_unpoisoned()
    }

    fn lock_control(&self) -> MutexGuard<'_, Library> {
        self.control.lock_unpoisoned()
    }
}

impl Drop for WatchFolders {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.lock_state().stopping = true;
        self.shared.wake.notify_all();
        if let Some(handle) = self.worker.take() {
            // A panicked scan thread has nothing left to clean up here.
            let _ = handle.join();
        }
    }
}

/// Monotonic milliseconds from the first call.
fn system_clock() -> i64 {
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX)
}

fn run(
    shared: &Shared,
    library_path: &str,
    readers: &MetadataReaderRegistry,
    options: &WatchFolderOptions,
) {
    let clock = options
        .clock
        .clone()
        .unwrap_or_else(|| Arc::new(system_clock));
    let poll = Duration::from_millis(u64::try_from(options.poll_ms).unwrap_or(0).max(1));
    // why: opened lazily on this thread so the connection is only ever used here; retried each
    // pass if it failed
    let mut scan_library: Option<Library> = None;
    let mut last_scan = clock();

    let mut state = shared.state.lock_unpoisoned();
    while !state.stopping {
        let now = clock();
        if !state.triggered && now - last_scan < shared.interval_ms.load(Ordering::SeqCst) {
            state = shared
                .wake
                .wait_timeout(state, poll)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
            continue;
        }
        state.triggered = false;
        last_scan = now;
        drop(state);

        if scan_library.is_none() {
            scan_library =
                open_scan_library(library_path, &options.migrations, options.on_error.as_ref());
        }
        // A panic in a reader or in the callbacks must not end the thread: the pass is counted as
        // failed, reported, and the next pass runs as usual.
        let scanned = panic::catch_unwind(AssertUnwindSafe(|| match &scan_library {
            Some(library) => scan_all(library, readers, &shared.stop),
            // surfaced to on_scan_done so callers can tell the pass did nothing
            None => IngestSummary {
                failed: 1,
                ..IngestSummary::default()
            },
        }));
        let summary = match scanned {
            Ok(summary) => summary,
            Err(payload) => {
                // the connection may be mid-transaction; reopen it on the next pass
                scan_library = None;
                report_error(
                    options.on_error.as_ref(),
                    &format!(
                        "watch folders: scan panicked: {}",
                        panic_message(payload.as_ref())
                    ),
                );
                IngestSummary {
                    failed: 1,
                    ..IngestSummary::default()
                }
            }
        };
        if let Some(callback) = &options.on_scan_done {
            let outcome = panic::catch_unwind(AssertUnwindSafe(|| callback(&summary)));
            if let Err(payload) = outcome {
                report_error(
                    options.on_error.as_ref(),
                    &format!(
                        "watch folders: callback panicked: {}",
                        panic_message(payload.as_ref())
                    ),
                );
            }
        }

        state = shared.state.lock_unpoisoned();
        state.passes += 1;
        shared.done.notify_all();
    }
}

/// Passes a message to the error callback. A callback that itself panics is ignored, so the scan
/// thread survives.
fn report_error(on_error: Option<&ErrorCallback>, message: &str) {
    if let Some(callback) = on_error {
        let _ = panic::catch_unwind(AssertUnwindSafe(|| callback(message)));
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_owned())
}

fn open_scan_library(
    library_path: &str,
    migrations: &[Migration],
    on_error: Option<&ErrorCallback>,
) -> Option<Library> {
    match Library::open(library_path, OpenMode::ReadWrite, migrations) {
        Ok(library) => Some(library),
        Err(e) => {
            if let Some(callback) = on_error {
                callback(&format!("watch folders: cannot open library: {e}"));
            }
            None
        }
    }
}

/// One pass over every watched folder. A folder that is missing or unreadable is skipped; an
/// unreadable folder list is not an empty list, so the pass is retried next time.
fn scan_all(
    library: &Library,
    readers: &MetadataReaderRegistry,
    stop: &AtomicBool,
) -> IngestSummary {
    let mut total = IngestSummary::default();
    let Ok(folders) = library.watch_folders().list() else {
        return total;
    };
    let ingestor = Ingestor::new(library, readers);
    let should_stop = || stop.load(Ordering::SeqCst);
    for folder in folders {
        let mut one = IngestSummary::default();
        // A missing folder fails here and is skipped; nothing is flagged missing for it.
        let _ = ingestor.ingest_folder(Path::new(&folder), &mut one, &should_stop);
        total += one;
    }
    total
}
