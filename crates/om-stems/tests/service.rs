//! Stem service with the mock separator: the end-to-end job (cache and library row), cancel leaves
//! no partial directory, a second submit is a cache hit, the licence gate refuses without network
//! I/O, a panicking separator fails only its job, and the cap and forget paths.
//! No audio is played; the fixture WAVs are written by these tests.

use om_analysis::AnalysisContext;
use om_core::jobs::{JobHandle, JobStatus};
use om_library::{core_migrations, Library, OpenMode, TrackRecord};
use om_stems::{
    Downloader, MockSeparator, ModelGate, ModelManifest, StemError, StemRequest, StemSeparator,
    StemService, StemServiceConfig, StemSink, StereoSource, WeightError, WeightManager,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

const WAIT: Duration = Duration::from_secs(60);

/// Writes a 16-bit stereo WAV of `frames` frames (a 440 Hz tone on the left, quieter on the right).
fn write_wav(path: &Path, rate: u32, frames: usize) {
    let data = frames * 4;
    let mut out = Vec::with_capacity(44 + data);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data as u32).to_le_bytes());
    for i in 0..frames {
        let phase = i as f32 * 440.0 * std::f32::consts::TAU / rate as f32;
        let l = (phase.sin() * 0.5 * 32767.0) as i16;
        let r = (phase.sin() * 0.25 * 32767.0) as i16;
        out.extend_from_slice(&l.to_le_bytes());
        out.extend_from_slice(&r.to_le_bytes());
    }
    fs::write(path, out).expect("write wav");
}

struct Fixture {
    dir: TempDir,
    library: String,
    tracks: Vec<(i64, PathBuf, String)>,
}

/// A library with `n` tracks, each a real 0.2 s WAV at 44.1 kHz with a distinct content hash.
fn fixture(n: usize) -> Fixture {
    let dir = TempDir::new().expect("tempdir");
    let library = dir.path().join("library.db").to_str().unwrap().to_owned();
    let lib = Library::open(&library, OpenMode::ReadWrite, core_migrations()).expect("library");
    let mut tracks = Vec::new();
    for i in 0..n {
        let path = dir.path().join(format!("track{i}.wav"));
        write_wav(&path, 44_100, 8820);
        let hash = format!("{:064x}", i + 1);
        let id = lib
            .tracks()
            .insert(&TrackRecord {
                path: path.to_string_lossy().into_owned(),
                content_hash: hash.clone(),
                ..Default::default()
            })
            .expect("insert track");
        tracks.push((id, path, hash));
    }
    Fixture {
        dir,
        library,
        tracks,
    }
}

fn service(
    fx: &Fixture,
    separator: Arc<dyn StemSeparator>,
    cap: Option<u64>,
    gate: Option<ModelGate>,
) -> StemService {
    StemService::new(StemServiceConfig {
        separator,
        cache_root: fx.dir.path().join("stems"),
        library_path: fx.library.clone(),
        max_cache_bytes: cap,
        gate,
    })
    .expect("service")
}

fn request(fx: &Fixture, i: usize) -> StemRequest {
    let (track_id, path, hash) = &fx.tracks[i];
    StemRequest {
        track_id: *track_id,
        path: path.clone(),
        content_hash: hash.clone(),
    }
}

fn finish(handle: &JobHandle) -> JobStatus {
    assert!(handle.wait(WAIT), "job did not finish");
    handle.status()
}

fn dir_names(root: &Path) -> Vec<String> {
    fs::read_dir(root)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Counts calls and delegates to the mock.
struct Counting {
    inner: MockSeparator,
    calls: Arc<AtomicUsize>,
}

impl StemSeparator for Counting {
    fn id(&self) -> &str {
        self.inner.id()
    }
    fn version(&self) -> i32 {
        self.inner.version()
    }
    fn native_rate(&self) -> f64 {
        self.inner.native_rate()
    }
    fn separate_to(
        &self,
        input: &mut dyn StereoSource,
        sink: &mut dyn StemSink,
        ctx: &dyn AnalysisContext,
    ) -> Result<(), StemError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.separate_to(input, sink, ctx)
    }
}

/// Writes one chunk, reports it, then waits for the job to be cancelled before returning.
struct BlockUntilCancelled {
    started: Mutex<Sender<()>>,
}

impl StemSeparator for BlockUntilCancelled {
    fn id(&self) -> &str {
        "blocking"
    }
    fn version(&self) -> i32 {
        1
    }
    fn native_rate(&self) -> f64 {
        44_100.0
    }
    fn separate_to(
        &self,
        input: &mut dyn StereoSource,
        sink: &mut dyn StemSink,
        ctx: &dyn AnalysisContext,
    ) -> Result<(), StemError> {
        let mut l = vec![0.0f32; 1000];
        let mut r = vec![0.0f32; 1000];
        let n = input.read(&mut l, &mut r)?;
        for kind in om_stems::StemKind::ALL {
            sink.write(kind, &l[..n], &r[..n])?;
        }
        let _ = self.started.lock().unwrap().send(());
        while !ctx.is_cancelled() {
            std::thread::yield_now();
        }
        Err(StemError::Cancelled)
    }
}

