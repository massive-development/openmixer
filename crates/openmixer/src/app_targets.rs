//! Glue between `PreferencesApplier` and the real subsystems., covering
//! plugin loading (and forwarding plugin themes to the registry), theme selection, the audio device
//! state and output pair routing, MIDI inputs and the map file.
//!
//! Targets are built per call, so the state that
//! must outlive one call (the wanted theme, the plugin folder, the last audio state) is passed in
//! by the owner, or read back from the preferences (see [`crate::composition::AppComposition`]).
//! Every call is idempotent.

use crate::error::AppError;
use om_audio::OutputPairs;
use om_ui::preferences::{default_map_file, AudioPrefs, MidiPrefs};
use om_ui::preferences_applier::PreferencesTargets;
use om_ui::settings::MidiHost;
use om_ui::theme::{Source, ThemeRegistry};
use std::path::{Path, PathBuf};

/// Output routing the preferences drive. The device's `OutputPairs` implements it.
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

/// The open audio device, as the stored device state sees it.
pub trait AudioDeviceControl {
    /// The restorable state of the open device; empty when none is open.
    fn current_state(&self) -> String;
    /// Opens the device described by `state`. The error text is logged, not shown.
    fn initialise(&mut self, state: &str) -> Result<(), AppError>;
}

/// One theme a plugin provides, as JSON, with the plugin that owns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginTheme {
    pub json: String,
    pub owner: String,
}

/// What loading a plugin folder produced.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PluginLoad {
    pub loaded: usize,
    pub errors: Vec<String>,
    pub themes: Vec<PluginTheme>,
}

/// Plugin side. `om_plugins::PluginRegistry` implements it (see `crate::plugins`).
pub trait PluginSink {
    /// Unloads every plugin. Called before a folder is loaded.
    fn unload_all(&mut self);
    fn load_folder(&mut self, folder: &Path) -> PluginLoad;
}

/// What the targets remember between calls, for the lifetime of the app. The owner
/// keeps one and passes it to every [`AppPreferencesTargets`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AppTargetsState {
    /// Re-selected after a plugin reload, because removing plugin themes drops the selection.
    wanted_theme: String,
    /// The map last loaded; a map is read again only when its path changes.
    last_map: Option<PathBuf>,
    /// The folder last applied; [`AppPreferencesTargets::reload_plugins`] reloads it.
    plugin_folder: Option<PathBuf>,
    /// The device state last applied or requested, so an unchanged state is not re-initialised.
    last_audio_state: String,
}

/// The subsystems the preferences drive. `'d` is the lifetime of the open device, separate from
/// the registry's, so an `Option` of it can be passed on as it is.
pub struct AppPreferencesTargets<'a, 'd> {
    state: &'a mut AppTargetsState,
    themes: &'a mut ThemeRegistry,
    audio: &'a dyn AudioSink,
    device: Option<&'d mut (dyn AudioDeviceControl + 'd)>,
    midi: Option<&'a dyn MidiHost>,
    plugins: Option<&'a mut (dyn PluginSink + 'static)>,
}

