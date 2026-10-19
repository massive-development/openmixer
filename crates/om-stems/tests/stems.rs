//! Model manifests, licence-gated verified downloads and stem
//! types. Downloads use local files and a loopback HTTP server; nothing leaves the machine.

use om_analysis::{hash_file_content, AnalysisContext, NullAnalysisContext};
use om_stems::{
    Downloader, LicensePrompt, ManifestError, MockSeparator, ModelContract, ModelManifest,
    SliceSource, StemBuffer, StemError, StemKind, StemSeparator, StemSet, StemSetSink,
    UrlDownloader, WeightError, WeightManager,
};
use std::cell::Cell;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The real URL downloader (file URLs) that counts calls.
struct CountingDownloader {
    calls: Arc<AtomicUsize>,
}

impl Downloader for CountingDownloader {
    fn download(
        &self,
        url: &str,
        destination: &Path,
        max_bytes: u64,
        on_progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<(), WeightError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        UrlDownloader::default().download(url, destination, max_bytes, on_progress)
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
            contract: None,
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

/// A loopback listener that accepts and hangs up without speaking TLS. The HTTPS path must reach
/// the handshake and fail there, not on a "scheme not supported" refusal. Nothing leaves the host.
#[test]
fn https_reaches_the_tls_handshake_and_fails_cleanly_without_a_tls_server() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let url = format!("https://{}/m.onnx", listener.local_addr().expect("addr"));
    std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            drop(stream);
        }
    });
    let dir = tempfile::tempdir().expect("dir");
    let target = dir.path().join("out.bin");
    let err = UrlDownloader::default()
        .download(&url, &target, 0, &mut |_| true)
        .unwrap_err();
    let text = err.to_string();
    assert!(!text.contains("not supported"), "{text}");
    assert!(!target.exists(), "a failed handshake writes nothing");
}

/// Serves `chunks` one at a time with `pause` between them. The total is sent as Content-Length,
/// so the client knows the body size, and nothing waits on the client.
fn serve_chunks(chunks: Vec<Vec<u8>>, pause: Duration) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let url = format!("http://{}/slow.onnx", listener.local_addr().expect("addr"));
    let total: usize = chunks.iter().map(Vec::len).sum();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            let head =
                format!("HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n");
            if stream.write_all(head.as_bytes()).is_err() {
                return;
            }
            for (index, chunk) in chunks.iter().enumerate() {
                if index > 0 {
                    std::thread::sleep(pause);
                }
                if stream.write_all(chunk).is_err() {
                    return;
                }
            }
        }
    });
    url
}

/// Serves `head` then `body` (close-delimited: no Content-Length), then waits `stall` before
/// closing. Used to check the read timeout and the size cap, which the server cannot bypass.
fn serve_close_delimited(body: Vec<u8>, stall: Duration) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let url = format!("http://{}/open.onnx", listener.local_addr().expect("addr"));
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            if stream
                .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")
                .is_err()
            {
                return;
            }
            let _ = stream.write_all(&body);
            std::thread::sleep(stall);
        }
    });
    url
}

#[test]
fn a_slow_body_longer_than_the_overall_budget_still_downloads() {
    // Five chunks with 120 ms between them: about 480 ms in total, more than the injected 300 ms.
    // Each gap is under the 300 ms read timeout, so the download must not be cut short.
    let f = Fixture::new();
    let body = std::fs::read(&f.source).expect("read source");
    let chunks: Vec<Vec<u8>> = body
        .chunks(body.len() / 5 + 1)
        .map(<[u8]>::to_vec)
        .collect();
    let url = serve_chunks(chunks, Duration::from_millis(120));
    let downloader =
        UrlDownloader::with_timeouts(Duration::from_millis(300), Duration::from_millis(300));
    let dir = tempfile::tempdir().expect("dir");
    let target = dir.path().join("slow.part");
    let started = Instant::now();
    downloader
        .download(&url, &target, 0, &mut |_| true)
        .expect("a steady slow body completes");
    assert!(started.elapsed() > Duration::from_millis(300));
    assert_eq!(std::fs::read(&target).expect("read"), body);
}

#[test]
fn a_stalled_connection_hits_the_read_timeout() {
    let url = serve_close_delimited(b"partial".to_vec(), Duration::from_millis(1500));
    let dir = tempfile::tempdir().expect("dir");
    let target = dir.path().join("stall.part");
    let downloader =
        UrlDownloader::with_timeouts(Duration::from_millis(200), Duration::from_millis(200));
    let started = Instant::now();
    assert!(downloader
        .download(&url, &target, 0, &mut |_| true)
        .is_err());
    assert!(started.elapsed() < Duration::from_millis(1200));
}

