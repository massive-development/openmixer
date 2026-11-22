//! The Tauri window's settings host: the same preference edits the GPUI overlay makes (the
//! `om-ui` settings pages over the shared output), saved and pushed into the subsystems as GPUI's
//! `apply_changes` does.

use crate::gui_tauri_devices::Devices;
use om_ui::preferences::Preferences;
use om_ui::settings::{
    stats_text, AppearanceSettingsPage, AudioSettingsModel, AudioSettingsPage, SettingsResult,
};
use om_ui::theme::DensityChoice;
use om_ui_tauri::{
    AppearanceChange, AppearancePage, AppearanceResult, AudioChange, AudioPage, DevicesHost,
    SettingsHost, ThemeChoice,
};
use openmixer::app_targets::AudioDeviceControl;
use openmixer::audio_settings::SharedOutput;
use openmixer::composition::AppComposition;
use std::cell::{RefCell, RefMut};
use std::path::PathBuf;
use std::rc::Rc;

/// The shared composition and where the preferences are saved. The settings pages share one
/// store, and every page edits the composition's own preferences, so a page never saves a section
/// another page changed since it last looked.
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

    /// Runs `edit` on the composition's preferences; when it says it changed something, they are
    /// saved and applied.
    pub fn edit_prefs(&self, edit: impl FnOnce(&mut Preferences) -> bool) {
        let mut composition = self.composition.borrow_mut();
        if edit(&mut composition.prefs) {
            self.save_and_apply(&mut composition);
        }
    }

    /// Saves the composition's preferences and pushes the changed sections into the subsystems.
    fn save_and_apply(&self, composition: &mut AppComposition) {
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
    mock: bool,
    decks: usize,
    devices: Devices,
}

impl TauriSettings {
    pub fn new(store: Rc<PrefsStore>, output: SharedOutput, mock: bool, decks: usize) -> Self {
        let devices = Devices::new(Rc::clone(&store));
        Self {
            store,
            output,
            mock,
            decks,
            devices,
        }
    }

    fn composition(&self) -> RefMut<'_, AppComposition> {
        self.store.composition().borrow_mut()
    }

    /// Runs `edit` on the composition's own preferences, the one copy every settings page edits, so
    /// a page never saves a section another page changed since it last looked. A change is saved
    /// and applied.
    fn edit(
        &self,
        edit: impl FnOnce(&mut AudioSettingsPage<'_>) -> SettingsResult,
    ) -> Result<(), String> {
        let mut composition = self.composition();
        let mut model = self.output.clone();
        let changed = edit(&mut AudioSettingsPage::new(
            &mut composition.prefs,
            &mut model,
        ))
        .map_err(|e| e.to_string())?;
        if changed {
            self.store.save_and_apply(&mut composition);
        }
        Ok(())
    }

    /// The appearance page over the composition's own preferences (see [`Self::edit`]).
    fn edit_appearance(
        &self,
        edit: impl FnOnce(&mut AppearanceSettingsPage<'_>) -> SettingsResult,
    ) -> Result<(), String> {
        let mut composition = self.composition();
        let changed = {
            let c = &mut *composition;
            let mut page = AppearanceSettingsPage::new(&mut c.prefs, &c.themes);
            edit(&mut page).map_err(|e| e.to_string())?
        };
        if changed {
            self.store.save_and_apply(&mut composition);
        }
        Ok(())
    }

    fn appearance_page(&self) -> AppearancePage {
        let composition = self.store.composition().borrow();
        let mut prefs = composition.prefs.clone();
        let theme = prefs.theme.clone();
        let page = AppearanceSettingsPage::new(&mut prefs, &composition.themes);
        AppearancePage {
            themes: page
                .choices()
                .into_iter()
                .map(|(id, name)| ThemeChoice { id, name })
                .collect(),
            theme,
            density: density_name(page.density()).to_owned(),
        }
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

fn density_name(choice: DensityChoice) -> &'static str {
    match choice {
        DensityChoice::Automatic => "automatic",
        DensityChoice::Roomy => "roomy",
        DensityChoice::Compact => "compact",
    }
}

fn density_of(name: &str) -> Result<DensityChoice, String> {
    DensityChoice::ALL
        .into_iter()
        .find(|c| density_name(*c) == name)
        .ok_or_else(|| format!("unknown density '{name}'"))
}

impl SettingsHost for TauriSettings {
    fn appearance(&self) -> AppearancePage {
        self.appearance_page()
    }

    fn change_appearance(&self, change: AppearanceChange) -> Result<AppearanceResult, String> {
        match change {
            AppearanceChange::Theme(id) => self.edit_appearance(|page| page.pick(&id))?,
            AppearanceChange::Density(name) => {
                let choice = density_of(&name)?;
                self.edit_appearance(|page| Ok(page.pick_density(choice)))?;
            }
        }
        let theme = crate::gui_tauri::shell_theme(&self.store.composition().borrow(), self.decks);
        Ok(AppearanceResult {
            page: self.appearance_page(),
            css: theme.css,
            strip_inks: theme.strip_inks,
        })
    }

    fn devices(&self) -> Option<&dyn DevicesHost> {
        Some(&self.devices)
    }

    fn audio(&self) -> AudioPage {
        self.page()
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
