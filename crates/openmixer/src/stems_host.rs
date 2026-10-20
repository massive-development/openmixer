//! The app's [`StemHost`]: the model installer (licence answer and verified download), the stem
//! service, and the mock that stands in for an installed model when `--mock-stems` is given.
//!
//! The model folder and the stem cache sit beside the library file (`models/`, `stems/`). The
//! manifest is `models/htdemucs.manifest.json`; without it the model is "not installed" and the
//! page says why. The service is created on first use, once the model is installed (or with the
//! mock), so an install made in the window takes effect without a restart.

use crate::composition::{
    installed_separator, STEM_MANIFEST_FILE, STEM_SEPARATOR_ID, STEM_SEPARATOR_VERSION,
};
use crate::error::AppError;
use om_core::jobs::JobHandle;
use om_stems::{
    clear_stem_cache, InstallState, MockSeparator, ModelGate, ModelInstaller, ModelManifest,
    StemCache, StemRequest, StemService, StemServiceConfig, UrlDownloader, WeightManager,
    HTDEMUCS_LICENSE_NAME, HTDEMUCS_LICENSE_TEXT,
};
use om_ui::settings::{SettingsError, SettingsResult};
use om_ui::stems::{ModelState, StemHost, StemJob};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Name of the stem cache folder beside the library file.
const CACHE_DIR: &str = "stems";
/// Name of the model folder beside the library file.
const MODEL_DIR: &str = "models";

pub struct StemsHost {
    mock: bool,
    library_path: String,
    /// `None` for an in-memory library: no model folder, no cache.
    folder: Option<PathBuf>,
    /// Why there is no installer (no manifest, unreadable manifest, no folder).
    problem: String,
    installer: Option<ModelInstaller>,
    manifest: Option<ModelManifest>,
    service: Option<Arc<StemService>>,
    max_cache_bytes: Option<u64>,
    auto_prepare: bool,
    /// Every job handed out; cancelled by `clear_cache`.
    jobs: Vec<JobHandle>,
}

impl StemsHost {
    /// Reads the manifest (unless `mock`) and sets up the installer. `max_cache_bytes` is the cap
    /// from the preferences (`None` for none); `auto_prepare` the preference of the same name.
    pub fn new(
        mock: bool,
        library_path: String,
        max_cache_bytes: Option<u64>,
        auto_prepare: bool,
    ) -> Self {
        let folder = (library_path != om_ui::library::MEMORY_PATH)
            .then(|| {
                Path::new(&library_path)
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_default()
            })
            .filter(|folder| !folder.as_os_str().is_empty());
        let mut host = Self {
            mock,
            library_path,
            folder,
            problem: String::new(),
            installer: None,
            manifest: None,
            service: None,
            max_cache_bytes,
            auto_prepare,
            jobs: Vec::new(),
        };
        if mock {
            return host;
        }
        let Some(folder) = host.folder.clone() else {
            host.problem = "the library is in memory".into();
            return host;
        };
        let models = folder.join(MODEL_DIR);
        let manifest_path = models.join(STEM_MANIFEST_FILE);
        if !manifest_path.is_file() {
            host.problem = format!("no model manifest at {}", manifest_path.display());
            return host;
        }
        match std::fs::read_to_string(&manifest_path)
            .map_err(|e| e.to_string())
            .and_then(|json| ModelManifest::from_json(&json).map_err(|e| e.to_string()))
        {
            Ok(manifest) => {
                host.installer = Some(ModelInstaller::new(
                    manifest.clone(),
                    models,
                    Box::new(UrlDownloader::default()),
                ));
                host.manifest = Some(manifest);
            }
            Err(error) => host.problem = format!("the model manifest is unreadable: {error}"),
        }
        host
    }

    /// The service, when the model is ready now (installed, or the mock). Creates it on first use.
    pub fn service_if_ready(&mut self) -> Result<Option<Arc<StemService>>, AppError> {
        if !matches!(self.model_state(), ModelState::Installed(_)) {
            return Ok(None);
        }
        self.ensure_service().map(Some)
    }

    fn ensure_service(&mut self) -> Result<Arc<StemService>, AppError> {
        if let Some(service) = &self.service {
            return Ok(Arc::clone(service));
        }
        let folder = self
            .folder
            .clone()
            .ok_or_else(|| AppError::Stems("the library is in memory".into()))?;
        let (separator, gate) = if self.mock {
            (
                Arc::new(MockSeparator::new()) as Arc<dyn om_stems::StemSeparator>,
                None,
            )
        } else {
            let manifest = self
                .manifest
                .clone()
                .ok_or_else(|| AppError::Stems(self.problem.clone()))?;
            let weights =
                WeightManager::new(folder.join(MODEL_DIR), Box::new(UrlDownloader::default()));
            let separator = installed_separator(&weights, &manifest)?
                .ok_or_else(|| AppError::Stems("the stem model is not ready".into()))?;
            (separator, Some(ModelGate::new(manifest, weights)))
        };
        let service = Arc::new(
            StemService::new(StemServiceConfig {
                separator,
                cache_root: folder.join(CACHE_DIR),
                library_path: self.library_path.clone(),
                max_cache_bytes: self.max_cache_bytes,
                gate,
            })
            .map_err(|e| AppError::Stems(e.to_string()))?,
        );
        self.service = Some(Arc::clone(&service));
        Ok(service)
    }

    /// The stem cache root beside the library file; `None` for an in-memory library.
    pub fn cache_root(&self) -> Option<PathBuf> {
        self.folder.as_ref().map(|folder| folder.join(CACHE_DIR))
    }
}

