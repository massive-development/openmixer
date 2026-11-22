//! The settings page's seam: what the window asks of the app and what it shows. The binary owns
//! the preferences, the output device and the subsystems, so it implements [`SettingsHost`]; this
//! crate only carries the page's data to the front end and one change back.

use crate::devices::DevicesHost;
use om_ui::preferences::AutomixPrefs;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

thread_local! {
    static HOST: RefCell<Option<Box<dyn SettingsHost>>> = const { RefCell::new(None) };
}

/// Keeps `host` for this thread's commands. Call on the main thread before the window runs.
pub(crate) fn install(host: Option<Box<dyn SettingsHost>>) {
    HOST.with(|slot| *slot.borrow_mut() = host);
}

/// Drops the host (and what it owns) on this thread once the window has closed.
pub(crate) fn uninstall() {
    install(None);
}

/// Runs `f` on the installed host; an error where there is none (another thread, or a run with
/// no settings).
pub(crate) fn with_host<R>(f: impl FnOnce(&dyn SettingsHost) -> R) -> Result<R, String> {
    HOST.with(|slot| match slot.borrow().as_deref() {
        Some(host) => Ok(f(host)),
        None => Err("settings are not available here".to_owned()),
    })
}

/// The audio page as the front end draws it: the choices the device offers and the current pick.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioPage {
    /// Audio host kinds (ALSA, JACK, WASAPI, ...); empty on the silent mock.
    pub device_types: Vec<String>,
    pub device_type: String,
    pub devices: Vec<String>,
    pub device: String,
    pub sample_rates: Vec<u32>,
    pub sample_rate: u32,
    pub buffer_sizes: Vec<u32>,
    pub buffer_size: u32,
    /// Output pair labels of the open device ("Outputs 1-2", ...).
    pub pairs: Vec<String>,
    pub master_pair: usize,
    /// The headphone cue pair; null for none.
    pub cue_pair: Option<usize>,
    /// The latency and load readout, or why no device runs.
    pub stats: String,
    /// The silent mock is running: no change reaches a sound card.
    pub mock: bool,
}

/// One edit of the audio page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "camelCase")]
pub enum AudioChange {
    DeviceType(String),
    Device(String),
    SampleRate(u32),
    BufferSize(u32),
    MasterPair(usize),
    CuePair(Option<usize>),
}

/// One theme the picker offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThemeChoice {
    pub id: String,
    pub name: String,
}

/// The appearance page: the themes and the density choice (`automatic`, `roomy` or `compact`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AppearancePage {
    pub themes: Vec<ThemeChoice>,
    pub theme: String,
    pub density: String,
}

/// One edit of the appearance page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "camelCase")]
pub enum AppearanceChange {
    Theme(String),
    Density(String),
}

/// What an appearance edit leaves the window to apply: the page, the theme's CSS and each deck's
/// lane ink.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceResult {
    pub page: AppearancePage,
    pub css: String,
    pub strip_inks: Vec<crate::StripInkDto>,
}

/// One transition length the Automix page offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LengthChoice {
    pub beats: i32,
    pub label: String,
}

/// Where the automix queue picks from: `library`, `playlist` (with `playlist`) or `search`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueSource {
    pub kind: String,
    #[serde(default)]
    pub playlist: i64,
}

/// The Automix page: the stored settings and the words for their choices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomixPage {
    pub enabled: bool,
    /// `automix.style` code and the labels in code order.
    pub style: i32,
    pub styles: Vec<String>,
    pub length_beats: i32,
    pub lengths: Vec<LengthChoice>,
    pub queue: QueueSource,
    pub smart: bool,
    /// Largest tempo gap a beat-matched style accepts, in percent.
    pub min_bpm_gap_percent: i32,
    pub max_bpm_gap_percent: i32,
    pub autostart_at_end: bool,
}

/// One edit of the Automix page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "camelCase")]
pub enum AutomixChange {
    Enabled(bool),
    Style(i32),
    LengthBeats(i32),
    Queue(QueueSource),
    Smart(bool),
    MinBpmGapPercent(i32),
    AutostartAtEnd(bool),
}

/// One recording format the page offers: the stored code and its label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FormatChoice {
    pub id: String,
    pub label: String,
}

