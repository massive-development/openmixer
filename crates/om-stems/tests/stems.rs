//! Port of `tests/unit/StemsTest.cpp`: model manifests, licence-gated verified downloads and stem
//! types. Downloads use local files and a loopback HTTP server; nothing leaves the machine.

use om_analysis::{hash_file_content, AnalysisContext, AudioClip, NullAnalysisContext};
use om_stems::{
    Downloader, LicensePrompt, MockSeparator, ModelManifest, StemKind, StemSeparator, StemSet,
    UrlDownloader, WeightError, WeightManager,
};
use std::cell::Cell;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// The real URL downloader (file URLs) that counts calls.
struct CountingDownloader {
    calls: Arc<AtomicUsize>,
}

impl Downloader for CountingDownloader {
    fn download(
        &self,
        url: &str,
        destination: &Path,
        on_progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<(), WeightError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        UrlDownloader.download(url, destination, on_progress)
    }
}

struct CancelledContext;

impl AnalysisContext for CancelledContext {
    fn is_cancelled(&self) -> bool {
        true
    }
    fn set_progress(&self, _fraction: f32) {}
}

struct Fixture {
    _source_dir: tempfile::TempDir,
    _folder_dir: tempfile::TempDir,
    source: PathBuf,
    folder: PathBuf,
    manifest: ModelManifest,
    downloads: Arc<AtomicUsize>,
}

