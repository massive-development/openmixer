//! The Automix tab: enable, style, transition length, queue source, smart picks, tempo gap and
//! autostart. The logic is om-ui's `AutomixSettingsPage`; this file draws the chips and forwards
//! their clicks to the methods on [`SettingsState`] below.

use super::chips::*;
use super::*;
use om_ui::settings::{AutomixSettingsPage, QueueChoice, LENGTH_CHOICES, STYLE_LABELS};

/// Minimum tempo gaps the page offers, in percent of the current BPM.
const GAP_CHOICES: [i32; 5] = [0, 3, 6, 10, 20];

/// Stands in for the automix service when there is none: settings are stored, nothing runs.
pub(super) struct NoAutomix;

impl AutomixControl for NoAutomix {
    fn available(&self) -> bool {
        false
    }

    fn apply(&mut self, _: &om_ui::preferences::AutomixPrefs) -> SettingsResult<()> {
        Ok(())
    }
}

fn on_off(on: bool) -> &'static str {
    if on {
        "On"
    } else {
        "Off"
    }
}

pub(super) fn automix_page(
    t: &ThemeTokens,
    settings: &SettingsState,
    cx: &mut Context<Shell>,
) -> Div {
    let a = &settings.prefs.automix;
    let mut column = div().flex().flex_col();
    if !settings.automix_available() {
        column = column.child(note(
            t,
            "The library is not open, so automix stays idle. Settings are still saved.",
        ));
    }

    let enabled = a.enabled;
    column = column.child(choice_row(
        t,
        "Automix",
        row_of(vec![chip(t, on_off(enabled), enabled, cx, move |this| {
            this.settings_mut(|s| s.automix_set_enabled(!enabled))
        })]),
    ));

    let styles: Vec<Div> = STYLE_LABELS
        .iter()
        .enumerate()
        .map(|(index, label)| {
            let code = index as i32;
            chip(t, label, a.style == code, cx, move |this| {
                this.settings_mut(|s| s.automix_set_style(code))
            })
        })
        .collect();
    column = column.child(choice_row(t, "Style", row_of(styles)));

    let lengths: Vec<Div> = LENGTH_CHOICES
        .iter()
        .map(|&beats| {
            chip(
                t,
                &format!("{beats} beats"),
                a.length_beats == beats,
                cx,
                move |this| this.settings_mut(|s| s.automix_set_length(i64::from(beats))),
            )
        })
        .collect();
    column = column.child(choice_row(t, "Transition length", row_of(lengths)));

    let mut sources = vec![
        chip(
            t,
            "Whole library",
            !a.queue_search && a.queue_playlist == 0,
            cx,
            |this| this.settings_mut(|s| s.automix_set_queue(QueueChoice::WholeLibrary)),
        ),
        chip(t, "Current search", a.queue_search, cx, |this| {
            this.settings_mut(|s| s.automix_set_queue(QueueChoice::CurrentSearch))
        }),
    ];
    for (id, name) in &settings.lists.playlists {
        let id = *id;
        let lit = !a.queue_search && a.queue_playlist == id;
        sources.push(chip(t, name, lit, cx, move |this| {
            this.settings_mut(|s| s.automix_set_queue(QueueChoice::Playlist(id)))
        }));
    }
    column = column.child(choice_row(t, "Queue from", row_of(sources)));

    let smart = a.smart;
    column = column.child(choice_row(
        t,
        "Smart picks (key and tempo)",
        row_of(vec![chip(t, on_off(smart), smart, cx, move |this| {
            this.settings_mut(|s| s.automix_set_smart(!smart))
        })]),
    ));

    let gaps: Vec<Div> = GAP_CHOICES
        .iter()
        .map(|&percent| {
            chip(
                t,
                &format!("{percent}%"),
                a.min_bpm_gap_percent == percent,
                cx,
                move |this| this.settings_mut(|s| s.automix_set_bpm_gap(percent)),
            )
        })
        .collect();
    column = column.child(choice_row(t, "Largest tempo gap", row_of(gaps)));

    let autostart = a.autostart_at_end;
    column = column.child(choice_row(
        t,
        "Start the next track at the end",
        row_of(vec![chip(
            t,
            on_off(autostart),
            autostart,
            cx,
            move |this| this.settings_mut(|s| s.automix_set_autostart(!autostart)),
        )]),
    ));
    column.child(note(
        t,
        "Changes apply to the next transition. A playlist plays in order; the other sources shuffle and skip tracks played recently.",
    ))
}

