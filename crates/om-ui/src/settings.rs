//! Settings model: the logic behind the settings overlay (Todo 29). Port of the non-drawing parts of
//! `src/ui/SettingsPanel.cpp`, `SettingsPage.cpp`, `AudioSettingsPage.cpp`, `MidiSettingsPage.cpp`,
//! `LibrarySettingsPage.cpp`, `AppearanceSettingsPage.cpp` and `PluginsSettingsPage.cpp`.
//!
//! Pages edit the shared [`Preferences`] and report whether an edit changed anything; the caller
//! saves and applies (see `openmixer::composition`). The MIDI, library and plugin pages talk to
//! placeholder traits until `om-midi`, `om-library` and `om-plugins` exist; the app supplies the
//! real implementations later. Drawing lives in `om-ui-gpui`.

use crate::preferences::{AudioPrefs, Preferences};
use crate::theme::ThemeRegistry;
use std::path::PathBuf;

/// Error text from a setter, shown to the user.
pub type SettingsResult<T = bool> = Result<T, String>;

/// What the audio page needs from the audio layer. `Ok(())` / `Err(message)`, like the C++ setters.
pub trait AudioSettingsModel {
    fn device_types(&self) -> Vec<String>;
    fn device_type(&self) -> String;
    fn set_device_type(&mut self, device_type: &str) -> SettingsResult<()>;

    fn output_devices(&self) -> Vec<String>;
    fn output_device(&self) -> String;
    fn set_output_device(&mut self, name: &str) -> SettingsResult<()>;

    fn sample_rates(&self) -> Vec<u32>;
    fn sample_rate(&self) -> u32;
    fn set_sample_rate(&mut self, rate: u32) -> SettingsResult<()>;

    fn buffer_sizes(&self) -> Vec<u32>;
    fn buffer_size(&self) -> u32;
    fn set_buffer_size(&mut self, samples: u32) -> SettingsResult<()>;

    /// Channels of the open device; 0 when none is open.
    fn output_channels(&self) -> usize;
    /// Restorable device state, stored in `AudioPrefs::device_state`.
    fn state_string(&self) -> String;
}

/// Latency and load readout shown under the audio controls.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioStats {
    pub sample_rate: f64,
    pub buffer_size: usize,
    pub output_latency_ms: f64,
    pub round_trip_latency_ms: f64,
    pub cpu_load: f64,
    pub max_callback_us: f64,
    pub budget_us: f64,
    pub xrun_count: u64,
}

/// The readout under the audio controls (C++ `AudioSettingsPage::refreshStats`). With no device
/// open it shows the last setup error, or "No audio device running".
pub fn stats_text(stats: &AudioStats, setup_error: &str) -> String {
    if stats.sample_rate <= 0.0 {
        return if setup_error.is_empty() {
            "No audio device running".to_owned()
        } else {
            setup_error.to_owned()
        };
    }
    format!(
        "Output {:.1} ms, round trip {:.1} ms at {} Hz / {} samples\nCPU {:.0}% (peak {:.2} ms of {:.2} ms), xruns {}",
        stats.output_latency_ms,
        stats.round_trip_latency_ms,
        stats.sample_rate.round() as i64,
        stats.buffer_size,
        stats.cpu_load * 100.0,
        stats.max_callback_us / 1000.0,
        stats.budget_us / 1000.0,
        stats.xrun_count,
    )
}

/// The audio page: device choices from the model, output pair choices from the stored preferences.
pub struct AudioSettingsPage<'a> {
    prefs: &'a mut Preferences,
    model: &'a mut dyn AudioSettingsModel,
}

impl<'a> AudioSettingsPage<'a> {
    pub fn new(prefs: &'a mut Preferences, model: &'a mut dyn AudioSettingsModel) -> Self {
        Self { prefs, model }
    }

    /// Output pairs of the open device: `channels / 2` pairs, labelled "Out 1+2", "Out 3+4"...
    pub fn pair_labels(&self) -> Vec<String> {
        (0..self.model.output_channels() / 2)
            .map(|p| format!("Out {}+{}", 2 * p + 1, 2 * p + 2))
            .collect()
    }

