//! Composition of the library subsystem for the app: the read-write `Library`, the search engine,
//! the analysis service with its result cache, the prep queue and the watch folders (their own scan
//! thread and connection). Nothing here knows about widgets; the browser, the track loader and the
//! settings page talk to it.
//!
//! Start-up order: library (writer, migrates) -> result cache -> analysis service -> prep queue ->
//! watch folders. Shutdown is the reverse, which the field order gives.
//!
//! Threading: the owner thread (the one that built the services) calls every method except the
//! watch-folder scan, which runs on its own thread. Scan results and prep events reach the owner
//! through an outbox that [`LibraryServices::pump`] drains, so listeners and error callbacks always
//! run on the owner thread. Listeners get a channel
//! ([`LibraryServices::subscribe`]); dropping the receiver unsubscribes.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use om_analysis::{
    hash_file_content, make_engine_grid, AnalysisService, Analyzer, AnalyzerRegistry, BpmResult,
    FileClipSource, ResultCache, SqliteResultCache, WaveformData,
};
use om_engine::grid::BeatGrid;
use om_library::{
    core_migrations, Database, ImportReport, Library, LibraryError, LibraryImporterRegistry,
    MetadataReaderRegistry, OpenMode, PrepEntry, PrepOptions, PrepQueue, SearchEngine, TrackRecord,
    WatchFolderOptions, WatchFolders,
};

/// Library path that runs without a file, watch folders or a cache file.
pub const MEMORY_PATH: &str = ":memory:";

/// Environment variable that overrides the default library location.
pub const LIBRARY_DB_ENV: &str = "OPENMIXER_LIBRARY_DB";

/// Receives an error message on the owner thread.
pub type ErrorHandler = Box<dyn Fn(&str)>;

/// Changes the owner thread reports to subscribers.
#[derive(Debug, Clone, PartialEq)]
pub enum LibraryEvent {
    /// Tracks or playlists changed (an import, a scan pass, a watch-folder edit).
    LibraryChanged,
    /// The prep queue changed a track's status or progress.
    PrepEntryChanged(PrepEntry),
}

/// What the result cache already knows about a track. `analysed` once waveform and tempo are both
/// cached.
#[derive(Clone, Default)]
pub struct TrackAnalysis {
    pub waveform: Option<Arc<WaveformData>>,
    /// `None` when no pulse was found.
    pub grid: Option<Arc<dyn BeatGrid>>,
    pub analysed: bool,
}

/// Start-up configuration.
pub struct Options {
    /// `:memory:` runs without watch folders and without a cache file.
    pub db_path: String,
    /// Empty: `<db_path>.analysis`, or `:memory:` for an in-memory library.
    pub cache_path: String,
    pub watch_folders: bool,
    pub scan_on_start: bool,
    pub scan_interval_ms: i64,
    pub analysis_workers: usize,
    /// `None`: the built-in analysers (bpm, key, waveform, structure).
    pub analyzers: Option<Arc<AnalyzerRegistry>>,
    /// Always called on the owner thread, from `pump` or the call that failed.
    pub on_error: Option<ErrorHandler>,
}

impl Options {
    /// Options for `db_path` with every default: watch folders on, scan on start, 5-minute rescans.
    pub fn for_path(db_path: impl Into<String>) -> Self {
        Self {
            db_path: db_path.into(),
            cache_path: String::new(),
            watch_folders: true,
            scan_on_start: true,
            scan_interval_ms: 5 * 60 * 1000,
            analysis_workers: 2,
            analyzers: None,
            on_error: None,
        }
    }
}

/// The default library file: `OPENMIXER_LIBRARY_DB`, else `<user config>/OpenMixer/library.db`, the
/// folder the preferences use.
pub fn default_options() -> Options {
    let path = library_file_from(
        std::env::var_os(LIBRARY_DB_ENV).as_deref(),
        crate::preferences::data_dir(),
        dirs::data_dir().map(|d| d.join("OpenMixer")),
    );
    Options::for_path(path.to_string_lossy().into_owned())
}