impl<'a, 'd> AppPreferencesTargets<'a, 'd> {
    pub fn new(
        state: &'a mut AppTargetsState,
        themes: &'a mut ThemeRegistry,
        audio: &'a dyn AudioSink,
        device: Option<&'d mut (dyn AudioDeviceControl + 'd)>,
        midi: Option<&'a dyn MidiHost>,
        plugins: Option<&'a mut (dyn PluginSink + 'static)>,
    ) -> Self {
        Self {
            state,
            themes,
            audio,
            device,
            midi,
            plugins,
        }
    }

    /// Records a device state the app has just opened itself at start-up, so applying the same
    /// state later does not open the device a second time.
    pub fn note_audio_requested(&mut self, state: &str) {
        self.state.last_audio_state = state.to_owned();
    }

    /// Loads the folder applied last again (the Plugins tab's Reload button).
    pub fn reload_plugins(&mut self) {
        if let Some(folder) = self.state.plugin_folder.clone() {
            self.load_plugin_folder(&folder);
        }
    }

    /// The theme registry the targets drive.
    pub fn themes(&self) -> &ThemeRegistry {
        self.themes
    }

    /// Plugin folder work, shared by the preference apply and the reload.
    fn load_plugin_folder(&mut self, folder: &Path) {
        self.state.plugin_folder = Some(folder.to_owned());
        // Themes a plugin registered are removed first, so unloading drops exactly those.
        self.themes.remove_source(Source::Plugin);
        if let Some(plugins) = self.plugins.as_deref_mut() {
            plugins.unload_all();
            let loaded = plugins.load_folder(folder);
            for error in &loaded.errors {
                eprintln!("plugin: {error}");
            }
            for theme in &loaded.themes {
                if let Err(error) = self
                    .themes
                    .add_json(&theme.json, Source::Plugin, &theme.owner)
                {
                    eprintln!("plugin theme from {}: {error}", theme.owner);
                }
            }
        }
        // Removing the plugin themes may have dropped the selection to the default.
        if !self.state.wanted_theme.is_empty() {
            self.themes.select(&self.state.wanted_theme);
        }
    }
}

impl PreferencesTargets for AppPreferencesTargets<'_, '_> {
    fn apply_plugin_folder(&mut self, folder: &Path) {
        self.load_plugin_folder(folder);
    }

    fn apply_theme(&mut self, theme_id: &str) -> bool {
        self.state.wanted_theme = theme_id.to_owned();
        if self.themes.select(theme_id) {
            return true;
        }
        eprintln!("theme '{theme_id}' not found, keeping the current one");
        false
    }

    /// Pair routing always; the device is initialised only when the stored state is new and is
    /// not what the device already reports.
    fn apply_audio(&mut self, audio: &AudioPrefs) {
        self.audio.set_master_pair(audio.master_pair);
        self.audio.set_cue_pair(audio.cue_pair);
        // A device change made in the settings page already reached the device (its state equals
        // the stored one). Start-up opens the device itself (`note_audio_requested`).
        if audio.device_state.is_empty() || audio.device_state == self.state.last_audio_state {
            return;
        }
        self.state.last_audio_state.clone_from(&audio.device_state);
        let Some(device) = self.device.as_deref_mut() else {
            return;
        };
        if device.current_state() == audio.device_state {
            return;
        }
        if let Err(error) = device.initialise(&audio.device_state) {
            eprintln!("audio: {error}");
        }
    }