    pub fn set_device_type(&mut self, device_type: &str) -> SettingsResult {
        self.model.set_device_type(device_type)?;
        self.store_state()
    }

    pub fn set_output_device(&mut self, name: &str) -> SettingsResult {
        self.model.set_output_device(name)?;
        self.store_state()
    }

    pub fn set_sample_rate(&mut self, rate: u32) -> SettingsResult {
        self.model.set_sample_rate(rate)?;
        self.store_state()
    }

    pub fn set_buffer_size(&mut self, samples: u32) -> SettingsResult {
        self.model.set_buffer_size(samples)?;
        self.store_state()
    }

    /// `pair` indexes [`Self::pair_labels`]; the master pair is always a real pair.
    pub fn set_master_pair(&mut self, pair: usize) -> SettingsResult {
        let pair = i32::try_from(pair).map_err(|e| e.to_string())?;
        Ok(set_if_changed(&mut self.prefs.audio.master_pair, pair))
    }

    /// `None` means no cue output (stored as -1).
    pub fn set_cue_pair(&mut self, pair: Option<usize>) -> SettingsResult {
        let stored = match pair {
            Some(p) => i32::try_from(p).map_err(|e| e.to_string())?,
            None => -1,
        };
        Ok(set_if_changed(&mut self.prefs.audio.cue_pair, stored))
    }

    /// The cue choice to show: the stored pair when it exists on this device, else "None".
    /// Viewing never rewrites the stored value.
    pub fn shown_cue_pair(&self) -> Option<usize> {
        let stored = self.prefs.audio.cue_pair;
        let count = self.pair_labels().len();
        usize::try_from(stored).ok().filter(|p| *p < count)
    }

    fn store_state(&mut self) -> SettingsResult {
        let state = self.model.state_string();
        Ok(set_if_changed(&mut self.prefs.audio.device_state, state))
    }

    /// The stored audio section, for the applier.
    pub fn audio_prefs(&self) -> &AudioPrefs {
        &self.prefs.audio
    }
}

/// Assigns and reports whether the value changed.
fn set_if_changed<T: PartialEq>(slot: &mut T, value: T) -> bool {
    if *slot == value {
        false
    } else {
        *slot = value;
        true
    }
}

/// What the MIDI page needs: the input devices the host can see.
pub trait MidiInputs {
    fn input_names(&self) -> Vec<String>;
}

/// MIDI page logic over the preferences. Placeholder for `om-midi` (Todo 10).
pub struct MidiSettingsPage<'a> {
    prefs: &'a mut Preferences,
    inputs: &'a dyn MidiInputs,
}

impl<'a> MidiSettingsPage<'a> {
    pub fn new(prefs: &'a mut Preferences, inputs: &'a dyn MidiInputs) -> Self {
        Self { prefs, inputs }
    }

    pub fn set_enable_all(&mut self, all: bool) -> bool {
        set_if_changed(&mut self.prefs.midi.enable_all, all)
    }

    /// Adds or removes one input from the explicit selection (used when "all" is off).
    pub fn set_input_enabled(&mut self, name: &str, on: bool) -> bool {
        let list = &mut self.prefs.midi.enabled_inputs;
        let present = list.iter().any(|n| n == name);
        match (on, present) {
            (true, false) => {
                list.push(name.to_owned());
                true
            }
            (false, true) => {
                list.retain(|n| n != name);
                true
            }
            _ => false,
        }
    }

    /// `None` resets to the default map file.
    pub fn set_map_file(&mut self, file: Option<PathBuf>) -> bool {
        let text = file
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        set_if_changed(&mut self.prefs.midi.map_file, text)
    }

    pub fn input_names(&self) -> Vec<String> {
        self.inputs.input_names()
    }
}

/// What the library page needs. Placeholder for `om-library` (Todo 12).
pub trait LibraryControl {
    fn available(&self) -> bool;
    fn add_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()>;
    fn remove_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()>;
    fn rescan(&mut self) -> SettingsResult<()>;
}

/// Library page logic. Interval and scan-on-start are read when the library starts.
pub struct LibrarySettingsPage<'a> {
    prefs: &'a mut Preferences,
    library: &'a mut dyn LibraryControl,
}

