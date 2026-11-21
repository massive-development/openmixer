//! The Tauri window's header host: the Automix strip and the Record button read the services
//! through it, and its presses go to the same ids the old GPUI header used (`automix.start | skip |
//! cancel`, `record.toggle`). The Off | On switch and the length chip are edits of the Automix
//! settings, so they go through the settings host (`gui_tauri_settings`), which owns the one copy
//! of the preferences.

use om_core::{AutomixSnapshot, ParameterRouter, ParameterTarget, RecordSnapshot};
use om_engine::DjEngine;
use om_ui_tauri::{AutomixAction, HeaderHost};
use openmixer::automix_service::AutomixShared;
use openmixer::record_service::RecordService;
use std::sync::Arc;

pub struct TauriHeader {
    engine: Arc<DjEngine>,
    router: Arc<ParameterRouter>,
    automix: Arc<AutomixShared>,
    record: Arc<RecordService>,
}

impl TauriHeader {
    pub fn new(
        engine: Arc<DjEngine>,
        router: Arc<ParameterRouter>,
        automix: Arc<AutomixShared>,
        record: Arc<RecordService>,
    ) -> Self {
        Self {
            engine,
            router,
            automix,
            record,
        }
    }
}

impl HeaderHost for TauriHeader {
    fn automix(&self) -> AutomixSnapshot {
        self.automix.snapshot()
    }

    fn record(&self) -> (RecordSnapshot, String) {
        (self.engine.snapshot().record, self.record.last_error())
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

    fn header() -> TauriHeader {
        let (engine, _audio) = DjEngine::new(2);
        let engine = Arc::new(engine);
        let router = Arc::new(ParameterRouter::new());
        let automix = AutomixShared::new();
        router.register_typed(Namespace::Automix, automix.control());
        let record = RecordService::new(engine.recorder_handle(), &Default::default());
        router.register_typed(Namespace::Record, record.clone());
        TauriHeader::new(engine, router, automix, record)
    }

    #[test]
    fn a_press_without_a_library_is_routed_and_harmless() {
        let header = header();
        for action in [
            AutomixAction::Start,
            AutomixAction::Skip,
            AutomixAction::Cancel,
        ] {
            header.press_automix(action);
        }
        assert!(!header.automix().enabled);
        assert!(!header.record().0.recording);
    }

    #[test]
    fn the_snapshots_follow_the_services() {
        let header = header();
        header.automix.control().set_enabled(true);
        header.automix.control().set_enabled(false);
        assert!(!header.automix().enabled);
        assert_eq!(header.record().1, "");
    }
}
