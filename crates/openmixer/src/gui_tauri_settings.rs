//! The Tauri window's settings host: the same preference edits the GPUI overlay makes (the
//! `om-ui` settings pages over the shared output), saved and pushed into the subsystems as GPUI's
//! `apply_changes` does.

use om_ui::preferences::RECORD_FORMATS;
use om_ui::preferences::{AutomixPrefs, MAX_BPM_GAP_PERCENT};
use om_ui::settings::{
    length_label, stats_text, AppearanceSettingsPage, AudioSettingsModel, AudioSettingsPage,
    AutomixSettingsPage, QueueChoice, RecordSettingsPage, SettingsResult, LENGTH_CHOICES,
    RECORD_FORMAT_LABELS, STYLE_LABELS,
};
use om_ui::theme::DensityChoice;
use om_ui_tauri::{
    AppearanceChange, AppearancePage, AppearanceResult, AudioChange, AudioPage, AutomixChange,
    AutomixPage, FormatChoice, LengthChoice, QueueSource, RecordChange, RecordPage, SettingsHost,
    ThemeChoice,
};
use openmixer::app_targets::AudioDeviceControl;
use openmixer::audio_settings::SharedOutput;
use openmixer::automix_settings::AppAutomixSettings;
use openmixer::composition::AppComposition;
use openmixer::record_service::{default_folder, AppRecordHost};
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

    fn audio(&self) -> AudioPage {
        self.page()
    }

    fn automix(&self) -> AutomixPage {
        automix_page(&self.composition.borrow().prefs.automix)
    }

    fn automix_prefs(&self) -> AutomixPrefs {
        self.composition.borrow().prefs.automix.clone()
    }

    fn change_automix(&self, change: AutomixChange) -> Result<AutomixPage, String> {
        let queue = match &change {
            AutomixChange::Queue(q) => Some(queue_choice(q)?),
            _ => None,
        };
        let mut composition = self.composition();
        // The control compares the settings it last applied with the edit, so it starts from what
        // the page held before this change.
        let mut control = AppAutomixSettings::new(
            composition.automix.clone(),
            None,
            composition.prefs.automix.clone(),
        );
        let changed = {
            let mut page = AutomixSettingsPage::new(&mut composition.prefs, &mut control);
            match change {
                AutomixChange::Enabled(on) => page.set_enabled(on),
                AutomixChange::Style(style) => page.set_style(style),
                AutomixChange::LengthBeats(beats) => page.set_length_beats(i64::from(beats)),
                AutomixChange::Queue(_) => {
                    page.set_queue(queue.unwrap_or(QueueChoice::WholeLibrary))
                }
                AutomixChange::Smart(on) => page.set_smart(on),
                AutomixChange::MinBpmGapPercent(p) => page.set_min_bpm_gap_percent(p),
                AutomixChange::AutostartAtEnd(on) => page.set_autostart_at_end(on),
            }
            .map_err(|e| e.to_string())?
        };
        if changed {
            self.save_and_apply(&mut composition);
        }
        Ok(automix_page(&composition.prefs.automix))
    }

    fn record(&self) -> RecordPage {
        record_page(&self.composition.borrow().prefs.record)
    }

    fn change_record(&self, change: RecordChange) -> Result<RecordPage, String> {
        let mut composition = self.composition();
        let mut host = AppRecordHost(composition.record.clone());
        let changed = {
            let mut page = RecordSettingsPage::new(&mut composition.prefs, &mut host);
            match change {
                RecordChange::Folder(folder) => page.set_folder(&folder),
                RecordChange::Format(format) => page.set_format(&format),
                RecordChange::StopOnLowDisk(on) => page.set_stop_on_low_disk(on),
            }
            .map_err(|e| e.to_string())?
        };
        if changed {
            self.save_and_apply(&mut composition);
        }
        Ok(record_page(&composition.prefs.record))
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

/// The Automix page's data for the stored settings.
fn automix_page(a: &AutomixPrefs) -> AutomixPage {
    let queue = if a.queue_search {
        QueueSource {
            kind: "search".to_owned(),
            playlist: 0,
        }
    } else if a.queue_playlist > 0 {
        QueueSource {
            kind: "playlist".to_owned(),
            playlist: a.queue_playlist,
        }
    } else {
        QueueSource {
            kind: "library".to_owned(),
            playlist: 0,
        }
    };
    AutomixPage {
        enabled: a.enabled,
        style: a.style,
        styles: STYLE_LABELS.iter().map(|s| (*s).to_owned()).collect(),
        length_beats: a.length_beats,
        lengths: LENGTH_CHOICES
            .iter()
            .map(|&beats| LengthChoice {
                beats,
                label: length_label(beats),
            })
            .collect(),
        queue,
        smart: a.smart,
        min_bpm_gap_percent: a.min_bpm_gap_percent,
        max_bpm_gap_percent: MAX_BPM_GAP_PERCENT as i32,
        autostart_at_end: a.autostart_at_end,
    }
}

/// The queue source an edit names.
fn queue_choice(q: &QueueSource) -> Result<QueueChoice, String> {
    match q.kind.as_str() {
        "library" => Ok(QueueChoice::WholeLibrary),
        "search" => Ok(QueueChoice::CurrentSearch),
        "playlist" if q.playlist > 0 => Ok(QueueChoice::Playlist(q.playlist)),
        "playlist" => Err("pick a playlist".to_owned()),
        other => Err(format!("unknown queue source '{other}'")),
    }
}

/// The Recording page's data for the stored settings.
fn record_page(r: &om_ui::preferences::RecordPrefs) -> RecordPage {
    RecordPage {
        folder: r.folder.clone(),
        default_folder: default_folder().to_string_lossy().into_owned(),
        format: r.format.clone(),
        formats: RECORD_FORMATS
            .iter()
            .zip(RECORD_FORMAT_LABELS)
            .map(|(id, label)| FormatChoice {
                id: (*id).to_owned(),
                label: label.to_owned(),
            })
            .collect(),
        stop_on_low_disk: r.stop_on_low_disk,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_queue_source_names_map_to_choices() {
        let q = |kind: &str, playlist| QueueSource {
            kind: kind.to_owned(),
            playlist,
        };
        assert_eq!(
            queue_choice(&q("library", 0)),
            Ok(QueueChoice::WholeLibrary)
        );
        assert_eq!(
            queue_choice(&q("search", 0)),
            Ok(QueueChoice::CurrentSearch)
        );
        assert_eq!(
            queue_choice(&q("playlist", 7)),
            Ok(QueueChoice::Playlist(7))
        );
        assert!(queue_choice(&q("playlist", 0)).is_err());
        assert!(queue_choice(&q("radio", 0)).is_err());
    }

    #[test]
    fn the_automix_page_reads_the_stored_queue_and_lengths() {
        let mut prefs = AutomixPrefs {
            queue_playlist: 4,
            ..AutomixPrefs::default()
        };
        let page = automix_page(&prefs);
        assert_eq!(
            (page.queue.kind.as_str(), page.queue.playlist),
            ("playlist", 4)
        );
        prefs.queue_search = true;
        assert_eq!(automix_page(&prefs).queue.kind, "search", "the search wins");
        assert_eq!(page.styles.len(), 6);
        assert_eq!(page.lengths[0].label, "Auto");
    }

    #[test]
    fn the_record_page_offers_every_stored_format_with_its_label() {
        let page = record_page(&om_ui::preferences::RecordPrefs::default());
        assert_eq!(page.formats.len(), RECORD_FORMATS.len());
        assert_eq!(page.formats[0].label, "16-bit");
        assert!(page.formats.iter().any(|f| f.id == page.format));
    }
}
