//! The MIDI tab: inputs, the map file and MIDI learn.

use super::chips::*;
use super::*;
use om_ui::labels::param_label;
use std::time::{Duration, Instant};

/// How often the input list is read again while the MIDI page is showing.
const MIDI_POLL: Duration = Duration::from_secs(1);

/// The device names the page was last given, as the MIDI page's input list.
pub(super) struct Listed<'a>(&'a [String]);

impl MidiInputs for Listed<'_> {
    fn input_names(&self) -> Vec<String> {
        self.0.to_vec()
    }
}

pub(super) fn midi_page(t: &ThemeTokens, settings: &SettingsState, cx: &mut Context<Shell>) -> Div {
    if !settings.midi_available() {
        return note(
            t,
            "MIDI is not available (see the terminal for the reason).",
        );
    }
    let all = settings.prefs.midi.enable_all;
    let mut inputs = vec![chip(t, "All inputs", all, cx, move |this| {
        this.settings_mut(|s| s.midi_set_enable_all(!all))
    })];
    for name in &settings.lists.midi_inputs {
        let lit = all || settings.prefs.midi.enabled_inputs.contains(name);
        let press = name.clone();
        inputs.push(chip(t, name, lit, cx, move |this| {
            let name = press.clone();
            this.settings_mut(|s| s.midi_toggle_input(&name))
        }));
    }
    let mut inputs_column = div().flex().flex_col().child(choice_row(
        t,
        "Input devices (turn off All inputs to choose)",
        row_of(inputs),
    ));
    if settings.lists.midi_inputs.is_empty() {
        inputs_column = inputs_column.child(note(t, "No MIDI input device is listed."));
    }

    let map_row = row_of(vec![
        chip(t, "Load map file…", false, cx, |this| {
            this.settings_mut(|s| s.begin_entry(EntryKind::MapFile))
        }),
        chip(t, "Load the current map", false, cx, |this| {
            this.settings_mut(|s| s.midi_load_current_map())
        }),
        chip(t, "Use the default map", false, cx, |this| {
            this.settings_mut(|s| s.midi_use_default_map())
        }),
    ]);
    let map_in_use = settings.prefs.effective_map_file();
    let mut column = inputs_column
        .child(note(t, &format!("Map file: {}", map_in_use.display())))
        .child(choice_row(t, "Controller map", map_row));
    if let Some(field) = entry_field(
        t,
        settings,
        EntryKind::MapFile,
        "controller map (JSON) path",
        cx,
    ) {
        column = column.child(field);
    }

    let mut learn: Vec<Div> = Vec::new();
    for target in settings.learn_targets() {
        let lit = settings.learning_target() == Some(target.as_str());
        let press = target.clone();
        learn.push(chip(t, &param_label(target), lit, cx, move |this| {
            let target = press.clone();
            this.settings_mut(|s| s.midi_learn(&target))
        }));
    }
    if settings.learning_target().is_some() {
        learn.push(chip(t, "Cancel learn", true, cx, |this| {
            this.settings_mut(|s| s.midi_cancel_learn())
        }));
    }
    column.child(choice_row(
        t,
        "MIDI learn: pick a target, then move a control",
        row_of(learn),
    ))
}

impl SettingsState {
    pub fn midi_available(&self) -> bool {
        self.midi.is_some()
    }

    pub fn learn_targets(&self) -> &[String] {
        &self.learn_targets
    }

    /// The target MIDI learn is waiting on, if one is armed.
    pub fn learning_target(&self) -> Option<&str> {
        self.learning.as_ref().map(|(target, _)| target.as_str())
    }

    /// Stores the "all inputs" switch.
    pub fn midi_set_enable_all(&mut self, all: bool) {
        let changed = MidiSettingsPage::new(&mut self.prefs, &Listed(&self.lists.midi_inputs))
            .set_enable_all(all);
        if changed {
            self.changed();
        }
    }

    /// Adds or removes one input from the explicit selection.
    pub fn midi_toggle_input(&mut self, name: &str) {
        let on = !self.prefs.midi.enabled_inputs.iter().any(|n| n == name);
        let changed = MidiSettingsPage::new(&mut self.prefs, &Listed(&self.lists.midi_inputs))
            .set_input_enabled(name, on);
        if changed {
            self.changed();
        }
    }