/// Panics on the first call, then behaves like the mock.
struct PanicOnce {
    armed: AtomicBool,
    inner: MockSeparator,
}

impl StemSeparator for PanicOnce {
    fn id(&self) -> &str {
        self.inner.id()
    }
    fn version(&self) -> i32 {
        self.inner.version()
    }
    fn native_rate(&self) -> f64 {
        self.inner.native_rate()
    }
    fn separate_to(
        &self,
        input: &mut dyn StereoSource,
        sink: &mut dyn StemSink,
        ctx: &dyn AnalysisContext,
    ) -> Result<(), StemError> {
        if self.armed.swap(false, Ordering::SeqCst) {
            panic!("separator exploded");
        }
        self.inner.separate_to(input, sink, ctx)
    }
}

struct NoNetwork(Arc<AtomicUsize>);

impl Downloader for NoNetwork {
    fn download(
        &self,
        _url: &str,
        _destination: &Path,
        _max_bytes: u64,
        _on_progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<(), WeightError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        panic!("the licence gate must not reach the network");
    }
}

fn gate(folder: &Path, downloads: Arc<AtomicUsize>, accepted: bool, installed: bool) -> ModelGate {
    let manifest = ModelManifest::htdemucs(
        "https://example.invalid/htdemucs.onnx",
        &"ab".repeat(32),
        1234,
    );
    fs::create_dir_all(folder).unwrap();
    if installed {
        fs::write(folder.join(&manifest.id), b"model").unwrap();
        fs::write(
            folder.join(format!("{}.sha256", manifest.id)),
            &manifest.sha256,
        )
        .unwrap();
    }
    if accepted {
        let digest = {
            use sha2::{Digest, Sha256};
            Sha256::digest(manifest.license_text.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        fs::write(folder.join(format!("{}.license", manifest.id)), digest).unwrap();
    }
    let weights = WeightManager::new(folder.to_path_buf(), Box::new(NoNetwork(downloads)));
    ModelGate::new(manifest, weights)
}

#[test]
fn end_to_end_job_writes_the_cache_and_the_library_row() {
    let fx = fixture(1);
    let svc = service(&fx, Arc::new(MockSeparator::new()), None, None);
    let handle = svc.submit(request(&fx, 0));
    assert_eq!(
        finish(&handle),
        JobStatus::Done,
        "error: {}",
        handle.error()
    );
    assert_eq!(handle.progress(), 1.0);

    let key = svc.key_for(&fx.tracks[0].2);
    let entry = svc.cache().lookup(&key).expect("cache entry");
    assert_eq!(entry.meta.frames, 8820);
    assert_eq!(
        entry.meta.source_rate, 44_100.0,
        "stems at the track's rate"
    );

    let lib = Library::open(&fx.library, OpenMode::ReadOnly, core_migrations()).unwrap();
    let rows = lib.track_stems().for_track(fx.tracks[0].0).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].dir, entry.dir_name);
    assert_eq!(rows[0].bytes as u64, entry.meta.bytes);
}

#[test]
fn cancel_mid_way_leaves_no_partial_directory() {
    let fx = fixture(1);
    let (tx, rx) = channel();
    let svc = service(
        &fx,
        Arc::new(BlockUntilCancelled {
            started: Mutex::new(tx),
        }),
        None,
        None,
    );
    let handle = svc.submit(request(&fx, 0));
    rx.recv_timeout(WAIT).expect("separator started");
    handle.cancel();
    assert_eq!(finish(&handle), JobStatus::Cancelled);

    assert!(
        dir_names(&fx.dir.path().join("stems")).is_empty(),
        "no tmp or final dir"
    );
    assert!(svc.cache().entries().unwrap().is_empty());
    let lib = Library::open(&fx.library, OpenMode::ReadOnly, core_migrations()).unwrap();
    assert!(lib
        .track_stems()
        .for_track(fx.tracks[0].0)
        .unwrap()
        .is_empty());
}

#[test]
fn a_second_submit_is_a_cache_hit() {
    let fx = fixture(1);
    let calls = Arc::new(AtomicUsize::new(0));
    let svc = service(
        &fx,
        Arc::new(Counting {
            inner: MockSeparator::new(),
            calls: Arc::clone(&calls),
        }),
        None,
        None,
    );
    assert_eq!(finish(&svc.submit(request(&fx, 0))), JobStatus::Done);
    assert_eq!(finish(&svc.submit(request(&fx, 0))), JobStatus::Done);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "the separator ran once");
    assert_eq!(svc.cache().entries().unwrap().len(), 1);
}