/// The Recording page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordPage {
    /// The stored folder; empty means the default.
    pub folder: String,
    /// Where an empty folder records to.
    pub default_folder: String,
    pub format: String,
    pub formats: Vec<FormatChoice>,
    pub stop_on_low_disk: bool,
}

/// One edit of the Recording page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "camelCase")]
pub enum RecordChange {
    Folder(String),
    Format(String),
    StopOnLowDisk(bool),
}

/// What the app offers the settings pages. The app's composition (library, plugins) cannot leave
/// the thread that built it, so a host is not `Send`: [`run`](crate::run) keeps it on the main
/// thread, where Tauri runs the synchronous commands, and nothing else reaches it.
pub trait SettingsHost {
    /// The audio page as it stands.
    fn audio(&self) -> AudioPage;
    /// Applies one edit, saves the preferences and pushes them to the subsystems; the page as it
    /// stands afterwards, or the reason the edit was refused (shown as is).
    fn change_audio(&self, change: AudioChange) -> Result<AudioPage, String>;
    /// The MIDI, mapping and plugins pages; none where the app has no such services.
    fn devices(&self) -> Option<&dyn DevicesHost> {
        None
    }
    /// The appearance page as it stands.
    fn appearance(&self) -> AppearancePage;
    /// Applies one edit (saved, and the theme switched in the composition); the page and the
    /// theme to show, or the reason it was refused.
    fn change_appearance(&self, change: AppearanceChange) -> Result<AppearanceResult, String>;
    /// The Automix page as it stands.
    fn automix(&self) -> AutomixPage;
    /// The stored Automix settings, for the top bar's strip (it shows the length).
    fn automix_prefs(&self) -> AutomixPrefs;
    /// Applies one edit to the stored settings and the running controller, and saves; the page as
    /// it stands afterwards, or the reason it was refused. The top bar's switch and length chip
    /// use this too, so every part edits one copy of the preferences.
    fn change_automix(&self, change: AutomixChange) -> Result<AutomixPage, String>;
    /// The Recording page as it stands.
    fn record(&self) -> RecordPage;
    /// Applies one edit (saved, and pushed to the recorder for the next recording).
    fn change_record(&self, change: RecordChange) -> Result<RecordPage, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_reads_its_field_and_value() {
        let c: AudioChange =
            serde_json::from_str(r#"{"field":"sampleRate","value":48000}"#).unwrap();
        assert_eq!(c, AudioChange::SampleRate(48_000));
        let c: AudioChange = serde_json::from_str(r#"{"field":"cuePair","value":null}"#).unwrap();
        assert_eq!(c, AudioChange::CuePair(None));
        assert!(serde_json::from_str::<AudioChange>(r#"{"field":"bogus","value":1}"#).is_err());
    }

    #[test]
    fn an_appearance_change_reads_its_field_and_value() {
        let c: AppearanceChange =
            serde_json::from_str(r#"{"field":"density","value":"compact"}"#).unwrap();
        assert_eq!(c, AppearanceChange::Density("compact".into()));
    }

    #[test]
    fn an_automix_change_reads_its_field_and_value() {
        let c: AutomixChange =
            serde_json::from_str(r#"{"field":"lengthBeats","value":16}"#).unwrap();
        assert_eq!(c, AutomixChange::LengthBeats(16));
        let c: AutomixChange =
            serde_json::from_str(r#"{"field":"queue","value":{"kind":"playlist","playlist":7}}"#)
                .unwrap();
        assert_eq!(
            c,
            AutomixChange::Queue(QueueSource {
                kind: "playlist".into(),
                playlist: 7
            })
        );
        let c: AutomixChange =
            serde_json::from_str(r#"{"field":"queue","value":{"kind":"search"}}"#).unwrap();
        assert!(matches!(c, AutomixChange::Queue(q) if q.playlist == 0));
    }

    #[test]
    fn a_record_change_reads_its_field_and_value() {
        let c: RecordChange =
            serde_json::from_str(r#"{"field":"stopOnLowDisk","value":false}"#).unwrap();
        assert_eq!(c, RecordChange::StopOnLowDisk(false));
        assert!(serde_json::from_str::<RecordChange>(r#"{"field":"bogus","value":1}"#).is_err());
    }

    #[test]
    fn the_page_serialises_in_camel_case() {
        let json = serde_json::to_string(&AudioPage::default()).unwrap();
        assert!(json.contains("\"deviceTypes\":[]") && json.contains("\"cuePair\":null"));
    }
}