    /// Arms MIDI learn for `target`. The status says so until a control is learned or the learn is
    /// cancelled.
    pub fn midi_learn(&mut self, target: &str) {
        let Some(midi) = self.midi.clone() else {
            return self.set_notice("MIDI is not available", true);
        };
        let generation = midi.map_generation();
        let inputs = Listed(&self.lists.midi_inputs);
        let page = MidiSettingsPage::new(&mut self.prefs, &inputs);
        page.learn(midi.as_ref(), target);
        self.learning = Some((target.to_owned(), generation));
        self.set_notice(
            format!(
                "Learning {}: move a control on the controller. Cancel stops it.",
                param_label(target)
            ),
            false,
        );
    }

    pub fn midi_cancel_learn(&mut self) {
        let Some(midi) = self.midi.clone() else {
            return;
        };
        let inputs = Listed(&self.lists.midi_inputs);
        let page = MidiSettingsPage::new(&mut self.prefs, &inputs);
        page.cancel_learn(midi.as_ref());
        self.learning = None;
        self.set_notice("Learn cancelled", false);
    }

    /// Called each frame: when the MIDI service has stopped learning, says what happened.
    /// Reads the input list again when the MIDI page is showing and `MIDI_POLL` has passed since
    /// the last read, so a controller plugged in with the overlay open appears. Enumerating asks
    /// the backend, so it is not done every frame. Returns whether the list changed.
    pub fn poll_midi_inputs(&mut self, now: Instant) -> bool {
        let Some(midi) = &self.midi else {
            return false;
        };
        if self.panel.page_kind(self.panel.current()) != Some(PageKind::Midi) {
            return false;
        }
        if self
            .midi_polled
            .is_some_and(|last| now.duration_since(last) < MIDI_POLL)
        {
            return false;
        }
        self.midi_polled = Some(now);
        let names = MidiInputs::input_names(&**midi);
        let changed = names != self.lists.midi_inputs;
        self.lists.midi_inputs = names;
        changed
    }

    pub fn poll_learn(&mut self) {
        let Some((target, generation)) = self.learning.clone() else {
            return;
        };
        let Some(midi) = self.midi.clone() else {
            self.learning = None;
            return;
        };
        if midi.is_learning() {
            return;
        }
        self.learning = None;
        if midi.map_generation() == generation {
            self.set_notice(
                format!(
                    "Learn for {} ended: no control was received",
                    param_label(&target)
                ),
                true,
            );
            return;
        }
        let saved = MidiSettingsPage::new(&mut self.prefs, &Listed(&self.lists.midi_inputs))
            .save_learned_map(midi.as_ref());
        match saved {
            Ok((file, changed)) => {
                self.set_notice(
                    format!(
                        "Learned a control for {} and saved to {}",
                        param_label(&target),
                        file.display()
                    ),
                    false,
                );
                if changed {
                    self.changed();
                }
            }
            Err(error) => self.set_notice(
                format!(
                    "Learned a control for {}, but the map was not saved: {error}",
                    param_label(&target)
                ),
                true,
            ),
        }
    }

    /// Loads `file` as the controller map. The preference is stored only when the file loads.
    pub fn midi_load_map(&mut self, file: PathBuf) {
        let Some(midi) = self.midi.clone() else {
            return self.set_notice("MIDI is not available", true);
        };
        match midi.load_map(&file) {
            Ok(()) => {
                let shown = file.display().to_string();
                let changed =
                    MidiSettingsPage::new(&mut self.prefs, &Listed(&self.lists.midi_inputs))
                        .set_map_file(Some(file));
                self.set_notice(format!("Loaded the map {shown}"), false);
                if changed {
                    self.changed();
                }
            }
            Err(error) => self.set_notice(format!("Map not loaded: {error}"), true),
        }
    }

    /// Loads the map the preferences name (or the default one) into the running service.
    pub fn midi_load_current_map(&mut self) {
        let Some(midi) = self.midi.clone() else {
            return self.set_notice("MIDI is not available", true);
        };
        let shown = self.prefs.effective_map_file().display().to_string();
        let inputs = Listed(&self.lists.midi_inputs);
        let page = MidiSettingsPage::new(&mut self.prefs, &inputs);
        match page.load_map_now(midi.as_ref()) {
            Ok(()) => self.set_notice(format!("Loaded the map {shown}"), false),
            Err(error) => self.set_notice(format!("Map not loaded: {error}"), true),
        }
    }

