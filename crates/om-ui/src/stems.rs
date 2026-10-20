//! Stem separation for the UI: the model state, the per-track prepare jobs with live progress and
//! cancel, the stems label of a track, and the auto-prepare trigger. The separator and the model
//! download sit behind [`StemHost`], which the app implements over `om-stems`, so this module has
//! no separator types. Nothing here starts a job by itself except the auto-prepare trigger, which
//! only fires for a track analysed after the library opened and only when the preference is on.

use std::collections::BTreeMap;
use std::path::PathBuf;

use om_core::jobs::{JobHandle, JobStatus};
use om_library::{Library, PrepStatus};

use crate::settings::{SettingsError, SettingsResult};

/// Where the stem model is.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelState {
    /// Not on disk. The text says why (no manifest, a bad manifest, or a missing file).
    NotInstalled(String),
    /// Not on disk and the licence has not been accepted.
    LicenceNotAccepted,
    /// A download is queued or running; 0..=1.
    Downloading(f32),
    /// Installed; the separator's version.
    Installed(i32),
    /// The last install failed.
    Failed(String),
}

impl ModelState {
    /// Whether stems can be prepared now.
    pub fn ready(&self) -> bool {
        matches!(self, ModelState::Installed(_))
    }

    /// The status line shown under the model state.
    pub fn line(&self) -> String {
        match self {
            ModelState::NotInstalled(reason) if reason.is_empty() => "Not installed".to_owned(),
            ModelState::NotInstalled(reason) => format!("Not installed: {reason}"),
            ModelState::LicenceNotAccepted => {
                "Licence not accepted: read the licence and accept it to download the model"
                    .to_owned()
            }
            ModelState::Downloading(fraction) => {
                format!(
                    "Downloading {}%",
                    (fraction.clamp(0.0, 1.0) * 100.0).round() as u32
                )
            }
            ModelState::Installed(version) => format!("Installed v{version}"),
            ModelState::Failed(message) => format!("Error: {message}"),
        }
    }
}

/// One track to separate, as the library holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StemJob {
    pub track_id: i64,
    pub path: PathBuf,
    /// Lowercase hex SHA-256 of the file (the library's content hash).
    pub content_hash: String,
}

/// What the app provides. Implemented over `om-stems` by the app; tests use fakes.
pub trait StemHost {
    fn model_state(&self) -> ModelState;
    /// The licence the dialog shows: name and text. `None` when there is no model to license.
    fn licence(&self) -> Option<(String, String)>;
    /// The user accepted the licence: start the download (no network before this call).
    fn accept_and_download(&mut self) -> SettingsResult<()>;
    fn cancel_download(&mut self);
    /// The separator's id and version, as the cache and the library index them.
    fn separator(&self) -> (String, i32);
    /// Bytes the stem cache holds.
    fn cache_bytes(&self) -> u64;
    /// Cancels the jobs the host runs, removes every stem set and index row, and returns how many
    /// sets were removed.
    fn clear_cache(&mut self) -> SettingsResult<usize>;
    /// The cap the next committed job enforces; 0 for none.
    fn set_max_cache_bytes(&mut self, bytes: u64);
    /// The preference: prepare stems for tracks analysed after the library opened.
    fn auto_prepare(&self) -> bool;
    fn set_auto_prepare(&mut self, on: bool);
    /// Queues one separation. The handle reports progress, status and errors.
    fn submit(&mut self, job: StemJob) -> SettingsResult<JobHandle>;
}

/// The stems indicator of a track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StemLabel {
    None,
    /// Queued or running; 0..=100.
    Preparing(u32),
    Ready,
    /// The last job for this track failed.
    Failed,
}

impl StemLabel {
    pub fn text(&self) -> String {
        match self {
            StemLabel::None => "None".to_owned(),
            StemLabel::Preparing(percent) => format!("Preparing {percent}%"),
            StemLabel::Ready => "Ready".to_owned(),
            StemLabel::Failed => "Failed".to_owned(),
        }
    }
}

/// True when a track analysed with `status` should get stems: the preference is on, the analysis
/// finished, and the track was ingested after the library opened (`track_id` above the watermark).
/// Tracks loaded from the library never qualify, whatever their id.
pub fn auto_prepare_wanted(
    enabled: bool,
    watermark: i64,
    track_id: i64,
    status: PrepStatus,
) -> bool {
    enabled && status == PrepStatus::Done && track_id > watermark
}

/// The largest track id in the library now. Tracks above it were ingested later.
pub fn watermark_of(library: &Library) -> i64 {
    library
        .database()
        .connection()
        .query_row("SELECT COALESCE(MAX(id), 0) FROM tracks;", [], |row| {
            row.get(0)
        })
        .unwrap_or(0)
}

/// The prepare jobs of this session, keyed by track id, and the auto-prepare watermark.
#[derive(Default)]
pub struct StemQueue {
    jobs: BTreeMap<i64, JobHandle>,
    watermark: i64,
}

impl StemQueue {
    /// `watermark` from [`watermark_of`] at the time the library opened.
    pub fn new(watermark: i64) -> Self {
        Self {
            jobs: BTreeMap::new(),
            watermark,
        }
    }