impl StemHost for StemsHost {
    fn model_state(&self) -> ModelState {
        if self.mock {
            return ModelState::Installed(STEM_SEPARATOR_VERSION);
        }
        let Some(installer) = &self.installer else {
            return ModelState::NotInstalled(self.problem.clone());
        };
        match installer.state() {
            InstallState::Installed if cfg!(feature = "onnx") => {
                ModelState::Installed(STEM_SEPARATOR_VERSION)
            }
            InstallState::Installed => ModelState::NotInstalled(
                "this build has no ONNX runtime (the onnx feature is off)".into(),
            ),
            InstallState::Downloading(fraction) => ModelState::Downloading(fraction),
            InstallState::LicenceNotAccepted => ModelState::LicenceNotAccepted,
            InstallState::NotInstalled => ModelState::NotInstalled(String::new()),
            InstallState::Failed(message) => ModelState::Failed(message),
        }
    }

    fn licence(&self) -> Option<(String, String)> {
        if self.mock {
            return Some((HTDEMUCS_LICENSE_NAME.into(), HTDEMUCS_LICENSE_TEXT.into()));
        }
        self.manifest
            .as_ref()
            .map(|m| (m.license_name.clone(), m.license_text.clone()))
    }

    fn accept_and_download(&mut self) -> SettingsResult<()> {
        if self.mock {
            return Ok(());
        }
        let state = self.model_state();
        match (&self.installer, state) {
            (_, ModelState::Installed(_)) => Ok(()),
            (Some(installer), _) if cfg!(feature = "onnx") => {
                // The button is the acceptance: the installer's job asks the licence gate with it.
                installer.install(true);
                Ok(())
            }
            (_, state) => Err(SettingsError::StemsNotReady(state.line())),
        }
    }

    fn cancel_download(&mut self) {
        if let Some(installer) = &self.installer {
            installer.cancel();
        }
    }

    fn separator(&self) -> (String, i32) {
        if self.mock {
            let mock = MockSeparator::new();
            return (
                om_stems::StemSeparator::id(&mock).to_owned(),
                om_stems::StemSeparator::version(&mock),
            );
        }
        (STEM_SEPARATOR_ID.to_owned(), STEM_SEPARATOR_VERSION)
    }

    fn cache_bytes(&self) -> u64 {
        self.cache_root()
            .and_then(|root| StemCache::open(root).ok())
            .and_then(|cache| cache.total_bytes().ok())
            .unwrap_or(0)
    }

    fn clear_cache(&mut self) -> SettingsResult<usize> {
        for job in self.jobs.iter().filter(|job| !job.is_finished()) {
            job.cancel();
        }
        let Some(root) = self.cache_root() else {
            return Ok(0);
        };
        clear_stem_cache(&root, &self.library_path).map_err(|e| SettingsError::Stems(Box::new(e)))
    }

    fn set_max_cache_bytes(&mut self, bytes: u64) {
        let cap = (bytes > 0).then_some(bytes);
        self.max_cache_bytes = cap;
        if let Some(service) = &self.service {
            service.set_max_cache_bytes(cap);
        }
    }

    fn auto_prepare(&self) -> bool {
        self.auto_prepare
    }

    fn set_auto_prepare(&mut self, on: bool) {
        self.auto_prepare = on;
    }

    fn submit(&mut self, job: StemJob) -> SettingsResult<JobHandle> {
        let state = self.model_state();
        if !state.ready() {
            return Err(SettingsError::StemsNotReady(state.line()));
        }
        let service = self
            .ensure_service()
            .map_err(|e| SettingsError::Stems(Box::new(e)))?;
        let handle = service.submit(StemRequest {
            track_id: job.track_id,
            path: job.path,
            content_hash: job.content_hash,
        });
        self.jobs.retain(|job| !job.is_finished());
        self.jobs.push(handle.clone());
        Ok(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library_in(dir: &Path) -> String {
        dir.join("db")
            .join("library.db")
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn without_a_manifest_the_model_is_not_installed_and_says_where() {
        let dir = tempfile::tempdir().expect("dir");
        let host = StemsHost::new(false, library_in(dir.path()), None, false);
        match host.model_state() {
            ModelState::NotInstalled(reason) => assert!(reason.contains("manifest"), "{reason}"),
            other => panic!("unexpected state {other:?}"),
        }
        assert!(host.licence().is_none());
    }

    #[test]
    fn the_mock_is_installed_and_needs_no_manifest() {
        let dir = tempfile::tempdir().expect("dir");
        let mut host = StemsHost::new(true, library_in(dir.path()), None, false);
        assert_eq!(
            host.model_state(),
            ModelState::Installed(STEM_SEPARATOR_VERSION)
        );
        assert_eq!(host.licence().expect("licence").0, HTDEMUCS_LICENSE_NAME);
        assert!(host.service_if_ready().expect("service").is_some());
    }

    #[test]
    fn a_declined_or_missing_model_refuses_jobs_without_creating_a_service() {
        let dir = tempfile::tempdir().expect("dir");
        let mut host = StemsHost::new(false, library_in(dir.path()), None, false);
        assert!(host.service_if_ready().expect("ok").is_none());
        let job = StemJob {
            track_id: 1,
            path: PathBuf::from("/music/a.wav"),
            content_hash: String::new(),
        };
        assert!(matches!(
            host.submit(job),
            Err(SettingsError::StemsNotReady(_))
        ));
        assert!(host.service.is_none());
    }

    #[test]
    fn an_in_memory_library_has_no_model_folder_or_cache() {
        let host = StemsHost::new(false, om_ui::library::MEMORY_PATH.into(), None, false);
        assert_eq!(host.cache_bytes(), 0);
        assert!(host.cache_root().is_none());
    }
}
