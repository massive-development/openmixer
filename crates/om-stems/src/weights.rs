//! Fetches model weights on first use. Port of `src/stems/WeightManager.cpp`.
//!
//! The user must accept the model's licence (a prompt callback) before anything is downloaded. The
//! file is fetched to `<id>.part`, checked against the manifest's size and SHA-256, and moved into the
//! model folder. A `<id>.sha256` marker avoids re-hashing large files on every start, and a
//! `<id>.license` file remembers the accepted licence by its digest. Blocking: call from a background
//! thread, one call per model at a time.

use crate::manifest::ModelManifest;
use om_analysis::{hash_file_content, AnalysisContext};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

/// Per-phase HTTP budgets. Both bound a single stalled step (connect, or one read); neither limits
/// the whole transfer, so a large model on a slow link still finishes.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(15);
const CHUNK: usize = 1 << 16;

#[derive(Debug, Error)]
pub enum WeightError {
    #[error("licence not accepted for {0}")]
    LicenceNotAccepted(String),
    #[error("cannot download {url}: {reason}")]
    Download { url: String, reason: String },
    #[error("cancelled")]
    Cancelled,
    #[error("size mismatch for {0}")]
    SizeMismatch(String),
    #[error("checksum mismatch for {0}")]
    ChecksumMismatch(String),
    #[error("cannot install {0}")]
    Install(String),
    #[error("cannot use {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Writes `url` to `destination`. `max_bytes` caps what is written (0 = no cap): the transfer is
/// abandoned as soon as it would exceed it, so a hostile or broken server cannot fill the disk.
/// `on_progress` gets 0..=1 after each chunk and returns false to abort.
pub trait Downloader: Send {
    fn download(
        &self,
        url: &str,
        destination: &Path,
        max_bytes: u64,
        on_progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<(), WeightError>;
}

/// Downloads `file://`, `http://` and `https://` URLs. HTTPS goes through rustls with the
/// webpki root bundle, as the C++ tree's `juce::URL` did through the platform stack.
#[derive(Debug, Clone, Copy)]
pub struct UrlDownloader {
    connect_timeout: Duration,
    read_timeout: Duration,
}

impl Default for UrlDownloader {
    fn default() -> Self {
        Self {
            connect_timeout: CONNECT_TIMEOUT,
            read_timeout: READ_TIMEOUT,
        }
    }
}

impl UrlDownloader {
    /// A downloader with explicit connect and per-read budgets. There is deliberately no overall
    /// deadline.
    pub fn with_timeouts(connect_timeout: Duration, read_timeout: Duration) -> Self {
        Self {
            connect_timeout,
            read_timeout,
        }
    }
}

impl Downloader for UrlDownloader {
    fn download(
        &self,
        url: &str,
        destination: &Path,
        max_bytes: u64,
        on_progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<(), WeightError> {
        let fail = |reason: String| WeightError::Download {
            url: url.to_owned(),
            reason,
        };
        let (mut reader, total): (Box<dyn Read>, Option<u64>) =
            if let Some(path) = url.strip_prefix("file://") {
                let file = File::open(path).map_err(|e| fail(e.to_string()))?;
                let total = file.metadata().ok().map(|m| m.len());
                (Box::new(file), total)
            } else if url.starts_with("http://") || url.starts_with("https://") {
                let agent = ureq::AgentBuilder::new()
                    .timeout_connect(self.connect_timeout)
                    .timeout_read(self.read_timeout)
                    .build();
                let response = agent.get(url).call().map_err(|e| fail(e.to_string()))?;
                let total = response
                    .header("Content-Length")
                    .and_then(|v| v.parse::<u64>().ok());
                (Box::new(response.into_reader()), total)
            } else {
                return Err(fail("unsupported URL scheme".into()));
            };

        let _ = fs::remove_file(destination);
        let mut out = File::create(destination).map_err(|e| WeightError::Io {
            path: destination.display().to_string(),
            source: e,
        })?;
        copy_with_progress(&mut reader, total, max_bytes, &mut out, on_progress).map_err(|e| {
            match e {
                WeightError::Download { reason, .. } => fail(reason),
                other => other,
            }
        })?;
        out.flush().map_err(|e| WeightError::Io {
            path: destination.display().to_string(),
            source: e,
        })
    }
}

fn copy_with_progress(
    reader: &mut dyn Read,
    total: Option<u64>,
    max_bytes: u64,
    out: &mut File,
    on_progress: &mut dyn FnMut(f32) -> bool,
) -> Result<(), WeightError> {
    let mut buffer = vec![0_u8; CHUNK];
    let mut written: u64 = 0;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|e| WeightError::Download {
                url: String::new(),
                reason: e.to_string(),
            })?;
        if read == 0 {
            return Ok(());
        }
        if max_bytes > 0 && written + read as u64 > max_bytes {
            return Err(WeightError::Download {
                url: String::new(),
                reason: format!("server sent more than {max_bytes} bytes"),
            });
        }
        out.write_all(&buffer[..read])
            .map_err(|e| WeightError::Io {
                path: String::new(),
                source: e,
            })?;
        written += read as u64;
        let fraction = match total {
            Some(t) if t > 0 => written as f32 / t as f32,
            _ => 0.0,
        };
        if !on_progress(fraction) {
            return Err(WeightError::Cancelled);
        }
    }
}

/// Asks the user to accept a model's licence. True = accepted.
pub type LicensePrompt<'a> = &'a dyn Fn(&ModelManifest) -> bool;

pub struct WeightManager {
    folder: PathBuf,
    downloader: Box<dyn Downloader>,
}

fn licence_digest(manifest: &ModelManifest) -> String {
    hex(&Sha256::digest(manifest.license_text.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn read_trimmed(path: &Path) -> String {
    fs::read_to_string(path)
        .map(|s| s.trim().to_owned())
        .unwrap_or_default()
}

impl WeightManager {
    pub fn new(folder: PathBuf, downloader: Box<dyn Downloader>) -> Self {
        Self { folder, downloader }
    }

    pub fn model_file(&self, manifest: &ModelManifest) -> PathBuf {
        self.folder.join(&manifest.id)
    }

    fn marker_file(&self, manifest: &ModelManifest) -> PathBuf {
        self.folder.join(format!("{}.sha256", manifest.id))
    }

    fn acceptance_file(&self, manifest: &ModelManifest) -> PathBuf {
        self.folder.join(format!("{}.license", manifest.id))
    }

    /// True when the verified file is present and its marker matches the manifest's digest.
    pub fn is_installed(&self, manifest: &ModelManifest) -> bool {
        self.model_file(manifest).is_file()
            && read_trimmed(&self.marker_file(manifest)) == manifest.sha256
    }

    /// True when this exact licence text has been accepted for the model before.
    pub fn has_accepted_licence(&self, manifest: &ModelManifest) -> bool {
        read_trimmed(&self.acceptance_file(manifest)) == licence_digest(manifest)
    }

    /// Ensures the verified model file is present, downloading it if needed. `prompt` is asked only
    /// when the licence has not been accepted yet; `None` means no one can accept it, so the call
    /// fails in that case.
    pub fn ensure_available(
        &self,
        manifest: &ModelManifest,
        prompt: Option<LicensePrompt<'_>>,
        context: &dyn AnalysisContext,
    ) -> Result<(), WeightError> {
        if self.is_installed(manifest) {
            return Ok(());
        }
        fs::create_dir_all(&self.folder).map_err(|e| WeightError::Io {
            path: self.folder.display().to_string(),
            source: e,
        })?;
        self.accept_licence(manifest, prompt)?;
        self.fetch_and_verify(manifest, context)
    }

    fn accept_licence(
        &self,
        manifest: &ModelManifest,
        prompt: Option<LicensePrompt<'_>>,
    ) -> Result<(), WeightError> {
        if self.has_accepted_licence(manifest) {
            return Ok(());
        }
        match prompt {
            Some(ask) if ask(manifest) => {
                fs::write(self.acceptance_file(manifest), licence_digest(manifest)).map_err(|e| {
                    WeightError::Io {
                        path: self.acceptance_file(manifest).display().to_string(),
                        source: e,
                    }
                })
            }
            _ => Err(WeightError::LicenceNotAccepted(manifest.id.clone())),
        }
    }

    fn fetch_and_verify(
        &self,
        manifest: &ModelManifest,
        context: &dyn AnalysisContext,
    ) -> Result<(), WeightError> {
        let part = self.folder.join(format!("{}.part", manifest.id));
        let outcome = self.download_verified(manifest, &part, context);
        if outcome.is_err() {
            let _ = fs::remove_file(&part);
        }
        outcome?;
        fs::write(self.marker_file(manifest), &manifest.sha256).map_err(|e| WeightError::Io {
            path: self.marker_file(manifest).display().to_string(),
            source: e,
        })
    }

    fn download_verified(
        &self,
        manifest: &ModelManifest,
        part: &Path,
        context: &dyn AnalysisContext,
    ) -> Result<(), WeightError> {
        let mut progress = |p: f32| {
            context.set_progress(p);
            !context.is_cancelled()
        };
        self.downloader
            .download(&manifest.url, part, manifest.size_bytes, &mut progress)?;

        let size = fs::metadata(part).map(|m| m.len()).unwrap_or(0);
        if manifest.size_bytes > 0 && size != manifest.size_bytes {
            return Err(WeightError::SizeMismatch(manifest.id.clone()));
        }
        if hash_file_content(part).as_deref() != Some(manifest.sha256.as_str()) {
            return Err(WeightError::ChecksumMismatch(manifest.id.clone()));
        }
        fs::rename(part, self.model_file(manifest))
            .map_err(|_| WeightError::Install(manifest.id.clone()))
    }
}
