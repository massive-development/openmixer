//! Installs the stem model in the background: the licence answer and the verified download, run as
//! one job on a single worker. The answer to the licence dialog decides whether the download may
//! start: `install(false)` returns a job that fails at the licence step and never reaches the
//! downloader. Only the worker's [`WeightManager`] downloads; the state reads use a second manager
//! that has no downloader, so polling the status can never start network I/O.

use crate::manifest::ModelManifest;
use crate::weights::{Downloader, WeightError, WeightManager};
use om_analysis::AnalysisContext;
use om_core::jobs::{JobCtx, JobHandle, WorkerPool};
use om_core::sync::LockExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// What the settings page shows for the model.
#[derive(Debug, Clone, PartialEq)]
pub enum InstallState {
    /// Not on disk and the licence has not been accepted.
    LicenceNotAccepted,
    /// Not on disk; the licence was accepted but the file is missing or unverified.
    NotInstalled,
    /// A download is queued or running; 0..=1.
    Downloading(f32),
    /// On disk, checksum marker matches the manifest.
    Installed,
    /// The last install failed for this reason.
    Failed(String),
}

/// Adapts the job's cancel and progress to the weight manager's context.
struct InstallProgress<'a>(&'a JobCtx);

impl AnalysisContext for InstallProgress<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    fn set_progress(&self, fraction: f32) {
        self.0.set_progress(fraction);
    }
}

/// Never asked to download: used by the state probe.
struct NoDownloader;

impl Downloader for NoDownloader {
    fn download(
        &self,
        url: &str,
        _destination: &std::path::Path,
        _max_bytes: u64,
        _on_progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<(), WeightError> {
        Err(WeightError::Download {
            url: url.to_owned(),
            reason: "the state probe does not download".into(),
        })
    }
}

/// Installs one model. Jobs run one at a time, in the order submitted.
pub struct ModelInstaller {
    manifest: ModelManifest,
    /// Reads the on-disk state only (`is_installed`, `has_accepted_licence`).
    probe: WeightManager,
    /// Outcome of the last finished install, when it did not succeed. Cleared by a success.
    last_failure: Arc<Mutex<Option<InstallState>>>,
    pool: WorkerPool<bool>,
    current: Mutex<Option<JobHandle>>,
}

impl ModelInstaller {
    /// `folder` is the model folder; `downloader` fetches the file (the UrlDownloader in the app,
    /// a fake in tests).
    pub fn new(manifest: ModelManifest, folder: PathBuf, downloader: Box<dyn Downloader>) -> Self {
        let probe = WeightManager::new(folder.clone(), Box::new(NoDownloader));
        let worker = Arc::new(Mutex::new(WeightManager::new(folder, downloader)));
        let last_failure: Arc<Mutex<Option<InstallState>>> = Arc::new(Mutex::new(None));
        let outcome = Arc::clone(&last_failure);
        let job_manifest = manifest.clone();
        let pool = WorkerPool::new(1, "model-install", move |ctx: &JobCtx, accepted: bool| {
            let weights = worker.lock_unpoisoned();
            let prompt = move |_: &ModelManifest| accepted;
            let progress = InstallProgress(ctx);
            match weights.ensure_available(&job_manifest, Some(&prompt), &progress) {
                Ok(()) => *outcome.lock_unpoisoned() = None,
                Err(_) if ctx.is_cancelled() => {}
                Err(error) => {
                    let message = error.to_string();
                    let state = match error {
                        WeightError::LicenceNotAccepted(_) => InstallState::LicenceNotAccepted,
                        _ => InstallState::Failed(message.clone()),
                    };
                    *outcome.lock_unpoisoned() = Some(state);
                    ctx.fail(message);
                }
            }
        });
        Self {
            manifest,
            probe,
            last_failure,
            pool,
            current: Mutex::new(None),
        }
    }

    pub fn manifest(&self) -> &ModelManifest {
        &self.manifest
    }

    /// Queues the install. `accepted` is the user's answer to the licence dialog: `false` makes
    /// the job fail at the licence step, with no network I/O. The handle reports progress and
    /// errors; [`ModelInstaller::cancel`] stops it.
    pub fn install(&self, accepted: bool) -> JobHandle {
        let handle = self.pool.submit(accepted);
        *self.current.lock_unpoisoned() = Some(handle.clone());
        handle
    }

    /// Cancels the running or queued install, if any. A partial `.part` file is removed.
    pub fn cancel(&self) {
        if let Some(job) = self.current.lock_unpoisoned().as_ref() {
            job.cancel();
        }
    }

    /// The model's state now. Never downloads.
    pub fn state(&self) -> InstallState {
        let downloading = self
            .current
            .lock_unpoisoned()
            .as_ref()
            .filter(|job| !job.is_finished())
            .map(|job| job.progress());
        if let Some(fraction) = downloading {
            return InstallState::Downloading(fraction);
        }
        if self.probe.is_installed(&self.manifest) {
            return InstallState::Installed;
        }
        if let Some(failure) = self.last_failure.lock_unpoisoned().clone() {
            return failure;
        }
        if self.probe.has_accepted_licence(&self.manifest) {
            InstallState::NotInstalled
        } else {
            InstallState::LicenceNotAccepted
        }
    }

    /// Where the verified model file is (or will be).
    pub fn model_file(&self) -> PathBuf {
        self.probe.model_file(&self.manifest)
    }
}
