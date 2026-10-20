//! Model installer: the licence answer gates the download (a declined licence never reaches the
//! downloader), a verified file installs, a checksum mismatch fails, and cancel leaves no `.part`.
//! Downloads use `file://` URLs only; no network.

use om_core::jobs::{JobHandle, JobStatus};
use om_stems::{
    Downloader, InstallState, ModelInstaller, ModelManifest, UrlDownloader, WeightError,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const WAIT: Duration = Duration::from_secs(30);

/// Counts calls and delegates to the real `file://` downloader.
struct Counting {
    calls: Arc<AtomicUsize>,
    inner: UrlDownloader,
}

impl Downloader for Counting {
    fn download(
        &self,
        url: &str,
        destination: &Path,
        max_bytes: u64,
        on_progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<(), WeightError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner
            .download(url, destination, max_bytes, on_progress)
    }
}

/// Reports progress until the caller cancels, then gives up as a cancel would.
struct Slow {
    calls: Arc<AtomicUsize>,
}

impl Downloader for Slow {
    fn download(
        &self,
        url: &str,
        _destination: &Path,
        _max_bytes: u64,
        on_progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<(), WeightError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        for step in 0..10_000 {
            std::thread::sleep(Duration::from_millis(1));
            if !on_progress((step % 100) as f32 / 100.0) {
                return Err(WeightError::Download {
                    url: url.to_owned(),
                    reason: "cancelled".into(),
                });
            }
        }
        Ok(())
    }
}

struct Fixture {
    dir: TempDir,
    manifest: ModelManifest,
    folder: PathBuf,
}

/// A model file served from a `file://` URL, with its real digest in the manifest.
fn fixture(bytes: &[u8]) -> Fixture {
    let dir = TempDir::new().expect("tempdir");
    let source = dir.path().join("source.onnx");
    fs::write(&source, bytes).expect("write source");
    let sha = om_analysis::hash_file_content(&source).expect("digest");
    let url = format!("file://{}", source.display());
    let manifest = ModelManifest::htdemucs(&url, &sha, bytes.len() as u64);
    let folder = dir.path().join("models");
    Fixture {
        dir,
        manifest,
        folder,
    }
}

fn wait(handle: &JobHandle) -> JobStatus {
    assert!(handle.wait(WAIT), "install did not finish");
    handle.status()
}

#[test]
fn a_declined_licence_never_reaches_the_downloader() {
    let fx = fixture(b"model bytes");
    let calls = Arc::new(AtomicUsize::new(0));
    let installer = ModelInstaller::new(
        fx.manifest.clone(),
        fx.folder.clone(),
        Box::new(Counting {
            calls: Arc::clone(&calls),
            inner: UrlDownloader::default(),
        }),
    );
    assert_eq!(installer.state(), InstallState::LicenceNotAccepted);
    let handle = installer.install(false);
    assert_eq!(wait(&handle), JobStatus::Failed);
    assert!(handle.error().contains("licence"), "{}", handle.error());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "no network before acceptance"
    );
    assert_eq!(installer.state(), InstallState::LicenceNotAccepted);
    assert!(!installer.model_file().exists());
    drop(fx.dir);
}

#[test]
fn an_accepted_install_downloads_verifies_and_installs() {
    let fx = fixture(b"model bytes");
    let calls = Arc::new(AtomicUsize::new(0));
    let installer = ModelInstaller::new(
        fx.manifest.clone(),
        fx.folder.clone(),
        Box::new(Counting {
            calls: Arc::clone(&calls),
            inner: UrlDownloader::default(),
        }),
    );
    let handle = installer.install(true);
    assert_eq!(wait(&handle), JobStatus::Done);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(installer.state(), InstallState::Installed);
    assert_eq!(
        fs::read(installer.model_file()).expect("installed file"),
        b"model bytes"
    );
    // A second install of an installed model is a no-op: no download.
    assert_eq!(wait(&installer.install(true)), JobStatus::Done);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn a_checksum_mismatch_fails_and_installs_nothing() {
    let fx = fixture(b"model bytes");
    let mut manifest = fx.manifest.clone();
    manifest.sha256 = "0".repeat(64);
    let installer = ModelInstaller::new(
        manifest,
        fx.folder.clone(),
        Box::new(UrlDownloader::default()),
    );
    let handle = installer.install(true);
    assert_eq!(wait(&handle), JobStatus::Failed);
    assert!(handle.error().contains("checksum"), "{}", handle.error());
    assert!(matches!(installer.state(), InstallState::Failed(_)));
    assert!(!installer.model_file().exists());
}

#[test]
fn cancel_stops_the_download_and_leaves_no_model() {
    let fx = fixture(b"model bytes");
    let calls = Arc::new(AtomicUsize::new(0));
    let installer = ModelInstaller::new(
        fx.manifest.clone(),
        fx.folder.clone(),
        Box::new(Slow {
            calls: Arc::clone(&calls),
        }),
    );
    let handle = installer.install(true);
    let started = Instant::now();
    while !matches!(installer.state(), InstallState::Downloading(p) if p > 0.0) {
        assert!(started.elapsed() < WAIT, "download never reported progress");
        std::thread::sleep(Duration::from_millis(2));
    }
    installer.cancel();
    assert!(handle.wait(WAIT));
    assert!(!installer.model_file().exists());
    assert_ne!(installer.state(), InstallState::Installed);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