impl SettingsState {
    pub fn automix_available(&self) -> bool {
        self.automix.as_ref().is_some_and(|a| a.available())
    }

    /// Runs `f` on the Automix page over the service (or the stand-in), then saves and applies the
    /// preferences when the page reports a change. A failure is shown on the status line.
    fn automix_edit(
        &mut self,
        f: impl FnOnce(&mut AutomixSettingsPage<'_>) -> SettingsResult<bool>,
    ) {
        let outcome = match self.automix.as_deref_mut() {
            Some(control) => f(&mut AutomixSettingsPage::new(&mut self.prefs, control)),
            None => f(&mut AutomixSettingsPage::new(
                &mut self.prefs,
                &mut NoAutomix,
            )),
        };
        match outcome {
            Ok(true) => self.changed(),
            Ok(false) => {}
            Err(error) => self.set_notice(error.to_string(), true),
        }
    }

    pub fn automix_set_enabled(&mut self, on: bool) {
        self.automix_edit(|p| p.set_enabled(on));
    }

    pub fn automix_set_style(&mut self, style: i32) {
        self.automix_edit(|p| p.set_style(style));
    }

    pub fn automix_set_length(&mut self, beats: i64) {
        self.automix_edit(|p| p.set_length_beats(beats));
    }

    pub fn automix_set_queue(&mut self, choice: QueueChoice) {
        self.automix_edit(|p| p.set_queue(choice));
    }

    pub fn automix_set_smart(&mut self, on: bool) {
        self.automix_edit(|p| p.set_smart(on));
    }

    pub fn automix_set_bpm_gap(&mut self, percent: i32) {
        self.automix_edit(|p| p.set_min_bpm_gap_percent(percent));
    }

    pub fn automix_set_autostart(&mut self, on: bool) {
        self.automix_edit(|p| p.set_autostart_at_end(on));
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures;
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    #[derive(Default)]
    struct Service {
        applies: Rc<Cell<usize>>,
    }
    impl AutomixControl for Service {
        fn apply(&mut self, _: &om_ui::preferences::AutomixPrefs) -> SettingsResult<()> {
            self.applies.set(self.applies.get() + 1);
            Ok(())
        }
        fn playlists(&self) -> SettingsResult<Vec<(i64, String)>> {
            Ok(vec![(4, "Warm-up".into())])
        }
    }

    #[test]
    fn automix_edits_store_apply_and_save_once_per_change() {
        let changes = Rc::new(RefCell::new(0usize));
        let applies = Rc::new(Cell::new(0usize));
        let mut state = fixtures::state(None, Rc::clone(&changes));
        state.automix = Some(Box::new(Service {
            applies: Rc::clone(&applies),
        }));
        state.refresh_lists();
        assert_eq!(state.lists.playlists, vec![(4, "Warm-up".to_owned())]);
        state.automix_set_enabled(true);
        state.automix_set_enabled(true);
        state.automix_set_style(2);
        state.automix_set_queue(QueueChoice::Playlist(4));
        assert!(state.prefs.automix.enabled);
        assert_eq!(state.prefs.automix.style, 2);
        assert_eq!(state.prefs.automix.queue_playlist, 4);
        assert_eq!(applies.get(), 3, "the repeated enable is not a change");
        assert_eq!(*changes.borrow(), 3, "each change is saved once");
    }

    #[test]
    fn without_a_service_settings_are_stored_and_the_page_says_idle() {
        let changes = Rc::new(RefCell::new(0usize));
        let mut state = fixtures::state(None, Rc::clone(&changes));
        assert!(!state.automix_available());
        state.automix_set_bpm_gap(10);
        assert_eq!(state.prefs.automix.min_bpm_gap_percent, 10);
        assert_eq!(*changes.borrow(), 1);
    }
}
