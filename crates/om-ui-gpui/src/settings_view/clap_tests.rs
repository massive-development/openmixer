//! The CLAP questions' logic, without a window: when each is due, what each answer stores and does.

use super::overlay::{Answer, Ask};
use super::*;
use crate::settings_view::fixtures::*;
use om_ui::preferences::ClapScan;
use om_ui::settings::{ClapStatus, PluginHost, SettingsResult};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// A plugin host with CLAP support that reports a shared status.
pub(crate) struct ClapHost {
    pub status: Rc<RefCell<ClapStatus>>,
}

impl PluginHost for ClapHost {
    fn loaded_plugins(&self) -> Vec<String> {
        Vec::new()
    }
    fn reload(&mut self) -> SettingsResult<()> {
        Ok(())
    }
    fn clap_status(&self) -> ClapStatus {
        self.status.borrow().clone()
    }
}

pub(crate) fn supported(waiting: usize) -> Rc<RefCell<ClapStatus>> {
    Rc::new(RefCell::new(ClapStatus {
        supported: true,
        standard_folders: vec!["/home/u/.clap".into(), "/usr/lib/clap".into()],
        files: vec!["/plugins/gain.clap".into()],
        waiting,
    }))
}

/// Settings over a `ClapHost` that asks for consent. `on_changed` mimics the app: accepting the
/// warning loads what was waiting. Returns the state and the number of saves.
pub(crate) fn asking(status: &Rc<RefCell<ClapStatus>>) -> (SettingsState, Rc<Cell<usize>>) {
    let saves = Rc::new(Cell::new(0));
    let mut o = options(
        None,
        None,
        Some(Box::new(ClapHost {
            status: Rc::clone(status),
        })),
        Rc::new(Cell::new(0)),
        Rc::clone(&saves),
    );
    o.ask_clap = true;
    let (seen, status) = (Rc::clone(&saves), Rc::clone(status));
    o.on_changed = Box::new(move |prefs| {
        seen.set(seen.get() + 1);
        if prefs.plugins.clap_warning_accepted {
            status.borrow_mut().waiting = 0;
        }
    });
    (SettingsState::new(o), saves)
}

#[test]
fn nothing_is_asked_without_clap_support_or_when_the_window_does_not_ask() {
    let (mut s, _) = asking(&Rc::new(RefCell::new(ClapStatus::default())));
    assert_eq!(s.clap_ask_due(), None, "no CLAP in this build");
    let (mut quiet, _) = asking(&supported(1));
    quiet.ask_clap = false;
    assert_eq!(quiet.clap_ask_due(), None, "demo and smoke runs do not ask");
    s.ask_clap = true;
    assert_eq!(s.clap_ask_due(), None);
}

#[test]
fn the_first_run_prompt_comes_first_and_both_answers_are_stored() {
    let status = supported(1);
    let (mut s, saves) = asking(&status);
    assert_eq!(s.clap_ask_due(), Some(Ask::ClapFirstRun));

    s.clap_answered(Ask::ClapFirstRun, Answer::Ok);
    assert_eq!(s.prefs.plugins.clap_scan, ClapScan::Allowed);
    assert_eq!(saves.get(), 1, "the answer was saved and applied once");
    assert!(!s.notice.error && s.notice.text.contains("Scanning"));
    // The warning is due next: a file waits for it.
    assert_eq!(s.clap_ask_due(), Some(Ask::ClapFirstLoad));

    let (mut declined, saves) = asking(&status);
    declined.clap_answered(Ask::ClapFirstRun, Answer::Cancel);
    assert_eq!(declined.prefs.plugins.clap_scan, ClapScan::Declined);
    assert_eq!(saves.get(), 1);
    assert!(!declined.prefs.plugins.scan_standard_folders());
}

#[test]
fn dismissing_the_first_run_prompt_stores_nothing_and_asks_again_next_time() {
    let (mut s, saves) = asking(&supported(0));
    s.clap_answered(Ask::ClapFirstRun, Answer::Dismissed);
    assert_eq!(s.prefs.plugins.clap_scan, ClapScan::Unasked);
    assert_eq!(saves.get(), 0);
    assert_eq!(s.clap_ask_due(), None, "not again in this run");
    // A new run (a new state over the same preferences) is asked again.
    let (fresh, _) = asking(&supported(0));
    assert_eq!(fresh.clap_ask_due(), Some(Ask::ClapFirstRun));
}

#[test]
fn the_first_load_warning_loads_on_accept_and_waits_on_cancel() {
    let status = supported(1);
    let (mut s, saves) = asking(&status);
    s.prefs.plugins.clap_scan = ClapScan::Declined;
    s.refresh_lists();
    assert_eq!(s.clap_ask_due(), Some(Ask::ClapFirstLoad));

    s.clap_answered(Ask::ClapFirstLoad, Answer::Cancel);
    assert!(!s.prefs.plugins.clap_warning_accepted);
    assert_eq!(saves.get(), 0, "cancelling stores nothing");
    assert_eq!(s.clap_ask_due(), None);
    assert!(s.notice.text.contains("not loaded"));

    // The Plugins tab can bring the question back.
    s.clap_ask_again();
    assert_eq!(s.clap_ask_due(), Some(Ask::ClapFirstLoad));

    s.clap_answered(Ask::ClapFirstLoad, Answer::Ok);
    assert!(s.prefs.plugins.clap_warning_accepted);
    assert_eq!(saves.get(), 1);
    assert_eq!(status.borrow().waiting, 0, "the app loaded what waited");
    assert_eq!(s.clap_ask_due(), None, "accepted once, never asked again");
    assert!(
        s.notice.text.contains("1 .clap file(s) found"),
        "{}",
        s.notice.text
    );
}

#[test]
fn the_standard_folder_switch_is_the_first_run_answer_given_later() {
    let (mut s, saves) = asking(&supported(0));
    s.clap_set_scan(true);
    assert!(s.prefs.plugins.scan_standard_folders());
    s.clap_set_scan(false);
    assert_eq!(s.prefs.plugins.clap_scan, ClapScan::Declined);
    assert_eq!(saves.get(), 2);
    assert_eq!(s.clap_ask_due(), None, "declined is an answer");
}

#[test]
fn the_dialog_texts_name_the_folders_and_the_risk() {
    let status = supported(2);
    let clap = status.borrow();
    let first_run = Ask::ClapFirstRun.body(&clap);
    assert!(
        first_run.contains("/home/u/.clap and /usr/lib/clap"),
        "{first_run}"
    );
    assert!(first_run.contains("runs code"), "{first_run}");
    let first_load = Ask::ClapFirstLoad.body(&clap);
    assert!(
        first_load.contains("2 CLAP plugin files are waiting"),
        "{first_load}"
    );
    assert!(first_load.contains("takes the whole app"), "{first_load}");
}