impl Fixture {
    fn new() -> Self {
        let source_dir = tempfile::tempdir().expect("source dir");
        let folder_dir = tempfile::tempdir().expect("model dir");
        let source = source_dir.path().join("weights.bin");
        std::fs::write(&source, "fake weights ".repeat(20_000)).expect("write source");
        let manifest = ModelManifest {
            id: "demo-model.onnx".into(),
            url: format!("file://{}", source.display()),
            sha256: hash_file_content(&source).expect("hash"),
            size_bytes: std::fs::metadata(&source).expect("meta").len(),
            license_name: "MIT".into(),
            license_text: "Permission is hereby granted ...".into(),
        };
        Self {
            folder: folder_dir.path().join("models"),
            _source_dir: source_dir,
            _folder_dir: folder_dir,
            source,
            manifest,
            downloads: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn manager(&self) -> WeightManager {
        WeightManager::new(
            self.folder.clone(),
            Box::new(CountingDownloader {
                calls: Arc::clone(&self.downloads),
            }),
        )
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).expect("read")
}

#[test]
fn model_manifest_parses_and_validates() {
    let json = format!(
        r#"{{"id":"htdemucs.onnx","url":"https://example.org/m.onnx","sha256":"{}","sizeBytes":42,"licenseName":"MIT","licenseText":"text"}}"#,
        "ab".repeat(32)
    );
    let m = ModelManifest::from_json(&json).expect("valid");
    assert_eq!(m.size_bytes, 42);
    assert!(ModelManifest::from_json(&m.to_json()).is_ok());

    let err = ModelManifest::from_json(&json.replace("htdemucs.onnx", "../evil")).unwrap_err();
    assert!(err.to_string().contains("id"));
    assert!(ModelManifest::from_json(&json.replace("abab", "zz")).is_err());
    assert!(ModelManifest::from_json(&json.replace("\"text\"", "\"\"")).is_err());
    assert!(ModelManifest::from_json("[]").is_err());
}

#[test]
fn manifest_lowercases_the_digest_before_checking_it() {
    let json = format!(
        r#"{{"id":"m.onnx","url":"file:///x","sha256":"{}","licenseName":"MIT","licenseText":"t"}}"#,
        "AB".repeat(32)
    );
    let m = ModelManifest::from_json(&json).expect("valid");
    assert_eq!(m.sha256, "ab".repeat(32));
    assert_eq!(m.size_bytes, 0, "missing size is unknown");
}

#[test]
fn weight_manager_requires_licence_before_downloading() {
    let f = Fixture::new();
    let manager = f.manager();
    let decline = |_: &ModelManifest| false;
    let declined = manager.ensure_available(&f.manifest, Some(&decline), &NullAnalysisContext);
    assert!(declined.is_err());
    assert_eq!(f.downloads.load(Ordering::SeqCst), 0);
    assert!(!manager.model_file(&f.manifest).exists());
    assert!(manager
        .ensure_available(&f.manifest, None, &NullAnalysisContext)
        .is_err());
}

#[test]
fn weight_manager_downloads_verifies_once_and_remembers_acceptance() {
    let f = Fixture::new();
    let manager = f.manager();
    let prompts = Cell::new(0);
    let accept = |_: &ModelManifest| {
        prompts.set(prompts.get() + 1);
        true
    };
    let prompt: LicensePrompt<'_> = &accept;
    manager
        .ensure_available(&f.manifest, Some(prompt), &NullAnalysisContext)
        .expect("installs");
    assert!(manager.is_installed(&f.manifest));
    assert!(manager.has_accepted_licence(&f.manifest));
    assert_eq!(
        read(&manager.model_file(&f.manifest)),
        read(&f.source),
        "the installed file is the source"
    );

    manager
        .ensure_available(&f.manifest, Some(prompt), &NullAnalysisContext)
        .expect("already installed");
    assert_eq!(f.downloads.load(Ordering::SeqCst), 1);
    assert_eq!(prompts.get(), 1);

    let mut changed = f.manifest.clone();
    changed.license_text = "new terms".into();
    assert!(!manager.has_accepted_licence(&changed));
}

#[test]
fn weight_manager_rejects_corrupt_or_cancelled_downloads() {
    let f = Fixture::new();
    let manager = f.manager();
    let accept = |_: &ModelManifest| true;
    let prompt: LicensePrompt<'_> = &accept;

    let mut corrupt = f.manifest.clone();
    corrupt.sha256 = "0".repeat(64);
    let bad = manager.ensure_available(&corrupt, Some(prompt), &NullAnalysisContext);
    assert!(bad.unwrap_err().to_string().contains("checksum"));
    assert!(!manager.model_file(&corrupt).exists());
    assert!(!f.folder.join(format!("{}.part", corrupt.id)).exists());

    let mut wrong_size = f.manifest.clone();
    wrong_size.size_bytes += 1;
    let sized = manager.ensure_available(&wrong_size, Some(prompt), &NullAnalysisContext);
    assert!(sized.unwrap_err().to_string().contains("size"));

    let cancelled = manager.ensure_available(&f.manifest, Some(prompt), &CancelledContext);
    assert!(cancelled.is_err());
    assert!(!manager.is_installed(&f.manifest));

    let mut missing = f.manifest.clone();
    missing.url = "file:///nonexistent/weights.onnx".into();
    assert!(manager
        .ensure_available(&missing, Some(prompt), &NullAnalysisContext)
        .is_err());
}

/// Serves `body` once on a loopback port, with `status`, and returns the URL.
fn serve_once(status: &'static str, body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let url = format!(
        "http://{}/weights.onnx",
        listener.local_addr().expect("addr")
    );
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    url
}

#[test]
fn http_download_from_a_loopback_server_is_verified() {
    let f = Fixture::new();
    let body = std::fs::read(&f.source).expect("read source");
    let mut manifest = f.manifest.clone();
    manifest.url = serve_once("200 OK", body);
    let manager = f.manager();
    let accept = |_: &ModelManifest| true;
    let prompt: LicensePrompt<'_> = &accept;
    manager
        .ensure_available(&manifest, Some(prompt), &NullAnalysisContext)
        .expect("installs over HTTP");
    assert!(manager.is_installed(&manifest));
}

#[test]
fn http_error_status_is_a_download_failure() {
    let f = Fixture::new();
    let mut manifest = f.manifest.clone();
    manifest.url = serve_once("404 Not Found", b"missing".to_vec());
    let manager = f.manager();
    let accept = |_: &ModelManifest| true;
    let prompt: LicensePrompt<'_> = &accept;
    assert!(manager
        .ensure_available(&manifest, Some(prompt), &NullAnalysisContext)
        .is_err());
    assert!(!manager.is_installed(&manifest));
}

#[test]
fn https_is_refused_until_a_tls_backend_is_chosen() {
    let dir = tempfile::tempdir().expect("dir");
    let target = dir.path().join("out.bin");
    let err = UrlDownloader
        .download("https://example.org/m.onnx", &target, &mut |_| true)
        .unwrap_err();
    assert!(err.to_string().contains("https is not supported"));
}

#[test]
fn stem_set_completeness_and_names() {
    let mut set = StemSet::new(44_100.0);
    for kind in [StemKind::Drums, StemKind::Bass, StemKind::Vocals] {
        set.stems.insert(kind, vec![0.0; 10]);
    }
    assert!(!set.is_complete(10));
    set.stems.insert(StemKind::Other, vec![0.0; 10]);
    assert!(set.is_complete(10));
    assert!(!set.is_complete(11));
    assert_eq!(StemKind::Vocals.to_string(), "vocals");
}

#[test]
fn mock_separator_splits_into_four_stems_that_sum_back_to_the_input() {
    let clip = AudioClip::new(vec![0.4_f32; 64], 48_000.0);
    let separator = MockSeparator::new();
    let stems = separator
        .separate(&clip, &NullAnalysisContext)
        .expect("separates");
    assert!(stems.is_complete(64));
    assert_eq!(stems.sample_rate, 48_000.0);
    let sum: f32 = StemKind::ALL.iter().map(|k| stems.stems[k][0]).sum();
    assert!((sum - 0.4).abs() < 1.0e-6);
    assert_eq!(separator.interface_version(), om_stems::INTERFACE_VERSION);
}

#[test]
fn mock_separator_gives_nothing_when_cancelled() {
    let clip = AudioClip::new(vec![0.4_f32; 8], 48_000.0);
    assert!(MockSeparator::new()
        .separate(&clip, &CancelledContext)
        .is_none());
}
