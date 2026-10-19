//! The Audio tab: device, output pairs and the stats readout.

use super::chips::*;
use super::*;

/// Plain values the Audio page shows, read once per frame.
pub(super) struct AudioView {
    types: Vec<String>,
    device_type: String,
    devices: Vec<String>,
    device: String,
    rates: Vec<u32>,
    rate: u32,
    buffers: Vec<u32>,
    buffer: u32,
    pairs: Vec<String>,
    master: usize,
    cue: Option<usize>,
    readout: String,
}

pub(super) fn audio_page(t: &ThemeTokens, view: &AudioView, cx: &mut Context<Shell>) -> Div {
    let mut types = Vec::new();
    for name in &view.types {
        let (lit, name) = (*name == view.device_type, name.clone());
        types.push(chip(t, &name.clone(), lit, cx, move |this| {
            let name = name.clone();
            this.settings_mut(|s| s.edit_audio(|page| page.set_device_type(&name)));
        }));
    }
    let mut devices = Vec::new();
    for name in &view.devices {
        let (lit, name) = (*name == view.device, name.clone());
        devices.push(chip(t, &name.clone(), lit, cx, move |this| {
            let name = name.clone();
            this.settings_mut(|s| s.edit_audio(|page| page.set_output_device(&name)));
        }));
    }
    let mut rates = Vec::new();
    for rate in &view.rates {
        let rate = *rate;
        rates.push(chip(
            t,
            &format!("{rate} Hz"),
            rate == view.rate,
            cx,
            move |this| {
                this.settings_mut(|s| s.edit_audio(|page| page.set_sample_rate(rate)));
            },
        ));
    }
    let mut buffers = Vec::new();
    for size in &view.buffers {
        let size = *size;
        buffers.push(chip(
            t,
            &format!("{size} samples"),
            size == view.buffer,
            cx,
            move |this| {
                this.settings_mut(|s| s.edit_audio(|page| page.set_buffer_size(size)));
            },
        ));
    }
    let mut master = Vec::new();
    for (index, label) in view.pairs.iter().enumerate() {
        master.push(chip(t, label, index == view.master, cx, move |this| {
            this.settings_mut(|s| s.edit_audio(|page| page.set_master_pair(index)));
        }));
    }
    let mut cue = vec![chip(t, "None", view.cue.is_none(), cx, |this| {
        this.settings_mut(|s| s.edit_audio(|page| page.set_cue_pair(None)));
    })];
    for (index, label) in view.pairs.iter().enumerate() {
        cue.push(chip(t, label, view.cue == Some(index), cx, move |this| {
            this.settings_mut(|s| s.edit_audio(|page| page.set_cue_pair(Some(index))));
        }));
    }

    let mut column = div().flex().flex_col();
    column = column.child(choice_row(t, "Driver", row_of(types)));
    column = column.child(choice_row(t, "Output device", row_of(devices)));
    column = column.child(choice_row(t, "Sample rate", row_of(rates)));
    column = column.child(choice_row(t, "Buffer size", row_of(buffers)));
    if view.pairs.is_empty() {
        // No pairs to choose from (a device with no stereo pairs): no headings without options.
        column = column.child(note(
            t,
            "This device lists no stereo output pairs, so master and headphone outputs use its default.",
        ));
    } else {
        column = column.child(choice_row(t, "Master output", row_of(master)));
        column = column.child(choice_row(t, "Headphone output", row_of(cue)));
        column = column.child(note(
            t,
            "Master and headphone mixes go to the chosen stereo pairs of the output device.",
        ));
    }
    column.child(
        div()
            .text_color(paint(t.text_secondary))
            .text_size(px(t.type_caption))
            .child(view.readout.clone()),
    )
}