impl<'a> LibrarySettingsPage<'a> {
    pub const MAX_INTERVAL_MINUTES: i32 = 24 * 60;

    pub fn new(prefs: &'a mut Preferences, library: &'a mut dyn LibraryControl) -> Self {
        Self { prefs, library }
    }

    pub fn set_scan_on_start(&mut self, on: bool) -> bool {
        set_if_changed(&mut self.prefs.library.scan_on_start, on)
    }

    /// Clamped to `1..=MAX_INTERVAL_MINUTES`, as the preferences reader does.
    pub fn set_scan_interval(&mut self, minutes: i32) -> bool {
        let clamped = minutes.clamp(1, Self::MAX_INTERVAL_MINUTES);
        set_if_changed(&mut self.prefs.library.scan_interval_minutes, clamped)
    }

    pub fn add_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
        self.require_available()?;
        self.library.add_watch_folder(folder)
    }

    pub fn remove_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
        self.require_available()?;
        self.library.remove_watch_folder(folder)
    }

    pub fn rescan(&mut self) -> SettingsResult<()> {
        self.require_available()?;
        self.library.rescan()
    }

    fn require_available(&self) -> SettingsResult<()> {
        if self.library.available() {
            Ok(())
        } else {
            Err("The library is not available".to_owned())
        }
    }
}

/// Appearance page: lists the registry's themes and stores the chosen id.
pub struct AppearanceSettingsPage<'a> {
    prefs: &'a mut Preferences,
    themes: &'a ThemeRegistry,
}

impl<'a> AppearanceSettingsPage<'a> {
    pub fn new(prefs: &'a mut Preferences, themes: &'a ThemeRegistry) -> Self {
        Self { prefs, themes }
    }

    /// `(id, display name)` for every registered theme.
    pub fn choices(&self) -> Vec<(String, String)> {
        self.themes
            .themes()
            .into_iter()
            .map(|e| (e.info.id.clone(), e.info.name.clone()))
            .collect()
    }

    /// Stores the id; `false` when it was already stored. Unknown ids are refused.
    pub fn pick(&mut self, id: &str) -> SettingsResult {
        if self.themes.find(id).is_none() {
            return Err(format!("unknown theme '{id}'"));
        }
        Ok(set_if_changed(&mut self.prefs.theme, id.to_owned()))
    }
}

/// What the plugins page needs. Placeholder for `om-plugins` (Todo 14).
pub trait PluginHost {
    fn loaded_plugins(&self) -> Vec<String>;
    fn reload(&mut self) -> SettingsResult<()>;
}

/// Plugins page: shows the folder in use and the loaded plugins, and reloads on request.
pub struct PluginsSettingsPage<'a> {
    prefs: &'a Preferences,
    host: &'a mut dyn PluginHost,
}

impl<'a> PluginsSettingsPage<'a> {
    pub fn new(prefs: &'a Preferences, host: &'a mut dyn PluginHost) -> Self {
        Self { prefs, host }
    }

    pub fn folder(&self) -> PathBuf {
        self.prefs.effective_plugin_folder()
    }

    pub fn loaded(&self) -> Vec<String> {
        self.host.loaded_plugins()
    }

    pub fn reload(&mut self) -> SettingsResult<()> {
        self.host.reload()
    }
}

/// The standard tabs, in order. Audio is absent when the app has no audio model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    Audio,
    Midi,
    Library,
    Appearance,
    Plugins,
}

impl PageKind {
    pub fn title(self) -> &'static str {
        match self {
            PageKind::Audio => "Audio",
            PageKind::Midi => "MIDI",
            PageKind::Library => "Library",
            PageKind::Appearance => "Appearance",
            PageKind::Plugins => "Plugins",
        }
    }
}

/// Tab bookkeeping for the overlay: which tabs exist, which one is shown, and what Escape and the
/// Close button do. Pages added by plugins are kept by title after the standard ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsPanel {
    tabs: Vec<String>,
    standard: Vec<PageKind>,
    current: usize,
}

