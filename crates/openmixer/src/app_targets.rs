//! Glue between `PreferencesApplier` and the real subsystems. Port of `src/app/AppPreferencesTargets.cpp`:
//! plugin loading (and forwarding plugin themes to the registry), theme selection, output pair
//! routing, MIDI inputs and the map file. Every call is idempotent.
//!
//! The audio device-state restore (reopening the device from `AudioPrefs::device_state`) is not
//! ported yet: the device owner opens the device, and only the pair selection is applied here.

use om_audio::OutputPairs;
use om_ui::preferences::{default_map_file, AudioPrefs, MidiPrefs};
use om_ui::preferences_applier::PreferencesTargets;
use om_ui::theme::{Source, ThemeRegistry};
use std::path::{Path, PathBuf};

/// Output routing the preferences drive. The cpal device's `OutputPairs` implements it.
pub trait AudioSink {
    fn set_master_pair(&self, pair: i32);
    fn set_cue_pair(&self, pair: i32);
}

impl AudioSink for OutputPairs {
    fn set_master_pair(&self, pair: i32) {
        OutputPairs::set_master_pair(self, pair);
    }

    fn set_cue_pair(&self, pair: i32) {
        OutputPairs::set_cue_pair(self, pair);
    }
}

/// MIDI side (placeholder for `om-midi`, Todo 10). The map is loaded before inputs are changed.
pub trait MidiSink {
    fn load_map(&mut self, file: &Path) -> Result<(), String>;
    fn enable_inputs(&mut self, enable_all: bool, inputs: &[String]);
}

/// Plugin side (placeholder for `om-plugins`, Todo 14). Returns the per-plugin error messages.
pub trait PluginSink {
    fn load_folder(&mut self, folder: &Path) -> Vec<String>;
}

pub struct AppPreferencesTargets<'a> {
    themes: &'a mut ThemeRegistry,
    audio: &'a dyn AudioSink,
    midi: Option<&'a mut (dyn MidiSink + 'static)>,
    plugins: Option<&'a mut (dyn PluginSink + 'static)>,
    /// Re-selected after a plugin reload, because removing plugin themes drops the selection.
    wanted_theme: String,
    last_map: Option<PathBuf>,
}

impl<'a> AppPreferencesTargets<'a> {
    pub fn new(
        themes: &'a mut ThemeRegistry,
        audio: &'a dyn AudioSink,
        midi: Option<&'a mut (dyn MidiSink + 'static)>,
        plugins: Option<&'a mut (dyn PluginSink + 'static)>,
    ) -> Self {
        Self {
            themes,
            audio,
            midi,
            plugins,
            wanted_theme: String::new(),
            last_map: None,
        }
    }
}

impl PreferencesTargets for AppPreferencesTargets<'_> {
    /// Themes a plugin registered are removed first, so unloading drops exactly those.
    fn apply_plugin_folder(&mut self, folder: &Path) {
        self.themes.remove_source(Source::Plugin);
        if let Some(plugins) = self.plugins.as_deref_mut() {
            for error in plugins.load_folder(folder) {
                eprintln!("plugin: {error}");
            }
        }
        if !self.wanted_theme.is_empty() {
            self.themes.select(&self.wanted_theme);
        }
    }

    fn apply_theme(&mut self, theme_id: &str) -> bool {
        self.wanted_theme = theme_id.to_owned();
        if self.themes.select(theme_id) {
            return true;
        }
        eprintln!("theme '{theme_id}' not found, keeping the current one");
        false
    }

    fn apply_audio(&mut self, audio: &AudioPrefs) {
        self.audio.set_master_pair(audio.master_pair);
        self.audio.set_cue_pair(audio.cue_pair);
    }

    fn apply_midi(&mut self, midi: &MidiPrefs) {
        let file = if midi.map_file.is_empty() {
            default_map_file()
        } else {
            PathBuf::from(&midi.map_file)
        };
        if self.last_map.as_ref() != Some(&file) && file.is_file() {
            self.last_map = Some(file.clone());
            if let Some(sink) = self.midi.as_deref_mut() {
                if let Err(error) = sink.load_map(&file) {
                    eprintln!("MIDI map {}: {error}", file.display());
                }
            }
        }
        if let Some(sink) = self.midi.as_deref_mut() {
            sink.enable_inputs(midi.enable_all, &midi.enabled_inputs);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_ui::preferences::Preferences;
    use om_ui::preferences_applier::PreferencesApplier;
    use std::cell::RefCell;

    #[derive(Default)]
    struct RecordingAudio {
        calls: RefCell<Vec<String>>,
    }

    impl AudioSink for RecordingAudio {
        fn set_master_pair(&self, pair: i32) {
            self.calls.borrow_mut().push(format!("master:{pair}"));
        }
        fn set_cue_pair(&self, pair: i32) {
            self.calls.borrow_mut().push(format!("cue:{pair}"));
        }
    }

    #[test]
    fn audio_pairs_reach_the_sink_and_theme_selection_follows_the_prefs() {
        let mut themes = ThemeRegistry::new();
        themes.load_builtins();
        let audio = RecordingAudio::default();
        let prefs = Preferences::default();
        let mut applier = PreferencesApplier::new();
        {
            let mut targets = AppPreferencesTargets::new(&mut themes, &audio, None, None);
            applier.apply(&mut targets, &prefs);
        }
        assert_eq!(*audio.calls.borrow(), vec!["master:0", "cue:1"]);
        assert_eq!(themes.active_id(), Some("openmixer.dark"));
    }

    #[test]
    fn an_unknown_theme_keeps_the_current_one() {
        let mut themes = ThemeRegistry::new();
        themes.load_builtins();
        assert!(themes.select("openmixer.dark"));
        let audio = RecordingAudio::default();
        let mut targets = AppPreferencesTargets::new(&mut themes, &audio, None, None);
        assert!(!targets.apply_theme("no.such.theme"));
        drop(targets);
        assert_eq!(themes.active_id(), Some("openmixer.dark"));
    }
}
