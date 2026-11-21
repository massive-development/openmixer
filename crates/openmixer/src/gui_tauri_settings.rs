//! The Tauri window's settings host: the same preference edits the GPUI overlay makes (the
//! `om-ui` settings pages over the shared output), saved and pushed into the subsystems as GPUI's
//! `apply_changes` does.

use crate::gui_tauri_devices::Devices;
use om_ui::preferences::Preferences;
use om_ui::settings::{stats_text, AudioSettingsModel, AudioSettingsPage, SettingsResult};
use om_ui_tauri::{AudioChange, AudioPage, DevicesHost, SettingsHost};
use openmixer::app_targets::AudioDeviceControl;
use openmixer::audio_settings::SharedOutput;
use openmixer::composition::AppComposition;
use std::cell::{RefCell, RefMut};
use std::path::PathBuf;
use std::rc::Rc;

/// The shared composition and where a changed preferences copy goes: kept, saved and pushed into
/// the subsystems. The settings pages share one store.
pub struct PrefsStore {
    composition: Rc<RefCell<AppComposition>>,
    output: SharedOutput,
    prefs_file: PathBuf,
}

impl PrefsStore {
    pub fn new(
        composition: Rc<RefCell<AppComposition>>,
        output: SharedOutput,
        prefs_file: PathBuf,
    ) -> Self {
        Self {
            composition,
            output,
            prefs_file,
        }
    }

    pub fn composition(&self) -> &Rc<RefCell<AppComposition>> {
        &self.composition
    }

    /// Keeps `prefs` as the preferences, saves the file and pushes the changed sections.
    pub fn commit(&self, prefs: Preferences) {
        let mut composition = self.composition.borrow_mut();
        self.keep(&mut composition, prefs);
    }

    fn keep(&self, composition: &mut AppComposition, prefs: Preferences) {
        composition.prefs = prefs;
        if !composition.prefs_replaceable {
            eprintln!(
                "openmixer: not saving preferences: {} could not be read, so it is left as it is",
                self.prefs_file.display()
            );
        } else if let Err(error) = composition.prefs.save(&self.prefs_file) {
            eprintln!("openmixer: could not save preferences: {error}");
        }
        let mut settings = self.output.lock();
        let pairs = settings.pairs();
        let device: &mut dyn AudioDeviceControl = &mut *settings;
        composition.apply_preferences(&pairs, Some(device), None);
    }
}

pub struct TauriSettings {
    store: Rc<PrefsStore>,
    output: SharedOutput,
    devices: Devices,
    mock: bool,
}

impl TauriSettings {
    pub fn new(store: Rc<PrefsStore>, output: SharedOutput, mock: bool) -> Self {
        let devices = Devices::new(Rc::clone(&store));
        Self {
            store,
            output,
            devices,
            mock,
        }
    }

    fn composition(&self) -> RefMut<'_, AppComposition> {
        self.store.composition().borrow_mut()
    }

    /// Runs `edit` on a copy of the preferences; a changed copy is kept, saved and applied.
    fn edit(
        &self,
        edit: impl FnOnce(&mut AudioSettingsPage<'_>) -> SettingsResult,
    ) -> Result<(), String> {
        let mut composition = self.composition();
        let mut prefs = composition.prefs.clone();
        let mut model = self.output.clone();
        let changed =
            edit(&mut AudioSettingsPage::new(&mut prefs, &mut model)).map_err(|e| e.to_string())?;
        if changed {
            self.store.keep(&mut composition, prefs);
        }
        Ok(())
    }

    fn page(&self) -> AudioPage {
        let mut prefs = self.store.composition().borrow().prefs.clone();
        let mut writer = self.output.clone();
        let page = AudioSettingsPage::new(&mut prefs, &mut writer);
        let model = self.output.clone();
        let stats = self.output.lock().stats();
        AudioPage {
            device_types: model.device_types(),
            device_type: model.device_type(),
            devices: model.output_devices(),
            device: model.output_device(),
            sample_rates: model.sample_rates(),
            sample_rate: model.sample_rate(),
            buffer_sizes: model.buffer_sizes(),
            buffer_size: model.buffer_size(),
            pairs: page.pair_labels(),
            master_pair: usize::try_from(page.audio_prefs().master_pair).unwrap_or(0),
            cue_pair: page.shown_cue_pair(),
            stats: stats_text(&stats, ""),
            mock: self.mock,
        }
    }
}

impl SettingsHost for TauriSettings {
    fn audio(&self) -> AudioPage {
        self.page()
    }

    fn devices(&self) -> Option<&dyn DevicesHost> {
        Some(&self.devices)
    }

    fn change_audio(&self, change: AudioChange) -> Result<AudioPage, String> {
        self.edit(|page| match change {
            AudioChange::DeviceType(t) => page.set_device_type(&t),
            AudioChange::Device(d) => page.set_output_device(&d),
            AudioChange::SampleRate(r) => page.set_sample_rate(r),
            AudioChange::BufferSize(b) => page.set_buffer_size(b),
            AudioChange::MasterPair(p) => page.set_master_pair(p),
            AudioChange::CuePair(p) => page.set_cue_pair(p),
        })?;
        Ok(self.page())
    }
}
