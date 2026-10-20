//! The Automix page: enable, style, transition length, queue source, smart picks, tempo gap and
//! autostart. Each edit is stored in the `automix` preferences section and then pushed to the
//! running controller through [`AutomixControl::apply`], so a change takes effect without a
//! restart. The app implements the trait over its automix service and queue.

use super::{set_if_changed, SettingsError, SettingsResult};
use crate::preferences::{
    snap_length_beats, AutomixPrefs, Preferences, MAX_AUTOMIX_STYLE, MAX_BPM_GAP_PERCENT,
};

/// Style labels in `automix.style` order (0 = Auto).
pub const STYLE_LABELS: [&str; 6] = [
    "Auto",
    "Crossfade",
    "Bass swap",
    "Echo out",
    "Cut",
    "Stem bridge",
];

/// Transition lengths the page offers, in beats.
pub const LENGTH_CHOICES: [i32; 3] = [8, 16, 32];

/// Where the automix queue picks from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueChoice {
    /// Every analysed track in the library, shuffled.
    WholeLibrary,
    /// A playlist (or crate), in its order, wrapping at the end.
    Playlist(i64),
    /// The library browser's current search, shuffled. Follows the browser live.
    CurrentSearch,
}

/// What the Automix page needs from the app.
pub trait AutomixControl {
    /// False when no library or analysis host is running: settings are still stored, but automix
    /// stays idle until a host is there.
    fn available(&self) -> bool {
        true
    }
    /// Playlists the queue can play, as `(id, name)` in the library's order.
    fn playlists(&self) -> SettingsResult<Vec<(i64, String)>> {
        Ok(Vec::new())
    }
    /// Pushes the running flag. An Enable press calls it even when the stored preference already
    /// has that value, because the controller may have turned itself off since.
    fn set_running(&mut self, _on: bool) -> SettingsResult<()> {
        Ok(())
    }
    /// Pushes the stored settings to the controller and the queue. Called after every change.
    fn apply(&mut self, prefs: &AutomixPrefs) -> SettingsResult<()>;
}

/// Automix page logic over the shared preferences.
pub struct AutomixSettingsPage<'a> {
    prefs: &'a mut Preferences,
    control: &'a mut dyn AutomixControl,
}

impl<'a> AutomixSettingsPage<'a> {
    pub fn new(prefs: &'a mut Preferences, control: &'a mut dyn AutomixControl) -> Self {
        Self { prefs, control }
    }

    pub fn settings(&self) -> &AutomixPrefs {
        &self.prefs.automix
    }

    pub fn available(&self) -> bool {
        self.control.available()
    }

    /// The queue source as the page shows it.
    pub fn queue_choice(&self) -> QueueChoice {
        let a = &self.prefs.automix;
        if a.queue_search {
            QueueChoice::CurrentSearch
        } else if a.queue_playlist > 0 {
            QueueChoice::Playlist(a.queue_playlist)
        } else {
            QueueChoice::WholeLibrary
        }
    }

    pub fn playlists(&self) -> SettingsResult<Vec<(i64, String)>> {
        self.control.playlists()
    }

    /// The Enable preference is the launch default; the running flag follows the controller, and
    /// an explicit press sets it directly.
    pub fn set_enabled(&mut self, on: bool) -> SettingsResult<bool> {
        self.control.set_running(on)?;
        self.edit(|a| set_if_changed(&mut a.enabled, on))
    }

    /// `style` is an `automix.style` code, `0..=MAX_AUTOMIX_STYLE`.
    pub fn set_style(&mut self, style: i32) -> SettingsResult<bool> {
        if !(0..=MAX_AUTOMIX_STYLE as i32).contains(&style) {
            return Err(SettingsError::InvalidValue { field: "style" });
        }
        self.edit(|a| set_if_changed(&mut a.style, style))
    }

    /// Snaps to 8, 16 or 32 beats, as the preferences reader does.
    pub fn set_length_beats(&mut self, beats: i64) -> SettingsResult<bool> {
        let snapped = snap_length_beats(beats);
        self.edit(|a| set_if_changed(&mut a.length_beats, snapped))
    }

    pub fn set_queue(&mut self, choice: QueueChoice) -> SettingsResult<bool> {
        self.edit(|a| {
            let (search, playlist) = match choice {
                QueueChoice::WholeLibrary => (false, 0),
                QueueChoice::Playlist(id) => (false, id.max(0)),
                QueueChoice::CurrentSearch => (true, a.queue_playlist),
            };
            let s = set_if_changed(&mut a.queue_search, search);
            let p = set_if_changed(&mut a.queue_playlist, playlist);
            s | p
        })
    }

    pub fn set_smart(&mut self, on: bool) -> SettingsResult<bool> {
        self.edit(|a| set_if_changed(&mut a.smart, on))
    }

    /// Clamped to `0..=MAX_BPM_GAP_PERCENT`.
    pub fn set_min_bpm_gap_percent(&mut self, percent: i32) -> SettingsResult<bool> {
        let clamped = percent.clamp(0, MAX_BPM_GAP_PERCENT as i32);
        self.edit(|a| set_if_changed(&mut a.min_bpm_gap_percent, clamped))
    }

    pub fn set_autostart_at_end(&mut self, on: bool) -> SettingsResult<bool> {
        self.edit(|a| set_if_changed(&mut a.autostart_at_end, on))
    }

