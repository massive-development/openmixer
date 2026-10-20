//! The MIDI page: input selection, the controller map and MIDI learn.

use super::{set_if_changed, SettingsError};
use crate::preferences::{default_user_map_file, Preferences};
use om_core::param_id::{
    ChannelParam, DeckParam, MixerParam, SamplerParam, StemParam, SAMPLER_PADS, STEMS_PER_DECK,
};
use om_core::ParamId;
use std::path::{Path, PathBuf};

/// What the MIDI page needs: the input devices the host can see.
pub trait MidiInputs {
    fn input_names(&self) -> Vec<String>;
}

/// MIDI page logic over the preferences.
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

/// The running MIDI service as the settings, the preferences and the MIDI page see it: the
/// controller map, MIDI learn and the input selection. Methods take `&self`: the service is shared
/// with the MIDI threads and locks internally.
pub trait MidiHost {
    /// Replaces the live map with the file's; a bad file changes nothing. Called before the
    /// inputs are enabled, so no message meets a stale map.
    fn load_map(&self, file: &Path) -> Result<(), SettingsError>;
    /// Selects the inputs named in `inputs` (every one when `enable_all`) and opens them.
    fn enable_inputs(&self, enable_all: bool, inputs: &[String]);
    /// The next control that moves binds to `target`.
    fn arm_learn(&self, target: &str);
    fn cancel_learn(&self);
    fn is_learning(&self) -> bool;
    /// Writes the live map to `file` (atomically).
    fn save_map(&self, file: &Path) -> Result<(), SettingsError>;
    /// Counts the edits to the live map (loads and learned bindings). A learn that finished
    /// shows as a change of this count.
    fn map_generation(&self) -> u64 {
        0
    }
}

/// The running MIDI service as the MIDI page sees it: the device list and the map and learn
/// controls. Blanket-implemented, so any type with both traits is a page host.
pub trait MidiPage: MidiInputs + MidiHost {}

impl<T: MidiInputs + MidiHost + ?Sized> MidiPage for T {}

/// The parameter ids the MIDI page offers for MIDI learn: the decks' transport and rate, each
/// stem's mute on every deck, the channel faders and trims, the crossfader, the master and the
/// eight sampler pad triggers. Ids are
/// the router's full ids.
pub fn learnable_targets(decks: usize) -> Vec<String> {
    let mut ids: Vec<ParamId> = Vec::new();
    for deck in 0..decks {
        let deck = deck as u8;
        ids.push(ParamId::Deck {
            deck,
            p: DeckParam::Play,
        });
        ids.push(ParamId::Deck {
            deck,
            p: DeckParam::Rate,
        });
        for stem in 0..STEMS_PER_DECK {
            ids.push(ParamId::Stem {
                deck,
                stem,
                p: StemParam::Mute,
            });
        }
    }
    for channel in 0..decks {
        let ch = channel as u8;
        ids.push(ParamId::Mixer(MixerParam::Channel {
            ch,
            p: ChannelParam::Fader,
        }));
        ids.push(ParamId::Mixer(MixerParam::Channel {
            ch,
            p: ChannelParam::Trim,
        }));
    }
    ids.push(ParamId::Mixer(MixerParam::Crossfader));
    ids.push(ParamId::Mixer(MixerParam::Master));
    for pad in 0..SAMPLER_PADS {
        ids.push(ParamId::Sampler {
            pad,
            p: SamplerParam::Trigger,
        });
    }
    ids.iter().map(ParamId::to_string).collect()
}