/// Resolves the library file. `folder` is the current `<config>/OpenMixer`; `legacy_folder` is where
/// the first Rust build put it (`<data>/OpenMixer`). The legacy file is used only while no file
/// exists at `folder`, so an existing library is never left behind.
pub fn library_file_from(
    env: Option<&std::ffi::OsStr>,
    folder: PathBuf,
    legacy_folder: Option<PathBuf>,
) -> PathBuf {
    if let Some(custom) = env.filter(|v| !v.is_empty()) {
        return PathBuf::from(custom);
    }
    let current = folder.join("library.db");
    if !current.exists() {
        if let Some(legacy) = legacy_folder
            .map(|f| f.join("library.db"))
            .filter(|p| p.exists())
        {
            return legacy;
        }
    }
    current
}

/// Events waiting for the owner thread. Filled by the prep queue's observer and error handler,
/// which run on the owner thread too (inside `pump` or the queue's own calls).
#[derive(Default)]
struct Outbox {
    prep: Vec<PrepEntry>,
    errors: Vec<String>,
}

pub struct LibraryServices {
    on_error: Option<ErrorHandler>,
    outbox: Rc<RefCell<Outbox>>,
    /// Errors raised on the scan thread; moved to the outbox by `pump`.
    scan_errors: Arc<Mutex<Vec<String>>>,
    finished_scans: Arc<AtomicUsize>,
    seen_scans: usize,
    listeners: Vec<Sender<LibraryEvent>>,
    importers: LibraryImporterRegistry,
    analyzers: Arc<AnalyzerRegistry>,
    cache: Option<Arc<dyn ResultCache>>,
    library: Arc<Library>,
    prep: PrepQueue<Arc<Library>, Arc<AnalysisService>>,
    watch: Option<WatchFolders>,
    service: Arc<AnalysisService>,
}

impl LibraryServices {
    /// Opens (and migrates) the library, then starts analysis and, for a file library, the watch
    /// folders. Fails with the reason when the library cannot be opened; a cache or watch-folder
    /// failure is reported through `on_error` and the services still start.
    pub fn create(mut options: Options) -> Result<Self, LibraryError> {
        let in_memory = options.db_path == MEMORY_PATH;
        if !in_memory {
            if let Some(parent) = Path::new(&options.db_path).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
        }
        let library = Library::open(&options.db_path, OpenMode::ReadWrite, core_migrations())?;
        // Shared only by handles on the owner thread; the connection is not Sync, and Rc would make
        // the services non-Send for no gain.
        #[allow(clippy::arc_with_non_send_sync)]
        let library = Arc::new(library);
        let on_error = options.on_error.take();
        let outbox = Rc::new(RefCell::new(Outbox::default()));

        let analyzers = options
            .analyzers
            .take()
            .unwrap_or_else(|| Arc::new(AnalyzerRegistry::builtin()));
        let cache_path = if !options.cache_path.is_empty() {
            options.cache_path.clone()
        } else if in_memory {
            MEMORY_PATH.to_owned()
        } else {
            format!("{}.analysis", options.db_path)
        };
        // A cache that cannot open is not fatal: tracks still load, they just never show cached
        // waveforms.
        let mut early_errors = Vec::new();
        let cache: Option<Arc<dyn ResultCache>> = match SqliteResultCache::open(&cache_path) {
            Ok(cache) => Some(Arc::new(cache)),
            Err(error) => {
                early_errors.push(format!("analysis cache unavailable: {error}"));
                None
            }
        };
        let service = Arc::new(AnalysisService::new(
            Arc::clone(&analyzers),
            cache.clone(),
            options.analysis_workers.max(1),
        ));

        let observer_outbox = Rc::clone(&outbox);
        let error_outbox = Rc::clone(&outbox);
        let prep = PrepQueue::new(
            Arc::clone(&library),
            Arc::clone(&service),
            PrepOptions {
                source_factory: Some(Box::new(|track: &TrackRecord| {
                    Box::new(FileClipSource::new(PathBuf::from(&track.path)))
                })),
                observer: Some(Box::new(move |entry: &PrepEntry| {
                    observer_outbox.borrow_mut().prep.push(entry.clone());
                })),
                on_error: Some(Box::new(move |message: &str| {
                    error_outbox.borrow_mut().errors.push(message.to_owned());
                })),
                ..PrepOptions::default()
            },
        );

        let finished_scans = Arc::new(AtomicUsize::new(0));
        let scan_errors = Arc::new(Mutex::new(Vec::new()));
        let mut watch = None;
        if options.watch_folders && !in_memory {
            let scans = Arc::clone(&finished_scans);
            let errors = Arc::clone(&scan_errors);
            let watch_options = WatchFolderOptions {
                interval_ms: options.scan_interval_ms,
                scan_on_start: options.scan_on_start,
                on_scan_done: Some(Arc::new(move |_| {
                    scans.fetch_add(1, Ordering::SeqCst);
                })),
                on_error: Some(Arc::new(move |message: &str| {
                    if let Ok(mut list) = errors.lock() {
                        list.push(message.to_owned());
                    }
                })),
                ..WatchFolderOptions::default()
            };
            match WatchFolders::create(
                &options.db_path,
                MetadataReaderRegistry::with_defaults(),
                watch_options,
            ) {
                Ok(folders) => watch = Some(folders),
                Err(error) => early_errors.push(format!("watch folders unavailable: {error}")),
            }
        }

        let services = Self {
            on_error,
            outbox,
            scan_errors,
            finished_scans,
            seen_scans: 0,
            listeners: Vec::new(),
            importers: LibraryImporterRegistry::with_defaults(),
            analyzers,
            cache,
            library,
            prep,
            watch,
            service,
        };
        for message in early_errors {
            services.report(&message);
        }
        Ok(services)
    }