    /// Applies `change` to the stored section; when it changed something, pushes the section to
    /// the controller. The stored value stays changed even if the push fails, and the error says so.
    fn edit(&mut self, change: impl FnOnce(&mut AutomixPrefs) -> bool) -> SettingsResult<bool> {
        let changed = change(&mut self.prefs.automix);
        if changed {
            self.control.apply(&self.prefs.automix)?;
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        applies: Vec<AutomixPrefs>,
        running: Vec<bool>,
        playlists: Vec<(i64, String)>,
        fail: bool,
    }
    impl AutomixControl for Fake {
        fn playlists(&self) -> SettingsResult<Vec<(i64, String)>> {
            Ok(self.playlists.clone())
        }
        fn set_running(&mut self, on: bool) -> SettingsResult<()> {
            self.running.push(on);
            Ok(())
        }

        fn apply(&mut self, prefs: &AutomixPrefs) -> SettingsResult<()> {
            self.applies.push(prefs.clone());
            if self.fail {
                Err(SettingsError::StemsNotReady("controller is gone".into()))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn every_change_is_stored_and_pushed_once() {
        let mut prefs = Preferences::default();
        let mut fake = Fake::default();
        {
            let mut page = AutomixSettingsPage::new(&mut prefs, &mut fake);
            assert!(page.set_enabled(true).unwrap());
            assert!(
                !page.set_enabled(true).unwrap(),
                "same value is not a change"
            );
            assert!(page.set_style(3).unwrap());
            assert!(page.set_length_beats(30).unwrap());
            assert!(page.set_smart(false).unwrap());
            assert!(page.set_autostart_at_end(false).unwrap());
        }
        assert_eq!(fake.applies.len(), 5, "one apply per real change");
        let a = &prefs.automix;
        assert!(a.enabled);
        assert_eq!(a.style, 3);
        assert_eq!(a.length_beats, 32);
        assert!(!a.smart);
        assert!(!a.autostart_at_end);
    }

    #[test]
    fn style_out_of_range_is_refused_and_nothing_is_pushed() {
        let mut prefs = Preferences::default();
        let mut fake = Fake::default();
        let mut page = AutomixSettingsPage::new(&mut prefs, &mut fake);
        assert!(matches!(
            page.set_style(5),
            Err(SettingsError::InvalidValue { field: "style" })
        ));
        assert!(page.set_style(-1).is_err());
        assert_eq!(page.settings().style, 0);
        assert!(fake.applies.is_empty());
    }

    #[test]
    fn length_snaps_and_gap_clamps() {
        let mut prefs = Preferences::default();
        let mut fake = Fake::default();
        let mut page = AutomixSettingsPage::new(&mut prefs, &mut fake);
        page.set_length_beats(9).unwrap();
        assert_eq!(page.settings().length_beats, 8);
        page.set_min_bpm_gap_percent(99).unwrap();
        assert_eq!(page.settings().min_bpm_gap_percent, 20);
        page.set_min_bpm_gap_percent(-3).unwrap();
        assert_eq!(page.settings().min_bpm_gap_percent, 0);
    }

    #[test]
    fn queue_choice_round_trips_through_the_stored_fields() {
        let mut prefs = Preferences::default();
        let mut fake = Fake::default();
        {
            let mut page = AutomixSettingsPage::new(&mut prefs, &mut fake);
            assert_eq!(page.queue_choice(), QueueChoice::WholeLibrary);
            page.set_queue(QueueChoice::Playlist(7)).unwrap();
            assert_eq!(page.queue_choice(), QueueChoice::Playlist(7));
            page.set_queue(QueueChoice::CurrentSearch).unwrap();
            assert_eq!(page.queue_choice(), QueueChoice::CurrentSearch);
            assert_eq!(
                page.settings().queue_playlist,
                7,
                "the playlist is kept while the search is chosen"
            );
            page.set_queue(QueueChoice::WholeLibrary).unwrap();
            assert_eq!(page.queue_choice(), QueueChoice::WholeLibrary);
        }
        assert_eq!(prefs.automix.queue_playlist, 0);
        assert!(!prefs.automix.queue_search);
    }

    #[test]
    fn a_failed_push_keeps_the_value_and_reports_the_error() {
        let mut prefs = Preferences::default();
        let mut fake = Fake {
            fail: true,
            ..Default::default()
        };
        let mut page = AutomixSettingsPage::new(&mut prefs, &mut fake);
        assert!(page.set_enabled(true).is_err());
        assert!(page.settings().enabled);
    }

    /// An Enable press sets the running flag even when the stored preference already has that value:
    /// after the controller turned itself off, the preference still says On.
    #[test]
    fn an_enable_press_reaches_the_running_flag_even_when_the_preference_agrees() {
        let mut prefs = Preferences::default();
        prefs.automix.enabled = true;
        let mut fake = Fake::default();
        {
            let mut page = AutomixSettingsPage::new(&mut prefs, &mut fake);
            assert!(!page.set_enabled(true).unwrap(), "stored value unchanged");
        }
        assert_eq!(fake.running, vec![true]);
        assert!(fake.applies.is_empty(), "nothing stored changed, nothing pushed");
    }

    #[test]
    fn playlists_come_from_the_control() {
        let mut prefs = Preferences::default();
        let mut fake = Fake {
            playlists: vec![(3, "Warm-up".into())],
            ..Default::default()
        };
        let page = AutomixSettingsPage::new(&mut prefs, &mut fake);
        assert_eq!(page.playlists().unwrap(), vec![(3, "Warm-up".to_owned())]);
    }
}
