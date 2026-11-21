//! The settings page's seam: what the window asks of the app and what it shows. The binary owns
//! the preferences, the output device and the subsystems, so it implements [`SettingsHost`]; this
//! crate only carries the page's data to the front end and one change back.

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

/// What the app offers the settings pages. The app's composition (library, plugins) cannot leave
/// the thread that built it, so a host is not `Send`: [`run`](crate::run) keeps it on the main
/// thread, where Tauri runs the synchronous commands, and nothing else reaches it.
pub trait SettingsHost {
    /// The audio page as it stands.
    fn audio(&self) -> AudioPage;
    /// Applies one edit, saves the preferences and pushes them to the subsystems; the page as it
    /// stands afterwards, or the reason the edit was refused (shown as is).
    fn change_audio(&self, change: AudioChange) -> Result<AudioPage, String>;
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
    fn the_page_serialises_in_camel_case() {
        let json = serde_json::to_string(&AudioPage::default()).unwrap();
        assert!(json.contains("\"deviceTypes\":[]") && json.contains("\"cuePair\":null"));
    }
}
