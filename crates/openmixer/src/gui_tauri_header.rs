//! The Tauri window's header host: the Automix strip and the Record button talk to the app through
//! it. A press goes to the same ids the old GPUI header used (`automix.start | skip | cancel`,
//! `record.toggle`); the Off | On switch and the length chip are edits of the Automix settings,
//! pushed into the running service and saved.

use om_core::{AutomixSnapshot, ParameterRouter, ParameterTarget, RecordSnapshot};
use om_engine::DjEngine;
use om_ui::preferences::{AutomixPrefs, Preferences};
use om_ui_tauri::{AutomixAction, HeaderHost};
use openmixer::automix_service::AutomixShared;
use openmixer::automix_settings::apply_automix_prefs;
use openmixer::record_service::RecordService;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

pub struct TauriHeader {
    engine: Arc<DjEngine>,
    router: Arc<ParameterRouter>,
    automix: Arc<AutomixShared>,
    record: Arc<RecordService>,
    /// The stored Automix settings as the header last changed them.
    prefs: Mutex<AutomixPrefs>,
    /// Where an edit is saved; `None` keeps the edit for this run only.
    prefs_file: Option<PathBuf>,
}

impl TauriHeader {
    pub fn new(
        engine: Arc<DjEngine>,
        router: Arc<ParameterRouter>,
        automix: Arc<AutomixShared>,
        record: Arc<RecordService>,
        prefs: AutomixPrefs,
        prefs_file: Option<PathBuf>,
    ) -> Self {
        Self {
            engine,
            router,
            automix,
            record,
            prefs: Mutex::new(prefs),
            prefs_file,
        }
    }

    fn prefs(&self) -> MutexGuard<'_, AutomixPrefs> {
        self.prefs.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Applies `edit` to the Automix settings, pushes them into the service and saves the file.
    fn edit(&self, edit: impl FnOnce(&mut AutomixPrefs)) {
        let mut prefs = self.prefs();
        let before = prefs.clone();
        edit(&mut prefs);
        if *prefs == before {
            return;
        }
        apply_automix_prefs(&self.automix, &prefs, Some(&before));
        self.save(&prefs);
    }

    /// Writes the Automix section into the preferences file, leaving the rest as it is. A file
    /// that cannot be read is left alone, as `apply_changes` does.
    fn save(&self, automix: &AutomixPrefs) {
        let Some(file) = &self.prefs_file else { return };
        match Preferences::load(file) {
            Ok(mut stored) => {
                stored.automix = automix.clone();
                if let Err(error) = stored.save(file) {
                    eprintln!("openmixer: could not save preferences: {error}");
                }
            }
            Err(error) => eprintln!(
                "openmixer: not saving preferences: {} could not be read ({error})",
                file.display()
            ),
        }
    }
}

impl HeaderHost for TauriHeader {
    fn automix(&self) -> (AutomixSnapshot, AutomixPrefs) {
        (self.automix.snapshot(), self.prefs().clone())
    }

    fn record(&self) -> (RecordSnapshot, String) {
        (self.engine.snapshot().record, self.record.last_error())
    }

    fn set_automix_enabled(&self, on: bool) {
        // The running flag belongs to the controller (it can turn itself off), so the press sets
        // it directly, as the Automix page's switch does; the stored choice follows.
        self.automix.control().set_enabled(on);
        self.edit(|p| p.enabled = on);
    }

    fn set_automix_length(&self, beats: i32) {
        self.edit(|p| p.length_beats = beats);
    }

    fn press_automix(&self, action: AutomixAction) {
        let _ = self.router.set_parameter(action.id(), 1.0);
    }

    fn toggle_record(&self) {
        let _ = self.router.set_parameter("record.toggle", 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_core::Namespace;

    fn header(file: Option<PathBuf>) -> TauriHeader {
        let (engine, _audio) = DjEngine::new(2);
        let engine = Arc::new(engine);
        let router = Arc::new(ParameterRouter::new());
        let automix = AutomixShared::new();
        router.register_typed(Namespace::Automix, automix.control());
        let record = RecordService::new(engine.recorder_handle(), &Default::default());
        router.register_typed(Namespace::Record, record.clone());
        TauriHeader::new(
            engine,
            router,
            automix,
            record,
            AutomixPrefs::default(),
            file,
        )
    }

    #[test]
    fn the_switch_turns_the_running_controller_on_and_off() {
        let header = header(None);
        assert!(!header.automix().0.enabled);
        header.set_automix_enabled(true);
        assert!(header.automix.control().is_enabled());
        assert!(header.automix().1.enabled);
        header.set_automix_enabled(false);
        assert!(!header.automix.control().is_enabled());
    }

    #[test]
    fn the_length_chip_is_saved_and_leaves_the_rest_of_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("preferences.json");
        let mut stored = Preferences::default();
        stored.record.stop_on_low_disk = !stored.record.stop_on_low_disk;
        let kept = stored.record.stop_on_low_disk;
        stored.save(&file).unwrap();
        let header = header(Some(file.clone()));
        header.set_automix_length(32);
        assert_eq!(header.automix().1.length_beats, 32);
        let saved = Preferences::load(&file).unwrap();
        assert_eq!(saved.automix.length_beats, 32);
        assert_eq!(
            saved.record.stop_on_low_disk, kept,
            "the other sections are kept"
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("preferences.json");
        std::fs::write(&file, "{ not json").unwrap();
        let header = header(Some(file.clone()));
        header.set_automix_length(8);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ not json");
        assert_eq!(header.automix().1.length_beats, 8, "kept for this run");
    }

    #[test]
    fn a_press_without_a_library_is_routed_and_harmless() {
        let header = header(None);
        header.press_automix(AutomixAction::Start);
        header.press_automix(AutomixAction::Skip);
        header.press_automix(AutomixAction::Cancel);
        assert!(!header.record().0.recording);
    }
}
