//! Mix recording in the app: the `record.*` target and the settings the Recording page edits.
//!
//! The recorder itself lives in the engine (`om_engine::recorder`); it owns a writer thread and the
//! files, so the app composition holds the settings and turns `record.start | stop | toggle` into
//! calls on it. A value of 0.5 or more triggers. Recordings are a user-visible file output, so tests
//! only write to temp dirs.

use om_core::param_id::RecordParam;
use om_core::{ParamId, ParamTarget};
use om_engine::recorder::{RecordFormat, RecordSettings, RecordState, RecorderControl};
use om_ui::preferences::RecordPrefs;
use om_ui::settings::RecordHost;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// `<music dir or home>/OpenMixer Recordings`.
pub fn default_folder() -> PathBuf {
    dirs::audio_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("OpenMixer Recordings")
}

/// The recorder settings the stored preferences describe.
pub fn settings_from_prefs(prefs: &RecordPrefs) -> RecordSettings {
    let folder = if prefs.folder.is_empty() {
        default_folder()
    } else {
        PathBuf::from(&prefs.folder)
    };
    let mut s = RecordSettings::new(folder);
    s.format = RecordFormat::parse(&prefs.format).unwrap_or_default();
    s.stop_on_low_disk = prefs.stop_on_low_disk;
    s
}

/// The `record.*` target plus the live settings.
pub struct RecordService {
    recorder: Arc<RecorderControl>,
    settings: Mutex<RecordSettings>,
    last_error: Mutex<String>,
}

impl RecordService {
    pub fn new(recorder: Arc<RecorderControl>, prefs: &RecordPrefs) -> Arc<Self> {
        Arc::new(Self {
            recorder,
            settings: Mutex::new(settings_from_prefs(prefs)),
            last_error: Mutex::new(String::new()),
        })
    }

    pub fn recorder(&self) -> &RecorderControl {
        &self.recorder
    }

    /// The last start or stop failure, for the header; empty when none.
    pub fn last_error(&self) -> String {
        lock(&self.last_error).clone()
    }

    pub fn set_prefs(&self, prefs: &RecordPrefs) {
        let mut live = lock(&self.settings);
        let fresh = settings_from_prefs(prefs);
        live.folder = fresh.folder;
        live.format = fresh.format;
        live.stop_on_low_disk = fresh.stop_on_low_disk;
    }

    pub fn start(&self) {
        let settings = lock(&self.settings).clone();
        match self.recorder.start(&settings) {
            Ok(()) => lock(&self.last_error).clear(),
            Err(error) => *lock(&self.last_error) = error.to_string(),
        }
    }

    pub fn stop(&self) {
        match self.recorder.stop() {
            Ok(summary) => *lock(&self.last_error) = summary.error.unwrap_or_default(),
            Err(error) => *lock(&self.last_error) = error.to_string(),
        }
    }

    pub fn toggle(&self) {
        // Error means the writer stopped itself; a toggle collects it rather than starting anew.
        match self.recorder.state() {
            RecordState::Idle => self.start(),
            RecordState::Recording | RecordState::Error => self.stop(),
        }
    }
}

impl ParamTarget for RecordService {
    fn set_param(&self, id: &ParamId, value: f64) -> bool {
        let ParamId::Record(p) = id else {
            return false;
        };
        if value >= 0.5 {
            match p {
                RecordParam::Start => self.start(),
                RecordParam::Stop => self.stop(),
                RecordParam::Toggle => self.toggle(),
            }
        }
        true
    }
}

/// Lets the Recording page push edits into the service.
pub struct AppRecordHost(pub Arc<RecordService>);

impl RecordHost for AppRecordHost {
    fn apply(&mut self, prefs: &RecordPrefs) {
        self.0.set_prefs(prefs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_core::{ParameterRouter, ParameterTarget};
    use om_engine::DjEngine;

    #[test]
    fn ids_start_toggle_and_stop_a_recording_in_a_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, mut audio) = DjEngine::new(2);
        om_engine::AudioCallback::prepare(&mut audio, 48_000.0, 256);
        let prefs = RecordPrefs {
            folder: dir.path().join("out").to_string_lossy().into_owned(),
            ..RecordPrefs::default()
        };
        let service = RecordService::new(engine.recorder_handle(), &prefs);
        let router = ParameterRouter::new();
        router.register_typed(om_core::Namespace::Record, service.clone());

        assert!(router.set_parameter("record.start", 0.2), "routed");
        assert_eq!(engine.recorder().state(), RecordState::Idle, "below 0.5");
        assert!(router.set_parameter("record.start", 1.0));
        assert_eq!(engine.recorder().state(), RecordState::Recording);
        assert!(engine.snapshot().record.recording);
        assert!(
            router.set_parameter("record.start", 1.0),
            "second start is harmless"
        );
        assert!(router.set_parameter("record.toggle", 1.0));
        assert_eq!(engine.recorder().state(), RecordState::Idle);
        assert!(router.set_parameter("record.toggle", 1.0));
        assert!(router.set_parameter("record.stop", 1.0));
        assert!(
            router.set_parameter("record.stop", 1.0),
            "stop when idle is harmless"
        );
        assert_eq!(engine.recorder().state(), RecordState::Idle);
        assert_eq!(
            std::fs::read_dir(dir.path().join("out")).unwrap().count(),
            2
        );
    }

    #[test]
    fn hostile_values_do_not_start_anything() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, mut audio) = DjEngine::new(2);
        om_engine::AudioCallback::prepare(&mut audio, 48_000.0, 256);
        let prefs = RecordPrefs {
            folder: dir.path().to_string_lossy().into_owned(),
            ..RecordPrefs::default()
        };
        let service = RecordService::new(engine.recorder_handle(), &prefs);
        for v in [f64::NAN, -1.0, 0.0, 0.49] {
            service.set_param(&ParamId::Record(RecordParam::Start), v);
            service.set_param(&ParamId::Record(RecordParam::Toggle), v);
        }
        assert_eq!(engine.recorder().state(), RecordState::Idle);
        // Not a record id: not handled.
        assert!(!service.set_param(&"mixer.master".parse().unwrap(), 1.0));
    }

    #[test]
    fn an_unwritable_folder_reports_and_stays_idle() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f");
        std::fs::write(&file, b"x").unwrap();
        let (engine, mut audio) = DjEngine::new(2);
        om_engine::AudioCallback::prepare(&mut audio, 48_000.0, 256);
        let prefs = RecordPrefs {
            folder: file.join("sub").to_string_lossy().into_owned(),
            ..RecordPrefs::default()
        };
        let service = RecordService::new(engine.recorder_handle(), &prefs);
        service.start();
        assert_eq!(engine.recorder().state(), RecordState::Idle);
        assert!(!service.last_error().is_empty());
    }
}