    /// A channel that receives every later event. Drop it to unsubscribe.
    pub fn subscribe(&mut self) -> Receiver<LibraryEvent> {
        let (tx, rx) = channel();
        self.listeners.push(tx);
        rx
    }

    /// Tells subscribers that tracks or playlists changed.
    pub fn notify_library_changed(&mut self) {
        self.broadcast(&LibraryEvent::LibraryChanged);
    }

    pub fn library(&self) -> &Library {
        &self.library
    }

    /// A search engine over this library, owning its own handle to the database.
    pub fn search(&self) -> SearchEngine<Arc<Database>> {
        SearchEngine::new(self.library.shared_database())
    }

    /// Shared handle to the library, for views that read rows after the call that made them.
    pub fn library_handle(&self) -> Arc<Library> {
        Arc::clone(&self.library)
    }

    pub fn prep_queue(&self) -> &PrepQueue<Arc<Library>, Arc<AnalysisService>> {
        &self.prep
    }

    pub fn prep_queue_mut(&mut self) -> &mut PrepQueue<Arc<Library>, Arc<AnalysisService>> {
        &mut self.prep
    }

    /// `None` for an in-memory library.
    pub fn watch_folders(&self) -> Option<&WatchFolders> {
        self.watch.as_ref()
    }

    pub fn importers(&self) -> &LibraryImporterRegistry {
        &self.importers
    }

    pub fn analysis_service(&self) -> &AnalysisService {
        &self.service
    }

    /// What the cache holds for `track`, decoded for the deck: waveform, grid and whether both are
    /// present.
    pub fn analysis_for(&self, track: &TrackRecord) -> TrackAnalysis {
        let hash = if track.content_hash.is_empty() {
            hash_file_content(Path::new(&track.path)).unwrap_or_default()
        } else {
            track.content_hash.clone()
        };
        let wave = self.cached(&hash, "waveform");
        let bpm = self.cached(&hash, "bpm");
        let mut out = TrackAnalysis {
            analysed: wave.is_some() && bpm.is_some(),
            ..TrackAnalysis::default()
        };
        if let Some(data) = wave.and_then(|json| WaveformData::from_json(&json)) {
            if data.is_valid() {
                out.waveform = Some(Arc::new(data));
            }
        }
        if let Some(result) = bpm.and_then(|json| BpmResult::from_json(&json)) {
            out.grid = make_engine_grid(&result.grid);
        }
        out
    }