    /// Back to the default map file.
    pub fn midi_use_default_map(&mut self) {
        let changed = MidiSettingsPage::new(&mut self.prefs, &Listed(&self.lists.midi_inputs))
            .set_map_file(None);
        if changed {
            self.changed();
        }
        self.set_notice("The default map file is used from the next load", false);
    }

    // Library page.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_view::fixtures::*;
    use om_ui::settings::MidiHost;
    use std::cell::Cell;
    use std::path::Path;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;

    /// A MIDI service that records loads and learn requests without touching a device.
    #[derive(Default)]
    struct FakeMidi {
        inputs: Vec<String>,
        /// Devices plugged in after the start.
        plugged: Mutex<Vec<String>>,
        learning: Mutex<Option<String>>,
        generation: AtomicU64,
        fail_load: bool,
        fail_save: bool,
        saved: Mutex<Vec<PathBuf>>,
    }

    impl MidiInputs for FakeMidi {
        fn input_names(&self) -> Vec<String> {
            let mut names = self.inputs.clone();
            names.extend(self.plugged.lock().unwrap().iter().cloned());
            names
        }
    }

    impl MidiHost for FakeMidi {
        fn load_map(&self, _file: &Path) -> Result<(), SettingsError> {
            if self.fail_load {
                return Err(SettingsError::Midi("not a controller map".into()));
            }
            self.generation.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn arm_learn(&self, target: &str) {
            *self.learning.lock().unwrap() = Some(target.to_owned());
        }
        fn cancel_learn(&self) {
            *self.learning.lock().unwrap() = None;
        }
        fn is_learning(&self) -> bool {
            self.learning.lock().unwrap().is_some()
        }
        fn save_map(&self, file: &Path) -> Result<(), SettingsError> {
            if self.fail_save {
                return Err(SettingsError::Midi("disk full".into()));
            }
            self.saved.lock().unwrap().push(file.to_path_buf());
            Ok(())
        }
        fn map_generation(&self) -> u64 {
            self.generation.load(Ordering::SeqCst)
        }
        fn enable_inputs(&self, _enable_all: bool, _inputs: &[String]) {}
    }

    #[test]
    fn a_controller_plugged_in_while_the_midi_page_shows_appears_within_a_second() {
        let fake = Arc::new(FakeMidi::default());
        let midi: Arc<dyn MidiPage> = fake.clone();
        let mut s = SettingsState::new(options(
            Some(midi),
            None,
            None,
            Rc::new(Cell::new(0)),
            Rc::new(Cell::new(0)),
        ));
        let midi_tab = s.panel.tabs().iter().position(|t| t == "MIDI").unwrap();
        let t0 = Instant::now();

        let other_tab = s.panel.tabs().iter().position(|t| t != "MIDI").unwrap();
        s.panel.show(other_tab);
        fake.plugged.lock().unwrap().push("Pad".into());
        assert!(!s.poll_midi_inputs(t0), "another page is showing");
        assert!(s.lists.midi_inputs.is_empty());

        s.panel.show(midi_tab);
        assert!(s.poll_midi_inputs(t0));
        assert_eq!(s.lists.midi_inputs, vec!["Pad".to_owned()]);

        fake.plugged.lock().unwrap().push("Deck".into());
        assert!(
            !s.poll_midi_inputs(t0 + Duration::from_millis(500)),
            "too soon"
        );
        assert!(s.poll_midi_inputs(t0 + MIDI_POLL));
        assert_eq!(
            s.lists.midi_inputs,
            vec!["Pad".to_owned(), "Deck".to_owned()]
        );
        assert!(!s.poll_midi_inputs(t0 + 2 * MIDI_POLL), "nothing changed");
    }