impl SettingsPanel {
    pub fn new(has_audio: bool) -> Self {
        let standard: Vec<PageKind> = [
            PageKind::Audio,
            PageKind::Midi,
            PageKind::Library,
            PageKind::Appearance,
            PageKind::Plugins,
        ]
        .into_iter()
        .filter(|k| has_audio || *k != PageKind::Audio)
        .collect();
        Self {
            tabs: standard.iter().map(|k| k.title().to_owned()).collect(),
            standard,
            current: 0,
        }
    }

    pub fn tabs(&self) -> &[String] {
        &self.tabs
    }

    pub fn current(&self) -> usize {
        self.current
    }

    /// The standard page behind tab `index`; `None` for a plugin-added tab.
    pub fn page_kind(&self, index: usize) -> Option<PageKind> {
        self.standard.get(index).copied()
    }

    /// Shows one tab at a time. Out-of-range indices are ignored.
    pub fn show(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.current = index;
        }
    }

    /// Adds a plugin page as a new tab and returns its index.
    pub fn add_page(&mut self, title: &str) -> usize {
        self.tabs.push(title.to_owned());
        self.tabs.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeRegistry;

    #[derive(Default)]
    struct FakeAudio {
        rates: Vec<u32>,
        rate: u32,
        buffer: u32,
        channels: usize,
        device: String,
        device_type: String,
        calls: Vec<String>,
    }

    impl FakeAudio {
        fn new() -> Self {
            Self {
                rates: vec![44_100, 48_000, 96_000],
                rate: 48_000,
                buffer: 256,
                channels: 4,
                device: "Built-in".into(),
                device_type: "ALSA".into(),
                calls: Vec::new(),
            }
        }
    }

    impl AudioSettingsModel for FakeAudio {
        fn device_types(&self) -> Vec<String> {
            vec!["ALSA".into(), "JACK".into()]
        }
        fn device_type(&self) -> String {
            self.device_type.clone()
        }
        fn set_device_type(&mut self, t: &str) -> SettingsResult<()> {
            self.calls.push(format!("type:{t}"));
            self.device_type = t.into();
            Ok(())
        }
        fn output_devices(&self) -> Vec<String> {
            vec!["Built-in".into(), "USB DJ Interface".into()]
        }
        fn output_device(&self) -> String {
            self.device.clone()
        }
        fn set_output_device(&mut self, name: &str) -> SettingsResult<()> {
            self.calls.push(format!("device:{name}"));
            self.device = name.into();
            Ok(())
        }
        fn sample_rates(&self) -> Vec<u32> {
            self.rates.clone()
        }
        fn sample_rate(&self) -> u32 {
            self.rate
        }
        fn set_sample_rate(&mut self, rate: u32) -> SettingsResult<()> {
            self.calls.push(format!("rate:{rate}"));
            self.rate = rate;
            Ok(())
        }
        fn buffer_sizes(&self) -> Vec<u32> {
            vec![128, 256, 512]
        }
        fn buffer_size(&self) -> u32 {
            self.buffer
        }
        fn set_buffer_size(&mut self, samples: u32) -> SettingsResult<()> {
            self.calls.push(format!("buffer:{samples}"));
            self.buffer = samples;
            Ok(())
        }
        fn output_channels(&self) -> usize {
            self.channels
        }
        fn state_string(&self) -> String {
            format!(
                "<DEVICESETUP deviceType=\"{}\" audioOutputDeviceName=\"{}\" sampleRate=\"{}\" bufferSize=\"{}\"/>",
                self.device_type, self.device, self.rate, self.buffer
            )
        }
    }

    #[test]
    fn pair_labels_follow_the_channel_count() {
        let mut prefs = Preferences::default();
        let mut audio = FakeAudio::new();
        let page = AudioSettingsPage::new(&mut prefs, &mut audio);
        assert_eq!(page.pair_labels(), vec!["Out 1+2", "Out 3+4"]);
    }

    #[test]
    fn rate_change_calls_the_model_and_stores_the_device_state() {
        let mut prefs = Preferences::default();
        let mut audio = FakeAudio::new();
        let mut page = AudioSettingsPage::new(&mut prefs, &mut audio);
        assert_eq!(page.set_sample_rate(96_000), Ok(true));
        assert_eq!(
            page.set_sample_rate(96_000),
            Ok(false),
            "same state: no change reported"
        );
        assert_eq!(audio.calls, vec!["rate:96000", "rate:96000"]);
        assert!(prefs.audio.device_state.contains("sampleRate=\"96000\""));
    }

    #[test]
    fn setting_the_same_pair_reports_no_change() {
        let mut prefs = Preferences::default();
        let mut audio = FakeAudio::new();
        let mut page = AudioSettingsPage::new(&mut prefs, &mut audio);
        assert_eq!(page.set_master_pair(1), Ok(true));
        assert_eq!(page.set_master_pair(1), Ok(false));
        assert_eq!(page.set_cue_pair(Some(0)), Ok(true));
        assert_eq!(page.set_cue_pair(None), Ok(true));
        assert_eq!(prefs.audio.master_pair, 1);
        assert_eq!(prefs.audio.cue_pair, -1, "none is stored as -1");
    }

    #[test]
    fn a_stored_cue_pair_missing_on_a_stereo_device_shows_none_and_is_not_rewritten() {
        let mut prefs = Preferences::default();
        let mut audio = FakeAudio::new();
        audio.channels = 2;
        let page = AudioSettingsPage::new(&mut prefs, &mut audio);
        assert_eq!(page.pair_labels().len(), 1);
        assert_eq!(page.shown_cue_pair(), None);
        assert_eq!(
            prefs.audio.cue_pair, 1,
            "viewing must not rewrite the stored choice"
        );
    }

    #[test]
    fn device_and_type_edits_store_the_state() {
        let mut prefs = Preferences::default();
        let mut audio = FakeAudio::new();
        let mut page = AudioSettingsPage::new(&mut prefs, &mut audio);
        page.set_output_device("USB DJ Interface").unwrap();
        page.set_device_type("JACK").unwrap();
        page.set_buffer_size(128).unwrap();
        assert_eq!(
            audio.calls,
            vec!["device:USB DJ Interface", "type:JACK", "buffer:128"]
        );
        assert!(prefs.audio.device_state.contains("deviceType=\"JACK\""));
        assert!(prefs.audio.device_state.contains("bufferSize=\"128\""));
    }

    #[derive(Default)]
    struct FakeMidi;
    impl MidiInputs for FakeMidi {
        fn input_names(&self) -> Vec<String> {
            vec!["usb-1".into(), "usb-2".into()]
        }
    }

    #[test]
    fn midi_inputs_and_the_all_switch_edit_the_preferences() {
        let mut prefs = Preferences::default();
        let inputs = FakeMidi;
        let mut page = MidiSettingsPage::new(&mut prefs, &inputs);
        assert!(page.set_enable_all(false));
        assert!(page.set_input_enabled("usb-1", true));
        assert!(!page.set_input_enabled("usb-1", true), "already enabled");
        assert!(page.set_input_enabled("usb-2", true));
        assert!(page.set_input_enabled("usb-1", false));
        assert_eq!(prefs.midi.enabled_inputs, vec!["usb-2"]);
        assert!(!prefs.midi.enable_all);
    }

    #[test]
    fn map_file_choice_and_reset() {
        let mut prefs = Preferences::default();
        let inputs = FakeMidi;
        let mut page = MidiSettingsPage::new(&mut prefs, &inputs);
        assert!(page.set_map_file(Some(PathBuf::from("/maps/a.json"))));
        assert_eq!(prefs.midi.map_file, "/maps/a.json");
        let mut page = MidiSettingsPage::new(&mut prefs, &inputs);
        assert!(page.set_map_file(None));
        assert_eq!(prefs.midi.map_file, "");
    }

    #[derive(Default)]
    struct FakeLibrary {
        available: bool,
        calls: Vec<String>,
    }
    impl LibraryControl for FakeLibrary {
        fn available(&self) -> bool {
            self.available
        }
        fn add_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
            self.calls.push(format!("add:{}", folder.display()));
            Ok(())
        }
        fn remove_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
            self.calls.push(format!("remove:{}", folder.display()));
            Ok(())
        }
        fn rescan(&mut self) -> SettingsResult<()> {
            self.calls.push("rescan".into());
            Ok(())
        }
    }

    #[test]
    fn library_interval_is_clamped_and_scan_on_start_is_stored() {
        let mut prefs = Preferences::default();
        let mut lib = FakeLibrary {
            available: true,
            ..Default::default()
        };
        {
            let mut page = LibrarySettingsPage::new(&mut prefs, &mut lib);
            assert!(page.set_scan_interval(0));
            assert!(!page.set_scan_interval(1), "clamped to 1 is already stored");
        }
        assert_eq!(prefs.library.scan_interval_minutes, 1);
        {
            let mut page = LibrarySettingsPage::new(&mut prefs, &mut lib);
            assert!(page.set_scan_interval(100_000));
            assert!(page.set_scan_on_start(false));
        }
        assert_eq!(prefs.library.scan_interval_minutes, 1440);
        assert!(!prefs.library.scan_on_start);
    }

    #[test]
    fn library_folder_buttons_are_disabled_without_a_library() {
        let mut prefs = Preferences::default();
        let mut lib = FakeLibrary::default();
        let mut page = LibrarySettingsPage::new(&mut prefs, &mut lib);
        assert!(page.rescan().is_err());
        assert!(page.add_watch_folder(PathBuf::from("/music")).is_err());
        assert!(lib.calls.is_empty());
    }

    #[test]
    fn appearance_lists_the_registry_and_stores_the_pick() {
        let mut prefs = Preferences::default();
        let mut themes = ThemeRegistry::new();
        themes.load_builtins();
        let mut page = AppearanceSettingsPage::new(&mut prefs, &themes);
        assert!(page.choices().iter().any(|(id, _)| id == "openmixer.dark"));
        assert!(page.pick("no.such.theme").is_err());
        let other = page
            .choices()
            .into_iter()
            .map(|(id, _)| id)
            .find(|id| id != "openmixer.dark")
            .expect("the built-ins ship more than one theme");
        assert_eq!(page.pick(&other), Ok(true));
        assert_eq!(page.pick(&other), Ok(false));
        assert_eq!(prefs.theme, other);
    }

    #[test]
    fn panel_omits_the_audio_tab_without_a_model_and_shows_one_page() {
        let mut with = SettingsPanel::new(true);
        assert_eq!(
            with.tabs(),
            &["Audio", "MIDI", "Library", "Appearance", "Plugins"]
        );
        let mut without = SettingsPanel::new(false);
        assert_eq!(
            without.tabs(),
            &["MIDI", "Library", "Appearance", "Plugins"]
        );
        assert_eq!(without.page_kind(0), Some(PageKind::Midi));
        without.show(2);
        assert_eq!(without.current(), 2);
        without.show(99);
        assert_eq!(without.current(), 2, "out of range is ignored");
        let plugin_tab = with.add_page("Extra");
        assert_eq!(plugin_tab, 5);
        assert_eq!(with.page_kind(plugin_tab), None);
    }
}