    /// Imports an iTunes or Rekordbox XML file. Subscribers hear about it on success.
    pub fn import_file(&mut self, xml: &Path) -> Result<ImportReport, LibraryError> {
        let mut report = ImportReport::default();
        om_library::import_library_file(&self.library, &self.importers, xml, &mut report)?;
        self.notify_library_changed();
        Ok(report)
    }

    /// One tick: applies finished analysis, finished scans and queued errors, and delivers the
    /// events they produced. Returns whether the prep queue did anything.
    pub fn pump(&mut self) -> bool {
        let worked = self.prep.pump();
        let scans = self.finished_scans.load(Ordering::SeqCst);
        if scans != self.seen_scans {
            self.seen_scans = scans;
            self.notify_library_changed();
        }
        let scan_errors: Vec<String> = match self.scan_errors.lock() {
            Ok(mut list) => list.drain(..).collect(),
            Err(_) => Vec::new(),
        };
        for message in scan_errors {
            self.report(&message);
        }
        let (prep_events, errors) = {
            let mut outbox = self.outbox.borrow_mut();
            (
                std::mem::take(&mut outbox.prep),
                std::mem::take(&mut outbox.errors),
            )
        };
        for message in errors {
            self.report(&message);
        }
        for entry in prep_events {
            self.broadcast(&LibraryEvent::PrepEntryChanged(entry));
        }
        worked
    }

    fn cached(&self, hash: &str, id: &str) -> Option<String> {
        let cache = self.cache.as_ref()?;
        if hash.is_empty() {
            return None;
        }
        let analyzer: Arc<dyn Analyzer> = self.analyzers.find(id)?;
        cache.find(hash, id, analyzer.version())
    }

    fn broadcast(&mut self, event: &LibraryEvent) {
        self.listeners.retain(|tx| tx.send(event.clone()).is_ok());
    }

    fn report(&self, message: &str) {
        match &self.on_error {
            Some(handler) => handler(message),
            None => eprintln!("library: {message}"),
        }
    }
}

impl Drop for LibraryServices {
    fn drop(&mut self) {
        // Errors still queued when the services die are dropped, not delivered.
        self.outbox.borrow_mut().errors.clear();
        self.on_error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn library_env_override_wins_over_both_folders() {
        let dir = tempfile::tempdir().expect("dir");
        let path = library_file_from(
            Some(OsStr::new("/custom/lib.db")),
            dir.path().join("config").join("OpenMixer"),
            Some(dir.path().join("data").join("OpenMixer")),
        );
        assert_eq!(path, PathBuf::from("/custom/lib.db"));
        let empty = library_file_from(
            Some(OsStr::new("")),
            dir.path().join("config").join("OpenMixer"),
            None,
        );
        assert!(
            empty.ends_with("OpenMixer/library.db") || empty.ends_with("OpenMixer\\library.db")
        );
    }

    #[test]
    fn library_lives_beside_the_preferences_by_default() {
        let config = tempfile::tempdir().expect("config");
        let folder = config.path().join("OpenMixer");
        let path = library_file_from(None, folder.clone(), None);
        assert_eq!(path, folder.join("library.db"));
    }

    #[test]
    fn an_existing_library_at_the_old_data_location_is_used_when_the_new_one_is_missing() {
        let root = tempfile::tempdir().expect("root");
        let folder = root.path().join("config").join("OpenMixer");
        let legacy = root.path().join("data").join("OpenMixer");
        std::fs::create_dir_all(&legacy).expect("legacy dir");
        std::fs::write(legacy.join("library.db"), b"old").expect("old library");

        let picked = library_file_from(None, folder.clone(), Some(legacy.clone()));
        assert_eq!(picked, legacy.join("library.db"), "no data is left behind");

        std::fs::create_dir_all(&folder).expect("new dir");
        std::fs::write(folder.join("library.db"), b"new").expect("new library");
        let picked = library_file_from(None, folder.clone(), Some(legacy));
        assert_eq!(
            picked,
            folder.join("library.db"),
            "the new location wins once it exists"
        );
    }
}
