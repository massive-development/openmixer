//! The Tauri window's settings host: the same preference edits the GPUI overlay makes (the
//! `om-ui` settings pages over the shared output), saved and pushed into the subsystems as GPUI's
//! `apply_changes` does.

use om_ui::settings::{
    stats_text, AppearanceSettingsPage, AudioSettingsModel, AudioSettingsPage, SettingsResult,
    StemsSettingsPage,
};
use om_ui::theme::DensityChoice;
use om_ui_tauri::{
    AppearanceChange, AppearancePage, AppearanceResult, AudioChange, AudioPage, SettingsHost,
    StemsPrefsChange, StemsPrefsPage, ThemeChoice,
};
use openmixer::app_targets::AudioDeviceControl;
use openmixer::audio_settings::SharedOutput;
use openmixer::composition::AppComposition;
use std::cell::{RefCell, RefMut};
use std::path::PathBuf;

pub struct TauriSettings {
    composition: RefCell<AppComposition>,
    output: SharedOutput,
    prefs_file: PathBuf,
    mock: bool,
    decks: usize,
}

impl TauriSettings {
    pub fn new(
        composition: AppComposition,
        output: SharedOutput,
        prefs_file: PathBuf,
        mock: bool,
        decks: usize,
    ) -> Self {
        Self {
            composition: RefCell::new(composition),
            output,
            prefs_file,
            mock,
            decks,
        }
    }

    fn composition(&self) -> RefMut<'_, AppComposition> {
        self.composition.borrow_mut()
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
            self.save_and_apply(&mut composition);
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
            self.save_and_apply(&mut composition);
        }
        Ok(())
    }

    fn appearance_page(&self) -> AppearancePage {
        let composition = self.composition.borrow();
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

    fn page(&self) -> AudioPage {
        let mut prefs = self.composition.borrow().prefs.clone();
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
        let theme = crate::gui_tauri::shell_theme(&self.composition.borrow(), self.decks);
        Ok(AppearanceResult {
            page: self.appearance_page(),
            css: theme.css,
            strip_inks: theme.strip_inks,
        })
    }

    fn stems_prefs(&self) -> StemsPrefsPage {
        let stems = &self.composition.borrow().prefs.stems;
        StemsPrefsPage {
            max_cache_gb: stems.max_cache_gb,
            max_cache_limit: StemsSettingsPage::MAX_CACHE_GB,
            auto_prepare: stems.auto_prepare,
            auto_load: stems.auto_load,
        }
    }

    fn change_stems_prefs(&self, change: StemsPrefsChange) -> Result<StemsPrefsPage, String> {
        let mut composition = self.composition();
        let changed = match change {
            StemsPrefsChange::MaxCacheGb(gb) => {
                let clamped = gb.clamp(1, StemsSettingsPage::MAX_CACHE_GB);
                std::mem::replace(&mut composition.prefs.stems.max_cache_gb, clamped) != clamped
            }
            StemsPrefsChange::AutoPrepare(on) => {
                std::mem::replace(&mut composition.prefs.stems.auto_prepare, on) != on
            }
            StemsPrefsChange::AutoLoad(on) => {
                std::mem::replace(&mut composition.prefs.stems.auto_load, on) != on
            }
        };
        if changed {
            self.save_and_apply(&mut composition);
        }
        drop(composition);
        Ok(self.stems_prefs())
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
