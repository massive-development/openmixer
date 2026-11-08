//! CLAP consent on the Plugins tab (ADR 0007, decisions 1 and 6): the first-run question (may the
//! standard CLAP folders be scanned?) and the first-load warning (a CLAP plugin runs inside this
//! process). The questions are alert dialogs (`overlay.rs`); the answers are stored in the
//! preferences file, so each is asked once. This file holds what an answer does and the section of
//! the Plugins page that shows and changes the consent later.

use super::chips::*;
use super::overlay::{Answer, Ask};
use super::*;
use om_ui::preferences::ClapScan;

impl SettingsState {
    /// The CLAP question that is due, if any. Only in builds with CLAP support and only when the
    /// window asks (`SettingsOptions::ask_clap`). The first-run question comes first; the warning is
    /// due when `.clap` files wait for it. A question that was dismissed or cancelled is not asked
    /// again in this run, until [`Self::clap_ask_again`].
    pub(crate) fn clap_ask_due(&self) -> Option<Ask> {
        if !self.ask_clap || !self.lists.clap.supported {
            return None;
        }
        let plugins = &self.prefs.plugins;
        if plugins.clap_scan == ClapScan::Unasked && !self.clap_scan_deferred {
            return Some(Ask::ClapFirstRun);
        }
        let waiting = self.lists.clap.waiting > 0;
        if !plugins.clap_warning_accepted && waiting && !self.clap_warning_deferred {
            return Some(Ask::ClapFirstLoad);
        }
        None
    }

    /// A CLAP dialog ended. Answers are stored and applied (the plugin folder is scanned again);
    /// leaving the first-run dialog without an answer asks again next start, and cancelling the
    /// warning leaves the plugins unloaded until the user asks for the warning again.
    pub(crate) fn clap_answered(&mut self, ask: Ask, answer: Answer) {
        match (ask, answer) {
            (Ask::ClapFirstRun, Answer::Ok) => {
                self.prefs.plugins.clap_scan = ClapScan::Allowed;
                self.clap_consent_changed("Scanning the standard CLAP folders");
            }
            (Ask::ClapFirstRun, Answer::Cancel) => {
                self.prefs.plugins.clap_scan = ClapScan::Declined;
                self.clap_consent_changed("Only the plugin folder is scanned for CLAP plugins");
            }
            (Ask::ClapFirstRun, Answer::Dismissed) => self.clap_scan_deferred = true,
            (Ask::ClapFirstLoad, Answer::Ok) => {
                self.prefs.plugins.clap_warning_accepted = true;
                self.clap_consent_changed("Loading CLAP plugins");
            }
            (Ask::ClapFirstLoad, Answer::Cancel | Answer::Dismissed) => {
                self.clap_warning_deferred = true;
                self.set_notice(
                    "CLAP plugins were not loaded. Review the warning on the Plugins tab to load them.",
                    false,
                );
            }
            _ => {}
        }
    }

    /// The Plugins tab's switch for the standard folders. Turning it on is the first-run answer
    /// given later; turning it off stops scanning them (plugins already loaded stay until reload).
    pub fn clap_set_scan(&mut self, allow: bool) {
        self.prefs.plugins.clap_scan = if allow {
            ClapScan::Allowed
        } else {
            ClapScan::Declined
        };
        self.clap_consent_changed(if allow {
            "Scanning the standard CLAP folders"
        } else {
            "Only the plugin folder is scanned for CLAP plugins"
        });
    }

    /// Asks the CLAP questions again that were dismissed or cancelled in this run.
    pub fn clap_ask_again(&mut self) {
        self.clap_scan_deferred = false;
        self.clap_warning_deferred = false;
        self.refresh_lists();
    }

    /// Stores and applies the consent, then reads the plugin lists again and says what was found.
    fn clap_consent_changed(&mut self, done: &str) {
        self.changed();
        self.refresh_lists();
        let (files, waiting) = (self.lists.clap.files.len(), self.lists.clap.waiting);
        let tail = match (files, waiting) {
            (0, _) => "no .clap file found".to_owned(),
            (n, 0) => format!("{n} .clap file(s) found"),
            (n, w) => format!("{n} .clap file(s) found, {w} waiting for the warning"),
        };
        self.set_notice(format!("{done}: {tail}"), false);
    }
}

/// The CLAP part of the Plugins page: the standard-folder switch, the warning, the files found.
/// `None` in a build without CLAP support.
pub(super) fn clap_section(
    t: &ThemeTokens,
    settings: &SettingsState,
    cx: &mut Context<Shell>,
) -> Option<Div> {
    let clap = &settings.lists.clap;
    if !clap.supported {
        return None;
    }
    let plugins = &settings.prefs.plugins;
    let scanning = plugins.scan_standard_folders();
    let mut files = div().flex().flex_col();
    if clap.files.is_empty() {
        files = files.child(note(t, "No .clap file found."));
    }
    for file in &clap.files {
        files = files.child(div().overflow_hidden().child(file.clone()));
    }
    let mut chips = vec![chip(
        t,
        if scanning {
            "Standard folders: scanned"
        } else {
            "Standard folders: not scanned"
        },
        scanning,
        cx,
        move |this| {
            this.settings_mut(|s| s.clap_set_scan(!scanning));
            this.refresh_theme();
        },
    )];
    let review = clap.waiting > 0 && !plugins.clap_warning_accepted;
    if review {
        chips.push(chip(t, "Review the warning", false, cx, |this| {
            this.settings_mut(|s| s.clap_ask_again());
        }));
    }
    let state = if plugins.clap_warning_accepted {
        "The in-process warning was accepted."
    } else {
        "CLAP plugins are not loaded until you accept the in-process warning."
    };
    Some(
        div()
            .flex()
            .flex_col()
            .child(choice_row(t, "CLAP plugins", row_of(chips)))
            .child(choice_row(t, "CLAP files found", files))
            .child(note(
                t,
                &format!(
                    "{state} Scanning the standard folders ({}) opens each file, which runs the plugin's code.",
                    clap.standard_folders.join(", ")
                ),
            )),
    )
}