impl SettingsState {
    /// Runs one audio edit. A failure becomes the readout's text; a change is saved and applied.
    pub fn edit_audio(&mut self, edit: impl FnOnce(&mut AudioSettingsPage<'_>) -> SettingsResult) {
        let Some(model) = self.audio.as_deref_mut() else {
            return;
        };
        let result = {
            let mut page = AudioSettingsPage::new(&mut self.prefs, model);
            edit(&mut page)
        };
        match result {
            Ok(changed) => {
                self.setup_error.clear();
                if changed {
                    self.changed();
                }
            }
            Err(error) => self.setup_error = error.to_string(),
        }
    }

    /// The Audio page's values, read through the page logic.
    pub(super) fn audio_view(&mut self) -> Option<AudioView> {
        let stats = self.audio_stats.as_ref().map(|f| f()).unwrap_or_default();
        let readout = stats_text(&stats, &self.setup_error);
        let model = self.audio.as_deref_mut()?;
        let types = model.device_types();
        let device_type = model.device_type();
        let devices = model.output_devices();
        let device = model.output_device();
        let rates = model.sample_rates();
        let rate = model.sample_rate();
        let buffers = model.buffer_sizes();
        let buffer = model.buffer_size();
        let page = AudioSettingsPage::new(&mut self.prefs, model);
        Some(AudioView {
            types,
            device_type,
            devices,
            device,
            rates,
            rate,
            buffers,
            buffer,
            pairs: page.pair_labels(),
            master: page.audio_prefs().master_pair.max(0) as usize,
            cue: page.shown_cue_pair(),
            readout,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_view::fixtures::*;
    use om_ui::settings::SettingsResult;
    use std::cell::RefCell;

    use std::rc::Rc;

    /// Records calls; fails a rate of 22 050 Hz so the error path runs.
    #[derive(Default)]
    struct FakeAudio {
        rate: u32,
        log: Rc<RefCell<Vec<String>>>,
    }

    impl AudioSettingsModel for FakeAudio {
        fn device_types(&self) -> Vec<String> {
            vec!["Fake".into()]
        }
        fn device_type(&self) -> String {
            "Fake".into()
        }
        fn set_device_type(&mut self, _: &str) -> SettingsResult<()> {
            Ok(())
        }
        fn output_devices(&self) -> Vec<String> {
            vec!["Out".into()]
        }
        fn output_device(&self) -> String {
            "Out".into()
        }
        fn set_output_device(&mut self, _: &str) -> SettingsResult<()> {
            Ok(())
        }
        fn sample_rates(&self) -> Vec<u32> {
            vec![44_100, 48_000]
        }
        fn sample_rate(&self) -> u32 {
            self.rate
        }
        fn set_sample_rate(&mut self, rate: u32) -> SettingsResult<()> {
            if rate == 22_050 {
                return Err(SettingsError::Audio(
                    "unsupported device configuration: 22050 Hz".into(),
                ));
            }
            self.rate = rate;
            self.log.borrow_mut().push(format!("rate {rate}"));
            Ok(())
        }
        fn buffer_sizes(&self) -> Vec<u32> {
            vec![256, 512]
        }
        fn buffer_size(&self) -> u32 {
            512
        }
        fn set_buffer_size(&mut self, _: u32) -> SettingsResult<()> {
            Ok(())
        }
        fn output_channels(&self) -> usize {
            2
        }
        fn state_string(&self) -> String {
            format!("rate={}", self.rate)
        }
    }

    #[test]
    fn an_audio_change_notifies_once_and_repeating_it_does_not() {
        let changes = Rc::new(RefCell::new(0));
        let log = Rc::new(RefCell::new(Vec::new()));
        let audio = FakeAudio {
            rate: 48_000,
            log: Rc::clone(&log),
        };
        let mut s = state(Some(Box::new(audio)), Rc::clone(&changes));
        s.edit_audio(|page| page.set_sample_rate(44_100));
        s.edit_audio(|page| page.set_sample_rate(44_100));
        assert_eq!(*changes.borrow(), 1);
        // The model is asked both times; only the stored state decides the change.
        assert_eq!(
            *log.borrow(),
            vec!["rate 44100".to_owned(), "rate 44100".to_owned()]
        );
        assert_eq!(s.prefs.audio.device_state, "rate=44100");
    }

    #[test]
    fn a_failed_audio_change_is_kept_as_the_readout_text_and_notifies_nobody() {
        let changes = Rc::new(RefCell::new(0));
        let audio = FakeAudio {
            rate: 48_000,
            ..FakeAudio::default()
        };
        let mut s = state(Some(Box::new(audio)), Rc::clone(&changes));
        s.edit_audio(|page| page.set_sample_rate(22_050));
        assert_eq!(*changes.borrow(), 0);
        assert!(s.setup_error.contains("22050"));
        // With no device the readout shows that error instead of "No audio device running".
        assert_eq!(
            stats_text(&AudioStats::default(), &s.setup_error),
            s.setup_error
        );
    }

    #[test]
    fn no_audio_model_leaves_the_audio_tab_out_and_edits_nothing() {
        let changes = Rc::new(RefCell::new(0));
        let mut s = state(None, Rc::clone(&changes));
        assert!(!s.panel.tabs().iter().any(|t| t == "Audio"));
        s.edit_audio(|page| page.set_sample_rate(44_100));
        assert_eq!(*changes.borrow(), 0);
    }
}
