//! The audio page: device choices from the model, output pairs from the preferences, and the stats readout.

use super::{set_if_changed, SettingsError, SettingsResult};
use crate::preferences::{AudioPrefs, Preferences};

/// What the audio page needs from the audio layer. The layer's errors arrive as
/// [`SettingsError::Audio`].
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

/// The readout under the audio controls. With no device
/// open it shows the last setup error, or "No audio device running".
pub fn stats_text(stats: &AudioStats, setup_error: &str) -> String {
    if stats.sample_rate <= 0.0 {
        return if setup_error.is_empty() {
            "No audio device running".to_owned()
        } else {
            setup_error.to_owned()
        };
    }
    crate::labels::audio_stats_text(stats)
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

    /// Output pairs of the open device: `channels / 2` pairs, labelled "Outputs 1-2", "Outputs 3-4"...
    pub fn pair_labels(&self) -> Vec<String> {
        (0..self.model.output_channels() / 2)
            .map(crate::labels::output_pair_label)
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
        let pair = i32::try_from(pair).map_err(|_| SettingsError::InvalidValue {
            field: "output pair",
        })?;
        Ok(set_if_changed(&mut self.prefs.audio.master_pair, pair))
    }

    /// `None` means no cue output (stored as -1).
    pub fn set_cue_pair(&mut self, pair: Option<usize>) -> SettingsResult {
        let stored = match pair {
            Some(p) => i32::try_from(p).map_err(|_| SettingsError::InvalidValue {
                field: "output pair",
            })?,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::Preferences;

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
        assert_eq!(page.pair_labels(), vec!["Outputs 1-2", "Outputs 3-4"]);
    }

    #[test]
    fn rate_change_calls_the_model_and_stores_the_device_state() {
        let mut prefs = Preferences::default();
        let mut audio = FakeAudio::new();
        let mut page = AudioSettingsPage::new(&mut prefs, &mut audio);
        assert_eq!(page.set_sample_rate(96_000).ok(), Some(true));
        assert_eq!(
            page.set_sample_rate(96_000).ok(),
            Some(false),
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
        assert_eq!(page.set_master_pair(1).ok(), Some(true));
        assert_eq!(page.set_master_pair(1).ok(), Some(false));
        assert_eq!(page.set_cue_pair(Some(0)).ok(), Some(true));
        assert_eq!(page.set_cue_pair(None).ok(), Some(true));
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
        assert!(text.contains("Latency 5.3 ms, round trip 10.6 ms at 48000 Hz / 256 samples"));
        assert!(text.contains("CPU 25% (peak 1.50 ms of 5.33 ms), dropouts 2"));
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

    #[test]
    fn reflecting_the_audio_page_does_not_change_the_stored_pairs() {
        struct Model;
        impl AudioSettingsModel for Model {
            fn device_types(&self) -> Vec<String> {
                Vec::new()
            }
            fn device_type(&self) -> String {
                String::new()
            }
            fn set_device_type(&mut self, _: &str) -> SettingsResult<()> {
                Err(SettingsError::Audio("must not be called".into()))
            }
            fn output_devices(&self) -> Vec<String> {
                Vec::new()
            }
            fn output_device(&self) -> String {
                String::new()
            }
            fn set_output_device(&mut self, _: &str) -> SettingsResult<()> {
                Err(SettingsError::Audio("must not be called".into()))
            }
            fn sample_rates(&self) -> Vec<u32> {
                Vec::new()
            }
            fn sample_rate(&self) -> u32 {
                0
            }
            fn set_sample_rate(&mut self, _: u32) -> SettingsResult<()> {
                Err(SettingsError::Audio("must not be called".into()))
            }
            fn buffer_sizes(&self) -> Vec<u32> {
                Vec::new()
            }
            fn buffer_size(&self) -> u32 {
                0
            }
            fn set_buffer_size(&mut self, _: u32) -> SettingsResult<()> {
                Err(SettingsError::Audio("must not be called".into()))
            }
            fn output_channels(&self) -> usize {
                4
            }
            fn state_string(&self) -> String {
                String::new()
            }
        }
        let mut prefs = Preferences::default();
        prefs.audio.master_pair = 1;
        prefs.audio.cue_pair = 9;
        let before = prefs.audio.clone();
        let mut model = Model;
        {
            let page = AudioSettingsPage::new(&mut prefs, &mut model);
            assert_eq!(page.pair_labels(), vec!["Outputs 1-2", "Outputs 3-4"]);
            assert_eq!(
                page.shown_cue_pair(),
                None,
                "a stored pair the device lacks shows None"
            );
        }
        assert_eq!(
            prefs.audio, before,
            "viewing never rewrites the stored pairs"
        );
    }
}