    pub fn watermark(&self) -> i64 {
        self.watermark
    }

    /// The stems indicator of `track_id`: a live job wins, then the library's index, else none.
    pub fn label(&self, library: &Library, track_id: i64, separator: (&str, i32)) -> StemLabel {
        if let Some(job) = self.jobs.get(&track_id) {
            match job.status() {
                JobStatus::Failed => return StemLabel::Failed,
                JobStatus::Done => {}
                JobStatus::Cancelled => return StemLabel::None,
                _ if !job.is_finished() => {
                    let percent = (job.progress().clamp(0.0, 1.0) * 100.0).round() as u32;
                    return StemLabel::Preparing(percent);
                }
                _ => {}
            }
        }
        match library
            .track_stems()
            .get(track_id, separator.0, separator.1)
        {
            Ok(Some(_)) => StemLabel::Ready,
            _ => StemLabel::None,
        }
    }

    /// Queues separation for `tracks` (those not already ready or preparing). Refused with the
    /// status line when the model is not ready. Returns how many were queued.
    pub fn prepare(
        &mut self,
        host: &mut dyn StemHost,
        library: &Library,
        tracks: &[StemJob],
    ) -> SettingsResult<usize> {
        let state = host.model_state();
        if !state.ready() {
            return Err(SettingsError::StemsNotReady(state.line()));
        }
        let separator = host.separator();
        let mut queued = 0;
        for track in tracks {
            if track.content_hash.is_empty() {
                return Err(SettingsError::TrackNotHashed);
            }
            match self.label(library, track.track_id, (&separator.0, separator.1)) {
                StemLabel::Ready | StemLabel::Preparing(_) => continue,
                StemLabel::None | StemLabel::Failed => {}
            }
            let handle = host.submit(track.clone())?;
            self.jobs.insert(track.track_id, handle);
            queued += 1;
        }
        Ok(queued)
    }

    /// Cancels the prepare job of one track. No-op when none runs.
    pub fn cancel(&mut self, track_id: i64) {
        if let Some(job) = self.jobs.get(&track_id) {
            job.cancel();
        }
    }

    /// Cancels every prepare job that has not finished.
    pub fn cancel_all(&mut self) {
        for job in self.jobs.values().filter(|job| !job.is_finished()) {
            job.cancel();
        }
    }

    /// Jobs queued or running.
    pub fn in_flight(&self) -> usize {
        self.jobs.values().filter(|job| !job.is_finished()).count()
    }