#[test]
fn a_missing_model_fails_before_any_work_and_without_network() {
    let fx = fixture(1);
    let downloads = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    let g = gate(
        &fx.dir.path().join("models"),
        Arc::clone(&downloads),
        false,
        false,
    );
    let svc = service(
        &fx,
        Arc::new(Counting {
            inner: MockSeparator::new(),
            calls: Arc::clone(&calls),
        }),
        None,
        Some(g),
    );
    let handle = svc.submit(request(&fx, 0));
    assert_eq!(finish(&handle), JobStatus::Failed);
    assert!(
        handle.error().contains("not installed"),
        "{}",
        handle.error()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(downloads.load(Ordering::SeqCst), 0);
}

#[test]
fn an_unaccepted_licence_fails_without_network() {
    let fx = fixture(1);
    let downloads = Arc::new(AtomicUsize::new(0));
    let g = gate(
        &fx.dir.path().join("models"),
        Arc::clone(&downloads),
        false,
        true,
    );
    let svc = service(&fx, Arc::new(MockSeparator::new()), None, Some(g));
    let handle = svc.submit(request(&fx, 0));
    assert_eq!(finish(&handle), JobStatus::Failed);
    assert!(handle.error().contains("licence"), "{}", handle.error());
    assert_eq!(downloads.load(Ordering::SeqCst), 0);
}

#[test]
fn an_installed_model_with_accepted_licence_runs() {
    let fx = fixture(1);
    let downloads = Arc::new(AtomicUsize::new(0));
    let g = gate(
        &fx.dir.path().join("models"),
        Arc::clone(&downloads),
        true,
        true,
    );
    let svc = service(&fx, Arc::new(MockSeparator::new()), None, Some(g));
    assert_eq!(finish(&svc.submit(request(&fx, 0))), JobStatus::Done);
    assert_eq!(downloads.load(Ordering::SeqCst), 0);
}

#[test]
fn a_panicking_separator_fails_its_job_and_the_worker_survives() {
    let fx = fixture(2);
    let sep = Arc::new(PanicOnce {
        armed: AtomicBool::new(true),
        inner: MockSeparator::new(),
    });
    let svc = service(&fx, sep, None, None);
    let first = svc.submit(request(&fx, 0));
    assert_eq!(finish(&first), JobStatus::Failed);
    assert!(
        first.error().contains("separator exploded"),
        "{}",
        first.error()
    );
    assert!(
        dir_names(&fx.dir.path().join("stems")).is_empty(),
        "panic leaves no tmp dir"
    );

    let second = svc.submit(request(&fx, 1));
    assert_eq!(
        finish(&second),
        JobStatus::Done,
        "error: {}",
        second.error()
    );
}

#[test]
fn the_cap_evicts_the_least_recent_stems_and_their_rows() {
    let fx = fixture(2);
    let one_entry = {
        // Measure one entry with an uncapped run in a scratch service.
        let probe = service(&fx, Arc::new(MockSeparator::new()), None, None);
        assert_eq!(finish(&probe.submit(request(&fx, 0))), JobStatus::Done);
        probe.cache().total_bytes().unwrap()
    };
    let fx2 = Fixture {
        dir: TempDir::new().unwrap(),
        library: fx.library.clone(),
        tracks: fx.tracks.clone(),
    };
    let svc = service(
        &fx2,
        Arc::new(MockSeparator::new()),
        Some(one_entry + 1),
        None,
    );
    assert_eq!(finish(&svc.submit(request(&fx2, 0))), JobStatus::Done);
    assert_eq!(finish(&svc.submit(request(&fx2, 1))), JobStatus::Done);

    let entries = svc.cache().entries().unwrap();
    assert_eq!(entries.len(), 1, "the older entry was evicted");
    assert_eq!(entries[0].meta.key.content_hash, fx.tracks[1].2);
    let lib = Library::open(&fx.library, OpenMode::ReadOnly, core_migrations()).unwrap();
    assert!(lib
        .track_stems()
        .for_track(fx.tracks[0].0)
        .unwrap()
        .is_empty());
    assert_eq!(
        lib.track_stems().for_track(fx.tracks[1].0).unwrap().len(),
        1
    );
}

#[test]
fn forgetting_a_track_removes_its_directories_and_rows() {
    let fx = fixture(1);
    let svc = service(&fx, Arc::new(MockSeparator::new()), None, None);
    assert_eq!(finish(&svc.submit(request(&fx, 0))), JobStatus::Done);
    assert_eq!(svc.forget_track(fx.tracks[0].0).unwrap(), 1);
    assert!(svc.cache().entries().unwrap().is_empty());
    let lib = Library::open(&fx.library, OpenMode::ReadOnly, core_migrations()).unwrap();
    assert!(lib.track_stems().list_all_bytes().unwrap().is_empty());
}

#[test]
fn a_row_whose_directory_is_gone_is_reconciled_away() {
    let fx = fixture(1);
    let svc = service(&fx, Arc::new(MockSeparator::new()), None, None);
    assert_eq!(finish(&svc.submit(request(&fx, 0))), JobStatus::Done);
    let entry = svc.cache().entries().unwrap().remove(0);
    fs::remove_dir_all(&entry.path).unwrap();
    assert_eq!(svc.reconcile().unwrap(), 1);
    let lib = Library::open(&fx.library, OpenMode::ReadOnly, core_migrations()).unwrap();
    assert!(lib.track_stems().list_all_bytes().unwrap().is_empty());
}