    /// The map is loaded before inputs are (re)enabled, so no message meets a stale map. A map is
    /// read only when its path changes.
    fn apply_midi(&mut self, midi: &MidiPrefs) {
        let file = if midi.map_file.is_empty() {
            default_map_file()
        } else {
            PathBuf::from(&midi.map_file)
        };
        if self.state.last_map.as_ref() != Some(&file) && file.is_file() {
            self.state.last_map = Some(file.clone());
            if let Some(sink) = self.midi {
                if let Err(error) = sink.load_map(&file) {
                    eprintln!("MIDI map {}: {error}", file.display());
                }
            }
        }
        if let Some(sink) = self.midi {
            sink.enable_inputs(midi.enable_all, &midi.enabled_inputs);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_ui::preferences::Preferences;
    use om_ui::preferences_applier::PreferencesApplier;
    use om_ui::settings::SettingsError;
    use std::cell::RefCell;

    /// The example plugin's theme id and owner, as the example plugin reports them.
    const EXAMPLE_THEME: &str = "org.openmixer.example.theme";
    const EXAMPLE_OWNER: &str = "org.openmixer.example.gain";

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

    /// Reports a fixed state and counts initialisations.
    struct FakeDevice {
        state: String,
        inits: usize,
    }

    impl AudioDeviceControl for FakeDevice {
        fn current_state(&self) -> String {
            self.state.clone()
        }
        fn initialise(&mut self, _state: &str) -> Result<(), AppError> {
            self.inits += 1;
            Ok(())
        }
    }

    /// Plugins: one folder holds the example plugin, every other folder is empty.
    struct FakePlugins {
        with_plugin: PathBuf,
        unloads: usize,
    }

    impl PluginSink for FakePlugins {
        fn unload_all(&mut self) {
            self.unloads += 1;
        }
        fn load_folder(&mut self, folder: &Path) -> PluginLoad {
            if folder != self.with_plugin {
                return PluginLoad::default();
            }
            PluginLoad {
                loaded: 1,
                errors: Vec::new(),
                themes: vec![PluginTheme {
                    json: format!(r#"{{"id":"{EXAMPLE_THEME}","name":"Example"}}"#),
                    owner: EXAMPLE_OWNER.to_owned(),
                }],
            }
        }
    }

    /// Records the order of MIDI calls.
    #[derive(Default)]
    struct RecordingMidi {
        log: RefCell<Vec<String>>,
    }

    impl MidiHost for RecordingMidi {
        fn load_map(&self, file: &Path) -> Result<(), SettingsError> {
            self.log
                .borrow_mut()
                .push(format!("load:{}", file.display()));
            Ok(())
        }
        fn enable_inputs(&self, enable_all: bool, inputs: &[String]) {
            self.log
                .borrow_mut()
                .push(format!("enable:{}:{}", enable_all, inputs.join("+")));
        }
        fn save_map(&self, _file: &Path) -> Result<(), SettingsError> {
            Ok(())
        }
        fn arm_learn(&self, _target: &str) {}
        fn cancel_learn(&self) {}
        fn is_learning(&self) -> bool {
            false
        }
    }

    /// A scratch directory under the system temp dir, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default();
            let dir = std::env::temp_dir()
                .join(format!("om-targets-{tag}-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("scratch dir");
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Registry with the built-ins and the default theme selected.
    fn registry() -> ThemeRegistry {
        let mut themes = ThemeRegistry::new();
        themes.load_builtins();
        themes.select(ThemeRegistry::DEFAULT_ID);
        themes
    }

    #[test]
    fn audio_pairs_reach_the_sink_and_theme_selection_follows_the_prefs() {
        let mut themes = ThemeRegistry::new();
        let mut state = AppTargetsState::default();
        themes.load_builtins();
        let audio = RecordingAudio::default();
        let prefs = Preferences::default();
        let mut applier = PreferencesApplier::new();
        {
            let mut targets =
                AppPreferencesTargets::new(&mut state, &mut themes, &audio, None, None, None);
            applier.apply(&mut targets, &prefs);
        }
        assert_eq!(*audio.calls.borrow(), vec!["master:0", "cue:1"]);
        assert_eq!(themes.active_id(), Some("openmixer.dark"));
    }

    #[test]
    fn an_unknown_theme_keeps_the_current_one() {
        let mut themes = ThemeRegistry::new();
        let mut state = AppTargetsState::default();
        themes.load_builtins();
        assert!(themes.select("openmixer.dark"));
        let audio = RecordingAudio::default();
        let mut targets =
            AppPreferencesTargets::new(&mut state, &mut themes, &audio, None, None, None);
        assert!(!targets.apply_theme("no.such.theme"));
        assert_eq!(themes.active_id(), Some("openmixer.dark"));
    }

    #[test]
    fn plugin_folder_loads_plugins_and_forwards_their_themes() {
        let folder = PathBuf::from("/plugins/with-example");
        let mut themes = registry();
        let mut state = AppTargetsState::default();
        let audio = RecordingAudio::default();
        let mut plugins = FakePlugins {
            with_plugin: folder.clone(),
            unloads: 0,
        };
        let mut targets = AppPreferencesTargets::new(
            &mut state,
            &mut themes,
            &audio,
            None,
            None,
            Some(&mut plugins),
        );
        targets.load_plugin_folder(&folder);
        assert_eq!(
            plugins.unloads, 1,
            "the plugins are unloaded before the folder loads"
        );
        let entry = themes.find(EXAMPLE_THEME).expect("plugin theme registered");
        assert_eq!(entry.source, Source::Plugin);
        assert_eq!(entry.owner, EXAMPLE_OWNER);
    }

    #[test]
    fn empty_folder_removes_plugin_themes_but_keeps_user_and_built_in_ones() {
        let mut themes = registry();
        let mut state = AppTargetsState::default();
        themes
            .add_json(
                r#"{"id":"user.mine","name":"Mine"}"#,
                Source::User,
                "mine.json",
            )
            .expect("user theme");
        let audio = RecordingAudio::default();
        let with_plugin = PathBuf::from("/plugins/with-example");
        let empty = PathBuf::from("/plugins/empty");
        let mut plugins = FakePlugins {
            with_plugin: with_plugin.clone(),
            unloads: 0,
        };
        {
            let mut targets = AppPreferencesTargets::new(
                &mut state,
                &mut themes,
                &audio,
                None,
                None,
                Some(&mut plugins),
            );
            targets.load_plugin_folder(&with_plugin);
            assert!(targets.themes().find(EXAMPLE_THEME).is_some());
            targets.load_plugin_folder(&empty);
        }
        assert!(themes.find(EXAMPLE_THEME).is_none());
        assert!(themes.find("user.mine").is_some());
        assert!(themes.find(ThemeRegistry::DEFAULT_ID).is_some());
    }

    #[test]
    fn active_plugin_theme_falls_back_when_its_plugin_goes_and_returns_on_reload() {
        let mut themes = registry();
        let mut state = AppTargetsState::default();
        let audio = RecordingAudio::default();
        let with_plugin = PathBuf::from("/plugins/with-example");
        let empty = PathBuf::from("/plugins/empty");
        let mut plugins = FakePlugins {
            with_plugin: with_plugin.clone(),
            unloads: 0,
        };
        {
            let mut targets = AppPreferencesTargets::new(
                &mut state,
                &mut themes,
                &audio,
                None,
                None,
                Some(&mut plugins),
            );
            targets.load_plugin_folder(&with_plugin);
            assert!(targets.apply_theme(EXAMPLE_THEME));
            assert_eq!(targets.themes().active_id(), Some(EXAMPLE_THEME));

            targets.load_plugin_folder(&empty);
            assert_eq!(
                targets.themes().active_id(),
                Some(ThemeRegistry::DEFAULT_ID),
                "the plugin theme vanished: fall back"
            );

            targets.load_plugin_folder(&with_plugin);
            assert_eq!(
                targets.themes().active_id(),
                Some(EXAMPLE_THEME),
                "the chosen theme is re-selected once it is back"
            );

            targets.reload_plugins();
            assert_eq!(targets.themes().active_id(), Some(EXAMPLE_THEME));
            assert!(!targets.apply_theme("no.such.theme"));
            assert_eq!(
                targets.themes().active_id(),
                Some(EXAMPLE_THEME),
                "an unknown id keeps the current theme"
            );
        }
    }

    #[test]
    fn session_theme_override_survives_a_plugin_reload() {
        let mut themes = registry();
        let mut state = AppTargetsState::default();
        let audio = RecordingAudio::default();
        let with_plugin = PathBuf::from("/plugins/with-example");
        let mut plugins = FakePlugins {
            with_plugin: with_plugin.clone(),
            unloads: 0,
        };
        {
            let mut targets = AppPreferencesTargets::new(
                &mut state,
                &mut themes,
                &audio,
                None,
                None,
                Some(&mut plugins),
            );
            targets.load_plugin_folder(&with_plugin);
            // The saved preference, then the session override applied after it.
            assert!(targets.apply_theme(ThemeRegistry::DEFAULT_ID));
            assert!(targets.apply_theme(EXAMPLE_THEME));
            targets.reload_plugins();
            assert_eq!(
                targets.themes().active_id(),
                Some(EXAMPLE_THEME),
                "the override wins over the saved theme after a reload"
            );
        }
    }

    #[test]
    fn audio_state_is_initialised_only_when_it_differs_from_the_device() {
        let mut themes = registry();
        let mut state = AppTargetsState::default();
        let audio = RecordingAudio::default();
        let mut device = FakeDevice {
            state: r#"{"device":"A"}"#.to_owned(),
            inits: 0,
        };
        let mut prefs = AudioPrefs {
            master_pair: 1,
            cue_pair: -1,
            device_state: device.state.clone(),
        };
        {
            let mut targets = AppPreferencesTargets::new(
                &mut state,
                &mut themes,
                &audio,
                Some(&mut device),
                None,
                None,
            );
            // What the UI stored after it changed the device itself.
            targets.apply_audio(&prefs);
            assert_eq!(*audio.calls.borrow(), vec!["master:1", "cue:-1"]);
        }
        assert_eq!(device.inits, 0);

        // A different state from elsewhere (a hand edit): applied once, then remembered.
        prefs.device_state = r#"{"device":"B"}"#.to_owned();
        {
            let mut targets = AppPreferencesTargets::new(
                &mut state,
                &mut themes,
                &audio,
                Some(&mut device),
                None,
                None,
            );
            targets.apply_audio(&prefs);
            targets.apply_audio(&prefs);
        }
        assert_eq!(device.inits, 1, "applied once, then remembered");
    }

    #[test]
    fn start_up_state_requested_by_the_app_is_not_initialised_again() {
        let mut themes = registry();
        let mut state = AppTargetsState::default();
        let audio = RecordingAudio::default();
        // The device reports something else than the saved state (the app opened it at start-up).
        let mut device = FakeDevice {
            state: r#"{"device":"other"}"#.to_owned(),
            inits: 0,
        };
        let prefs = AudioPrefs {
            master_pair: 0,
            cue_pair: 1,
            device_state: r#"{"saved":true}"#.to_owned(),
        };
        {
            let mut targets = AppPreferencesTargets::new(
                &mut state,
                &mut themes,
                &audio,
                Some(&mut device),
                None,
                None,
            );
            targets.note_audio_requested(&prefs.device_state);
            targets.apply_audio(&prefs);
        }
        assert_eq!(device.inits, 0);
    }

    #[test]
    fn midi_map_is_loaded_before_inputs_are_enabled_and_only_when_it_changes() {
        let scratch = Scratch::new("midi");
        let mut themes = registry();
        let mut state = AppTargetsState::default();
        let audio = RecordingAudio::default();
        let midi = RecordingMidi::default();
        let map = scratch.0.join("map.json");
        std::fs::write(
            &map,
            r#"{"mappings":[{"type":"cc","number":7,"target":"a"}]}"#,
        )
        .expect("write map");
        let prefs = MidiPrefs {
            enable_all: false,
            enabled_inputs: vec!["usb-1".to_owned()],
            map_file: map.to_string_lossy().into_owned(),
        };
        {
            let mut targets = AppPreferencesTargets::new(
                &mut state,
                &mut themes,
                &audio,
                None,
                Some(&midi),
                None,
            );
            targets.apply_midi(&prefs);
        }
        assert_eq!(
            midi.log.borrow().clone(),
            vec![
                format!("load:{}", map.display()),
                "enable:false:usb-1".to_owned()
            ],
            "the map loads first, then the inputs are enabled"
        );

        std::fs::write(&map, r#"{"mappings":[]}"#).expect("rewrite map");
        midi.log.borrow_mut().clear();
        {
            let mut targets = AppPreferencesTargets::new(
                &mut state,
                &mut themes,
                &audio,
                None,
                Some(&midi),
                None,
            );
            targets.apply_midi(&prefs);
        }
        assert_eq!(
            midi.log.borrow().clone(),
            vec!["enable:false:usb-1".to_owned()],
            "the same path is not reloaded"
        );
    }

    #[test]
    fn a_missing_map_file_is_not_loaded_but_inputs_still_follow_the_prefs() {
        let scratch = Scratch::new("midi-missing");
        let mut themes = registry();
        let mut state = AppTargetsState::default();
        let audio = RecordingAudio::default();
        let midi = RecordingMidi::default();
        let prefs = MidiPrefs {
            enable_all: true,
            enabled_inputs: Vec::new(),
            map_file: scratch.0.join("absent.json").to_string_lossy().into_owned(),
        };
        let mut targets =
            AppPreferencesTargets::new(&mut state, &mut themes, &audio, None, Some(&midi), None);
        targets.apply_midi(&prefs);
        assert_eq!(*midi.log.borrow(), vec!["enable:true:".to_owned()]);
    }
}