    /// Auto-prepare for a track whose analysis finished. Returns whether a job was queued. Does
    /// nothing unless [`auto_prepare_wanted`] holds, the model is ready and the track has no stems.
    pub fn on_analysed(
        &mut self,
        host: &mut dyn StemHost,
        library: &Library,
        track_id: i64,
        status: PrepStatus,
    ) -> SettingsResult<bool> {
        if !auto_prepare_wanted(host.auto_prepare(), self.watermark, track_id, status)
            || !host.model_state().ready()
        {
            return Ok(false);
        }
        let Some(track) = library.tracks().find_by_id(track_id).ok().flatten() else {
            return Ok(false);
        };
        let job = StemJob {
            track_id,
            path: PathBuf::from(&track.path),
            content_hash: track.content_hash.clone(),
        };
        Ok(self.prepare(host, library, &[job])? == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_prepare_needs_the_pref_a_finished_analysis_and_a_new_track() {
        // Off by default: nothing fires.
        assert!(!auto_prepare_wanted(false, 10, 11, PrepStatus::Done));
        // On, analysed, ingested after the library opened.
        assert!(auto_prepare_wanted(true, 10, 11, PrepStatus::Done));
        // Still analysing or failed: not yet.
        assert!(!auto_prepare_wanted(true, 10, 11, PrepStatus::Running));
        assert!(!auto_prepare_wanted(true, 10, 11, PrepStatus::Failed));
        // A track that was already in the library (loaded, not ingested) never qualifies.
        assert!(!auto_prepare_wanted(true, 10, 10, PrepStatus::Done));
        assert!(!auto_prepare_wanted(true, 10, 3, PrepStatus::Done));
    }

    #[test]
    fn model_state_lines_name_each_state() {
        assert_eq!(
            ModelState::NotInstalled(String::new()).line(),
            "Not installed"
        );
        assert!(ModelState::NotInstalled("no manifest".into())
            .line()
            .contains("no manifest"));
        assert!(ModelState::LicenceNotAccepted
            .line()
            .starts_with("Licence not accepted"));
        assert_eq!(ModelState::Downloading(0.424).line(), "Downloading 42%");
        assert_eq!(ModelState::Installed(1).line(), "Installed v1");
        assert!(ModelState::Failed("disk full".into())
            .line()
            .contains("disk full"));
        assert!(ModelState::Installed(1).ready());
        assert!(!ModelState::Downloading(0.5).ready());
    }

    /// Host with a model state and a worker that finishes each job at once.
    struct FakeHost {
        state: ModelState,
        auto: bool,
        submitted: Vec<StemJob>,
        pool: om_core::jobs::WorkerPool<()>,
    }

    impl FakeHost {
        fn new(state: ModelState) -> Self {
            Self {
                state,
                auto: true,
                submitted: Vec::new(),
                pool: om_core::jobs::WorkerPool::new(1, "fake-stems", |_, _| {}),
            }
        }
    }

    impl StemHost for FakeHost {
        fn model_state(&self) -> ModelState {
            self.state.clone()
        }
        fn licence(&self) -> Option<(String, String)> {
            None
        }
        fn accept_and_download(&mut self) -> SettingsResult<()> {
            Ok(())
        }
        fn cancel_download(&mut self) {}
        fn separator(&self) -> (String, i32) {
            ("htdemucs".into(), 1)
        }
        fn cache_bytes(&self) -> u64 {
            0
        }
        fn clear_cache(&mut self) -> SettingsResult<usize> {
            Ok(0)
        }
        fn set_max_cache_bytes(&mut self, _bytes: u64) {}
        fn auto_prepare(&self) -> bool {
            self.auto
        }
        fn set_auto_prepare(&mut self, on: bool) {
            self.auto = on;
        }
        fn submit(&mut self, job: StemJob) -> SettingsResult<JobHandle> {
            self.submitted.push(job);
            Ok(self.pool.submit(()))
        }
    }

    fn memory_library() -> Library {
        Library::open(
            ":memory:",
            om_library::OpenMode::ReadWrite,
            om_library::core_migrations(),
        )
        .expect("memory library")
    }

    fn add_track(library: &Library, path: &str) -> i64 {
        library
            .tracks()
            .insert(&om_library::TrackRecord {
                path: path.into(),
                content_hash: "ab".repeat(32),
                ..Default::default()
            })
            .expect("insert")
    }

    #[test]
    fn auto_prepare_fires_for_a_new_track_only_when_on_and_ready() {
        let library = memory_library();
        let old = add_track(&library, "/music/old.wav");
        let watermark = watermark_of(&library);
        assert_eq!(watermark, old);
        let fresh = add_track(&library, "/music/new.wav");
        let mut queue = StemQueue::new(watermark);

        // Off: an analysed new track is not queued.
        let mut host = FakeHost::new(ModelState::Installed(1));
        host.auto = false;
        assert!(!queue
            .on_analysed(&mut host, &library, fresh, PrepStatus::Done)
            .expect("ok"));
        assert!(host.submitted.is_empty());

        host.auto = true;
        // On, but the track was loaded from the library (not ingested): not queued.
        assert!(!queue
            .on_analysed(&mut host, &library, old, PrepStatus::Done)
            .expect("ok"));
        assert!(host.submitted.is_empty());

        // On, new track, analysis done: queued once with its path and hash.
        assert!(queue
            .on_analysed(&mut host, &library, fresh, PrepStatus::Done)
            .expect("ok"));
        assert_eq!(host.submitted.len(), 1);
        assert_eq!(host.submitted[0].path, PathBuf::from("/music/new.wav"));
    }

    #[test]
    fn auto_prepare_waits_for_the_model() {
        let library = memory_library();
        let watermark = watermark_of(&library);
        let fresh = add_track(&library, "/music/new.wav");
        let mut queue = StemQueue::new(watermark);
        let mut host = FakeHost::new(ModelState::LicenceNotAccepted);
        assert!(!queue
            .on_analysed(&mut host, &library, fresh, PrepStatus::Done)
            .expect("no error when the model is missing"));
        assert!(host.submitted.is_empty());
    }

    #[test]
    fn prepare_refuses_with_the_status_line_when_not_ready() {
        let library = memory_library();
        let mut queue = StemQueue::default();
        let mut host = FakeHost::new(ModelState::LicenceNotAccepted);
        let job = StemJob {
            track_id: 1,
            path: PathBuf::from("/music/a.wav"),
            content_hash: String::new(),
        };
        let error = queue
            .prepare(&mut host, &library, &[job])
            .expect_err("not ready");
        assert!(error.to_string().starts_with("Licence not accepted"));
        assert!(host.submitted.is_empty());
    }

    #[test]
    fn a_track_without_a_content_hash_says_to_wait_for_ingest() {
        let library = memory_library();
        let mut queue = StemQueue::default();
        let mut host = FakeHost::new(ModelState::Installed(1));
        let job = StemJob {
            track_id: 1,
            path: PathBuf::from("/music/a.wav"),
            content_hash: String::new(),
        };
        let error = queue
            .prepare(&mut host, &library, &[job])
            .expect_err("no hash yet");
        assert_eq!(
            error.to_string(),
            "track has no content hash yet — wait for ingest to finish"
        );
        assert!(host.submitted.is_empty(), "nothing is submitted for it");
    }

    #[test]
    fn labels_read_as_none_preparing_ready() {
        assert_eq!(StemLabel::None.text(), "None");
        assert_eq!(StemLabel::Preparing(42).text(), "Preparing 42%");
        assert_eq!(StemLabel::Ready.text(), "Ready");
    }
}