impl MidiSettingsPage<'_> {
    /// Loads the effective map file (the preference, or the default one) into the running service.
    pub fn load_map_now(&self, control: &dyn MidiHost) -> Result<(), SettingsError> {
        control.load_map(&self.prefs.effective_map_file())
    }

    /// Starts MIDI learn for `target`; the page shows the learning state from [`MidiHost::is_learning`].
    pub fn learn(&self, control: &dyn MidiHost, target: &str) {
        control.arm_learn(target);
    }

    pub fn cancel_learn(&self, control: &dyn MidiHost) {
        control.cancel_learn();
    }

    /// Saves the live map after a learn and makes the file the map in use. The file is the stored
    /// map file when there is one, else the user map. Returns the file and whether the preference
    /// changed (the caller saves preferences then).
    pub fn save_learned_map(
        &mut self,
        control: &dyn MidiHost,
    ) -> Result<(PathBuf, bool), SettingsError> {
        let file = if self.prefs.midi.map_file.is_empty() {
            default_user_map_file()
        } else {
            PathBuf::from(&self.prefs.midi.map_file)
        };
        control.save_map(&file)?;
        let changed = self.set_map_file(Some(file.clone()));
        Ok((file, changed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::Preferences;

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

    struct Inputs(Vec<String>);

    impl MidiInputs for Inputs {
        fn input_names(&self) -> Vec<String> {
            self.0.clone()
        }
    }

    #[test]
    fn midi_lists_the_inputs_the_host_sees() {
        let mut prefs = Preferences::default();
        prefs.midi.enabled_inputs = vec!["usb-1".into()];
        let inputs = Inputs(vec!["usb-1".into(), "usb-2".into()]);
        let page = MidiSettingsPage::new(&mut prefs, &inputs);
        assert_eq!(page.input_names(), vec!["usb-1", "usb-2"]);
    }

    #[derive(Default)]
    struct FakeControl {
        loaded: std::cell::RefCell<Vec<PathBuf>>,
        learning: std::cell::RefCell<Option<String>>,
        saved: std::cell::RefCell<Vec<PathBuf>>,
    }
    impl MidiHost for FakeControl {
        fn load_map(&self, file: &Path) -> Result<(), SettingsError> {
            if file.extension().and_then(|e| e.to_str()) != Some("json") {
                return Err(SettingsError::Midi("not a map".into()));
            }
            self.loaded.borrow_mut().push(file.to_path_buf());
            Ok(())
        }
        fn save_map(&self, file: &Path) -> Result<(), SettingsError> {
            self.saved.borrow_mut().push(file.to_path_buf());
            Ok(())
        }
        fn arm_learn(&self, target: &str) {
            *self.learning.borrow_mut() = Some(target.to_owned());
        }
        fn cancel_learn(&self) {
            *self.learning.borrow_mut() = None;
        }
        fn is_learning(&self) -> bool {
            self.learning.borrow().is_some()
        }
        fn enable_inputs(&self, _enable_all: bool, _inputs: &[String]) {}
    }

    #[test]
    fn learn_saves_the_map_to_the_stored_file_or_the_user_map() {
        let control = FakeControl::default();
        let mut prefs = Preferences::default();
        let inputs = Inputs(Vec::new());
        {
            let mut page = MidiSettingsPage::new(&mut prefs, &inputs);
            let (file, changed) = page.save_learned_map(&control).expect("saves");
            assert_eq!(file, crate::preferences::default_user_map_file());
            assert!(changed);
        }
        assert_eq!(
            prefs.midi.map_file,
            crate::preferences::default_user_map_file().to_string_lossy()
        );
        assert_eq!(control.saved.borrow().len(), 1);

        prefs.midi.map_file = "/maps/mine.json".into();
        let mut page = MidiSettingsPage::new(&mut prefs, &inputs);
        let (file, changed) = page.save_learned_map(&control).expect("saves");
        assert_eq!(file, PathBuf::from("/maps/mine.json"));
        assert!(!changed, "the stored file is already the map in use");
        assert_eq!(
            control.saved.borrow().last(),
            Some(&PathBuf::from("/maps/mine.json"))
        );
    }

    #[test]
    fn midi_page_loads_the_effective_map_and_drives_learn() {
        struct NoInputs;
        impl MidiInputs for NoInputs {
            fn input_names(&self) -> Vec<String> {
                Vec::new()
            }
        }
        let mut prefs = Preferences::default();
        prefs.midi.map_file = "/maps/pad.json".into();
        let inputs = NoInputs;
        let control = FakeControl::default();
        let page = MidiSettingsPage::new(&mut prefs, &inputs);
        page.load_map_now(&control).expect("loads");
        assert_eq!(
            control.loaded.borrow().as_slice(),
            [PathBuf::from("/maps/pad.json")]
        );

        page.learn(&control, "deck.0.play");
        assert!(control.is_learning());
        page.cancel_learn(&control);
        assert!(!control.is_learning());

        let mut prefs = Preferences::default();
        prefs.midi.map_file = "/maps/pad.txt".into();
        let page = MidiSettingsPage::new(&mut prefs, &inputs);
        assert!(page.load_map_now(&control).is_err());
    }

    #[test]
    fn midi_with_no_inputs_still_builds_and_the_all_switch_stores_once() {
        let mut prefs = Preferences::default();
        prefs.midi.enable_all = false;
        let inputs = Inputs(Vec::new());
        let mut page = MidiSettingsPage::new(&mut prefs, &inputs);
        assert!(page.input_names().is_empty());
        assert!(page.set_enable_all(true));
        assert!(!page.set_enable_all(true));
        assert!(prefs.midi.enable_all);
    }

    #[test]
    fn learn_targets_are_full_router_ids_for_each_deck_and_the_mixer() {
        let ids = learnable_targets(2);
        assert_eq!(ids.len(), 6 * 2 + 2 * 2 + 2 + 8);
        assert!(ids.contains(&"deck.1.play".to_owned()));
        assert!(ids.contains(&"stem.1.3.mute".to_owned()));
        assert!(!ids.contains(&"stem.1.3.volume".to_owned()));
        assert!(ids.contains(&"mixer.ch0.fader".to_owned()));
        assert!(ids.contains(&"mixer.crossfader".to_owned()));
        assert!(ids.contains(&"sampler.pad0.trigger".to_owned()));
        assert!(ids.contains(&"sampler.pad7.trigger".to_owned()));
        assert!(
            learnable_targets(0).len() == 2 + 8,
            "no decks: mixer and sampler only"
        );
    }

    #[test]
    fn every_learn_target_parses_back_to_a_typed_id() {
        for id in learnable_targets(4) {
            let parsed: ParamId = id.parse().unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_eq!(parsed.to_string(), id);
            assert!(parsed.namespace().is_some(), "{id} is an engine id");
        }
    }
}