#[test]
fn a_body_over_the_manifest_size_is_cut_off_before_the_hash_check() {
    let url = serve_close_delimited(vec![7_u8; 64 * 1024], Duration::from_millis(0));
    let dir = tempfile::tempdir().expect("dir");
    let target = dir.path().join("big.part");
    let err = UrlDownloader::default()
        .download(&url, &target, 1000, &mut |_| true)
        .unwrap_err();
    assert!(err.to_string().contains("more than 1000"), "{err}");
    let written = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
    assert!(written <= 1000, "wrote {written} bytes past the cap");
}

#[test]
fn stem_set_completeness_and_names() {
    let mut set = StemSet::new(44_100.0);
    let buffer = |n: usize| StemBuffer {
        left: vec![0.0; n],
        right: vec![0.0; n],
    };
    for kind in [StemKind::Drums, StemKind::Bass, StemKind::Vocals] {
        set.stems.insert(kind, buffer(10));
    }
    assert!(!set.is_complete(10));
    set.stems.insert(StemKind::Other, buffer(10));
    assert!(set.is_complete(10));
    assert!(!set.is_complete(11));
    assert_eq!(StemKind::Vocals.to_string(), "vocals");
}

#[test]
fn from_mono_duplicates_the_channel() {
    let set = StemSet::from_mono(44_100.0, [(StemKind::Bass, vec![0.5, 0.25])]);
    let bass = &set.stems[&StemKind::Bass];
    assert_eq!(bass.left, bass.right);
    assert_eq!(bass.len(), 2);
}

#[test]
fn mock_separator_splits_into_four_stems_that_sum_back_to_the_input() {
    let samples = vec![0.4_f32; 10_000];
    let separator = MockSeparator::new();
    let mut sink = StemSetSink::new(48_000.0);
    separator
        .separate_to(
            &mut SliceSource::mono(&samples, 48_000.0),
            &mut sink,
            &NullAnalysisContext,
        )
        .expect("separates");
    assert!(sink.max_chunk <= 4096, "streamed in chunks");
    let stems = sink.into_set();
    assert!(stems.is_complete(10_000));
    assert_eq!(stems.sample_rate, 48_000.0);
    let sum: f32 = StemKind::ALL
        .iter()
        .map(|k| stems.stems[k].left[9_999])
        .sum();
    assert!((sum - 0.4).abs() < 1.0e-6);
    assert_eq!(separator.interface_version(), 2);
}

#[test]
fn mock_separator_stops_when_cancelled() {
    let samples = vec![0.4_f32; 8];
    let mut sink = StemSetSink::new(48_000.0);
    let result = MockSeparator::new().separate_to(
        &mut SliceSource::mono(&samples, 48_000.0),
        &mut sink,
        &CancelledContext,
    );
    assert_eq!(result, Err(StemError::Cancelled));
    assert!(sink.into_set().stems.is_empty());
}

#[test]
fn manifest_contract_round_trips_and_defaults() {
    let mut f = Fixture::new();
    assert_eq!(f.manifest.contract_or_default(), ModelContract::htdemucs());
    let mut contract = ModelContract::htdemucs();
    contract.segment_samples = 1000;
    contract.spec_input = Some("z".into());
    contract.spec_output = Some("zout".into());
    f.manifest.contract = Some(contract);
    let parsed = ModelManifest::from_json(&f.manifest.to_json()).expect("parses");
    assert_eq!(parsed, f.manifest);
    let bad = f.manifest.to_json().replace("\"drums\"", "\"bass\"");
    assert!(matches!(
        ModelManifest::from_json(&bad),
        Err(ManifestError::InvalidContract(_))
    ));
}

#[test]
fn file_stereo_source_decodes_a_wav_in_chunks() {
    use om_stems::{FileStereoSource, StereoSource};
    // 16-bit stereo PCM WAV, 1000 frames: left ramps up, right is silent.
    let frames = 1000u32;
    let mut data = Vec::new();
    for i in 0..frames {
        data.extend_from_slice(&((i as i16) * 16).to_le_bytes());
        data.extend_from_slice(&0i16.to_le_bytes());
    }
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&44_100u32.to_le_bytes());
    wav.extend_from_slice(&(44_100u32 * 4).to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("ramp.wav");
    std::fs::write(&path, wav).expect("write");

    let mut source = FileStereoSource::open(&path).expect("opens");
    assert_eq!(source.sample_rate(), 44_100.0);
    assert_eq!(source.total_frames(), Some(1000));
    let (mut l, mut r) = (vec![0.0f32; 400], vec![0.0f32; 400]);
    let mut total = 0;
    let mut last = 0.0;
    loop {
        let n = source.read(&mut l, &mut r).expect("reads");
        if n == 0 {
            break;
        }
        assert!(r[..n].iter().all(|&v| v == 0.0));
        last = l[n - 1];
        total += n;
    }
    assert_eq!(total, 1000);
    assert!(last > 0.4, "ramp reached {last}");
}