    #[test]
    fn midi_learn_says_what_it_waits_for_and_what_it_got() {
        let fake = Arc::new(FakeMidi {
            inputs: vec!["Controller".into()],
            ..FakeMidi::default()
        });
        let midi: Arc<dyn MidiPage> = fake.clone();
        let changes = Rc::new(Cell::new(0));
        let mut s = SettingsState::new(options(
            Some(midi),
            None,
            None,
            Rc::new(Cell::new(0)),
            Rc::clone(&changes),
        ));
        s.refresh_lists();
        assert_eq!(s.lists.midi_inputs, vec!["Controller".to_owned()]);

        s.midi_learn("deck.0.play");
        assert_eq!(s.learning_target(), Some("deck.0.play"));
        assert!(s.notice.text.starts_with("Learning Deck 1 Play"));

        // The service learns a control: the map changes and the learn ends.
        *fake.learning.lock().unwrap() = None;
        fake.generation.fetch_add(1, Ordering::SeqCst);
        s.poll_learn();
        assert_eq!(s.learning_target(), None);
        assert!(s
            .notice
            .text
            .starts_with("Learned a control for Deck 1 Play and saved to "));
        assert_eq!(changes.get(), 1, "the saved map file is the preference");

        s.midi_learn("deck.0.play");
        s.midi_cancel_learn();
        assert_eq!(s.learning_target(), None);
        assert_eq!(s.notice.text, "Learn cancelled");
        assert_eq!(changes.get(), 1, "arming and cancelling edit no preference");
    }

    #[test]
    fn a_learned_binding_is_saved_and_its_file_becomes_the_map_in_use() {
        let fake = Arc::new(FakeMidi {
            inputs: vec!["Controller".into()],
            ..FakeMidi::default()
        });
        let midi: Arc<dyn MidiPage> = fake.clone();
        let changes = Rc::new(Cell::new(0));
        let mut s = SettingsState::new(options(
            Some(midi),
            None,
            None,
            Rc::new(Cell::new(0)),
            Rc::clone(&changes),
        ));
        s.refresh_lists();
        s.midi_learn("deck.0.play");
        *fake.learning.lock().unwrap() = None;
        fake.generation.fetch_add(1, Ordering::SeqCst);
        s.poll_learn();

        let saved = fake.saved.lock().unwrap().clone();
        assert_eq!(saved, vec![om_ui::preferences::default_user_map_file()]);
        assert_eq!(s.prefs.midi.map_file, saved[0].to_string_lossy());
        assert!(!s.notice.error);
        assert!(s.notice.text.contains("saved to"));
        assert_eq!(changes.get(), 1);
    }

    #[test]
    fn a_learned_binding_that_cannot_be_saved_says_so_and_keeps_the_preference() {
        let fake = Arc::new(FakeMidi {
            inputs: vec!["Controller".into()],
            fail_save: true,
            ..FakeMidi::default()
        });
        let midi: Arc<dyn MidiPage> = fake.clone();
        let changes = Rc::new(Cell::new(0));
        let mut s = SettingsState::new(options(
            Some(midi),
            None,
            None,
            Rc::new(Cell::new(0)),
            Rc::clone(&changes),
        ));
        s.refresh_lists();
        s.midi_learn("deck.0.play");
        *fake.learning.lock().unwrap() = None;
        fake.generation.fetch_add(1, Ordering::SeqCst);
        s.poll_learn();

        assert!(s.notice.error);
        assert_eq!(
            s.notice.text,
            "Learned a control for Deck 1 Play, but the map was not saved: disk full"
        );
        assert!(s.prefs.midi.map_file.is_empty());
        assert_eq!(changes.get(), 0);
    }

    #[test]
    fn a_map_file_is_stored_only_when_it_loads_and_the_text_field_drives_it() {
        let fake: Arc<dyn MidiPage> = Arc::new(FakeMidi {
            fail_load: true,
            ..FakeMidi::default()
        });
        let changes = Rc::new(Cell::new(0));
        let mut s = SettingsState::new(options(
            Some(fake),
            None,
            None,
            Rc::new(Cell::new(0)),
            Rc::clone(&changes),
        ));
        s.begin_entry(EntryKind::MapFile);
        s.type_text("/maps/bad.json");
        s.submit_entry();
        assert!(s.notice.error);
        assert!(s.prefs.midi.map_file.is_empty());
        assert_eq!(changes.get(), 0);

        let good: Arc<dyn MidiPage> = Arc::new(FakeMidi::default());
        let mut s = SettingsState::new(options(
            Some(good),
            None,
            None,
            Rc::new(Cell::new(0)),
            Rc::clone(&changes),
        ));
        s.begin_entry(EntryKind::MapFile);
        s.type_text("/maps/good.json");
        s.backspace();
        s.type_text("n");
        s.submit_entry();
        assert_eq!(s.prefs.midi.map_file, "/maps/good.json");
        assert_eq!(changes.get(), 1);
        assert!(s.entry.is_none());
    }
}