#[cfg(test)]
mod stats_tests {
    use super::*;

    fn running() -> AudioStats {
        AudioStats {
            sample_rate: 48_000.0,
            buffer_size: 256,
            output_latency_ms: 5.3,
            round_trip_latency_ms: 10.6,
            cpu_load: 0.25,
            max_callback_us: 1_500.0,
            budget_us: 5_333.0,
            xrun_count: 2,
        }
    }

    #[test]
    fn stats_show_latency_load_and_xruns_for_a_running_device() {
        let text = stats_text(&running(), "");
        assert!(text.contains("Output 5.3 ms, round trip 10.6 ms at 48000 Hz / 256 samples"));
        assert!(text.contains("CPU 25% (peak 1.50 ms of 5.33 ms), xruns 2"));
    }

    #[test]
    fn without_a_device_the_label_says_so_unless_a_setup_error_is_pending() {
        assert_eq!(
            stats_text(&AudioStats::default(), ""),
            "No audio device running"
        );
        assert_eq!(
            stats_text(&AudioStats::default(), "no such device"),
            "no such device"
        );
    }

    #[test]
    fn a_setup_error_is_shown_in_place_of_the_stats_only_without_a_device() {
        assert!(!stats_text(&running(), "stale error").contains("stale error"));
    }
}
