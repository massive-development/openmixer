//! Pushes preferences into the subsystems that consume them, at start-up and on every change.
//! Subsystems sit behind [`PreferencesTargets`], so tests use
//! mocks and the app supplies the real glue (`openmixer::app_targets`). Only sections that differ
//! from the last apply are pushed, in dependency order: plugins (they may provide themes), theme,
//! audio, MIDI. Targets must be idempotent.
//!
//! The targets are passed to [`PreferencesApplier::apply`] instead of
//! being held by the applier, so the owner keeps one `&mut` to the targets.

use crate::preferences::{AudioPrefs, MidiPrefs, Preferences};
use std::path::Path;

pub trait PreferencesTargets {
    /// Loads plugins from `folder` and forwards their themes to the registry.
    fn apply_plugin_folder(&mut self, folder: &Path);
    /// `false` when the theme is unknown (the current one is kept).
    fn apply_theme(&mut self, theme_id: &str) -> bool;
    fn apply_audio(&mut self, audio: &AudioPrefs);
    fn apply_midi(&mut self, midi: &MidiPrefs);
}

#[derive(Debug, Default)]
pub struct PreferencesApplier {
    applied: Option<Preferences>,
}

impl PreferencesApplier {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns how many sections were pushed (everything on the first call).
    pub fn apply<T: PreferencesTargets + ?Sized>(
        &mut self,
        targets: &mut T,
        prefs: &Preferences,
    ) -> usize {
        let mut pushed = 0;
        let first = self.applied.is_none();
        let old = self.applied.as_ref().unwrap_or(prefs);

        let plugins_changed =
            first || old.effective_plugin_folder() != prefs.effective_plugin_folder();
        if plugins_changed {
            targets.apply_plugin_folder(&prefs.effective_plugin_folder());
            pushed += 1;
        }
        // A plugin theme may have just appeared or gone.
        if plugins_changed || old.theme != prefs.theme {
            targets.apply_theme(&prefs.theme);
            pushed += 1;
        }
        if first || old.audio != prefs.audio {
            targets.apply_audio(&prefs.audio);
            pushed += 1;
        }
        if first || old.midi != prefs.midi {
            targets.apply_midi(&prefs.midi);
            pushed += 1;
        }
        self.applied = Some(prefs.clone());
        pushed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::{LibraryPrefs, Preferences};

    fn customised() -> Preferences {
        Preferences {
            theme: "openmixer.daylight".into(),
            audio: AudioPrefs {
                device_state: String::new(),
                master_pair: 2,
                cue_pair: -1,
            },
            midi: MidiPrefs {
                enable_all: false,
                enabled_inputs: vec!["usb-1".into(), "usb-2".into()],
                map_file: "/maps/my map.json".into(),
            },
            library: LibraryPrefs {
                scan_on_start: false,
                scan_interval_minutes: 15,
            },
            plugin_folder: "/opt/openmixer/plugins".into(),
            stems: Default::default(),
        }
    }

    #[derive(Default)]
    struct MockTargets {
        calls: Vec<String>,
    }

    impl PreferencesTargets for MockTargets {
        fn apply_plugin_folder(&mut self, folder: &Path) {
            self.calls.push(format!("plugins:{}", folder.display()));
        }
        fn apply_theme(&mut self, id: &str) -> bool {
            self.calls.push(format!("theme:{id}"));
            true
        }
        fn apply_audio(&mut self, a: &AudioPrefs) {
            self.calls
                .push(format!("audio:{},{}", a.master_pair, a.cue_pair));
        }
        fn apply_midi(&mut self, m: &MidiPrefs) {
            self.calls.push(format!(
                "midi:{}",
                if m.enable_all {
                    "all".to_owned()
                } else {
                    m.enabled_inputs.join("+")
                }
            ));
        }
    }

    #[test]
    fn first_apply_pushes_every_section_in_dependency_order() {
        let mut targets = MockTargets::default();
        let mut applier = PreferencesApplier::new();
        assert_eq!(applier.apply(&mut targets, &customised()), 4);
        assert_eq!(
            targets.calls,
            vec![
                "plugins:/opt/openmixer/plugins",
                "theme:openmixer.daylight",
                "audio:2,-1",
                "midi:usb-1+usb-2",
            ]
        );
    }

    #[test]
    fn unchanged_preferences_push_nothing() {
        let mut targets = MockTargets::default();
        let mut applier = PreferencesApplier::new();
        applier.apply(&mut targets, &customised());
        targets.calls.clear();
        assert_eq!(applier.apply(&mut targets, &customised()), 0);
        assert!(targets.calls.is_empty());
    }

    #[test]
    fn only_the_changed_section_is_pushed() {
        let mut targets = MockTargets::default();
        let mut applier = PreferencesApplier::new();
        let mut p = customised();
        applier.apply(&mut targets, &p);

        targets.calls.clear();
        p.audio.cue_pair = 1;
        assert_eq!(applier.apply(&mut targets, &p), 1);
        assert_eq!(targets.calls.join(";"), "audio:2,1");

        targets.calls.clear();
        p.midi.enable_all = true;
        applier.apply(&mut targets, &p);
        assert_eq!(targets.calls.join(";"), "midi:all");

        targets.calls.clear();
        p.theme = "openmixer.dark".into();
        p.library.scan_interval_minutes = 60; // read at library start, nothing to push
        applier.apply(&mut targets, &p);
        assert_eq!(targets.calls.join(";"), "theme:openmixer.dark");
    }

    #[test]
    fn changing_the_plugin_folder_reselects_the_theme() {
        let mut targets = MockTargets::default();
        let mut applier = PreferencesApplier::new();
        let mut p = customised();
        applier.apply(&mut targets, &p);
        targets.calls.clear();
        p.plugin_folder = "/elsewhere".into();
        assert_eq!(applier.apply(&mut targets, &p), 2);
        assert_eq!(
            targets.calls.join(";"),
            "plugins:/elsewhere;theme:openmixer.daylight"
        );
    }

    #[test]
    fn empty_folder_means_the_default_folder_and_does_not_repeat() {
        let mut targets = MockTargets::default();
        let mut applier = PreferencesApplier::new();
        let mut p = Preferences::default();
        applier.apply(&mut targets, &p);
        assert_eq!(
            targets.calls[0],
            format!(
                "plugins:{}",
                crate::preferences::default_plugin_folder().display()
            )
        );
        targets.calls.clear();
        p.plugin_folder = crate::preferences::default_plugin_folder()
            .to_string_lossy()
            .into_owned(); // same folder, spelled out
        assert_eq!(applier.apply(&mut targets, &p), 0);
    }
}
