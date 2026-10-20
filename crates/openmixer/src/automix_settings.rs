//! The Automix page's link to the app. [`apply_automix_prefs`] pushes the stored settings into the
//! running automix (the controller config, the style, and the queue settings the host reads before
//! each pick); [`AppAutomixSettings`] is the page's control. The running enable flag follows the
//! controller: the stored `enabled` is the launch default and an Enable press sets the flag directly
//! (`set_running`), so a controller that turned itself off stays off until someone turns it on. The window also
//! calls `apply_automix_prefs` at start-up, so the page and the start-up path cannot disagree.
//!
//! Threads: the window thread only (the library handle is not `Send`).

use crate::automix_queue::{QueueSettings, QueueSource};
use crate::automix_service::AutomixShared;
use om_automix::AutomixConfig;
use om_core::AutomixStyle;
use om_library::{Library, PlaylistDao};
use om_ui::preferences::AutomixPrefs;
use om_ui::settings::{AutomixControl, SettingsResult};
use std::sync::Arc;

/// The controller settings the preferences describe.
pub fn automix_config(a: &AutomixPrefs) -> AutomixConfig {
    AutomixConfig {
        length_beats: (a.length_beats > 0).then_some(f64::from(a.length_beats)),
        max_bpm_ratio: f64::from(a.min_bpm_gap_percent) / 100.0,
        autostart: a.autostart_at_end,
        ..AutomixConfig::default()
    }
}

/// The queue source the preferences describe: the browser's live search, a playlist, or the whole
/// library (an empty search, shuffled).
pub fn queue_source(a: &AutomixPrefs) -> QueueSource {
    if a.queue_search {
        QueueSource::CurrentSearch
    } else if a.queue_playlist > 0 {
        QueueSource::Playlist(a.queue_playlist)
    } else {
        QueueSource::Search(String::new())
    }
}

/// Pushes the stored automix settings to the running service. The running flag belongs to the
/// controller (MIDI and automix itself turn it off), so `enabled` is pushed only at start-up
/// (`previous` is `None`: the preference is the launch default) or when the preference changed.
/// Editing style, smart or the queue never flips it.
pub fn apply_automix_prefs(
    shared: &AutomixShared,
    a: &AutomixPrefs,
    previous: Option<&AutomixPrefs>,
) {
    shared.update_config(automix_config(a));
    let control = shared.control();
    control.set_style(AutomixStyle::from_u8(a.style as u8));
    if previous.is_none_or(|p| p.enabled != a.enabled) {
        control.set_enabled(a.enabled);
    }
    let shared_queue = shared.queue_settings();
    let mut queue = shared_queue.lock().unwrap_or_else(|e| e.into_inner());
    *queue = QueueSettings {
        source: queue_source(a),
        smart: a.smart,
    };
}

/// The Automix page's control: applies changes and lists the library's playlists.
pub struct AppAutomixSettings {
    shared: Arc<AutomixShared>,
    library: Option<Arc<Library>>,
    /// The preferences last pushed (the launch ones at first).
    applied: AutomixPrefs,
}

impl AppAutomixSettings {
    /// `library` is `None` when the library did not open; the page then says automix is idle.
    /// `launch` is the preferences the service started with.
    pub fn new(
        shared: Arc<AutomixShared>,
        library: Option<Arc<Library>>,
        launch: AutomixPrefs,
    ) -> Self {
        Self {
            shared,
            library,
            applied: launch,
        }
    }
}

impl AutomixControl for AppAutomixSettings {
    fn available(&self) -> bool {
        self.library.is_some()
    }

    fn playlists(&self) -> SettingsResult<Vec<(i64, String)>> {
        let Some(library) = &self.library else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        collect_playlists(&library.playlists(), None, "", &mut out)?;
        Ok(out)
    }

    fn set_running(&mut self, on: bool) -> SettingsResult<()> {
        self.shared.control().set_enabled(on);
        Ok(())
    }

    fn apply(&mut self, prefs: &AutomixPrefs) -> SettingsResult<()> {
        apply_automix_prefs(&self.shared, prefs, Some(&self.applied));
        self.applied = prefs.clone();
        Ok(())
    }
}

/// Playlists (not folders) in tree order, labelled `Folder / Name`.
fn collect_playlists(
    dao: &PlaylistDao<'_>,
    parent: Option<i64>,
    prefix: &str,
    out: &mut Vec<(i64, String)>,
) -> om_library::Result<()> {
    for node in dao.children(parent)? {
        let label = format!("{prefix}{}", node.name);
        if node.is_folder {
            collect_playlists(dao, Some(node.id), &format!("{label} / "), out)?;
        } else {
            out.push((node.id, label));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_queue_source_follows_the_preferences() {
        let mut a = AutomixPrefs::default();
        assert_eq!(queue_source(&a), QueueSource::Search(String::new()));
        a.queue_playlist = 7;
        assert_eq!(queue_source(&a), QueueSource::Playlist(7));
        a.queue_search = true;
        assert_eq!(queue_source(&a), QueueSource::CurrentSearch);
    }

    #[test]
    fn the_config_is_derived_from_the_length_gap_and_autostart() {
        let a = AutomixPrefs {
            length_beats: 32,
            min_bpm_gap_percent: 10,
            autostart_at_end: false,
            ..Default::default()
        };
        let cfg = automix_config(&a);
        assert_eq!(cfg.length_beats, Some(32.0));
        assert!((cfg.max_bpm_ratio - 0.10).abs() < 1e-9);
        assert!(!cfg.autostart);
    }

    /// The controller turned itself off (an underrun) while the preference says On. Editing another
    /// setting must not turn the running flag back on.
    #[test]
    fn editing_other_settings_does_not_touch_the_running_flag() {
        let shared = AutomixShared::new();
        let launch = AutomixPrefs {
            enabled: true,
            ..Default::default()
        };
        apply_automix_prefs(&shared, &launch, None);
        assert!(shared.control().is_enabled());
        shared.control().set_enabled(false);
        let edited = AutomixPrefs {
            style: 2,
            ..launch.clone()
        };
        apply_automix_prefs(&shared, &edited, Some(&launch));
        assert!(!shared.control().is_enabled());
    }

    #[test]
    fn applying_prefs_updates_the_queue_settings_the_host_reads() {
        let shared = AutomixShared::new();
        let a = AutomixPrefs {
            enabled: true,
            style: 2,
            smart: false,
            queue_search: true,
            ..Default::default()
        };
        apply_automix_prefs(&shared, &a, None);
        let queue = shared.queue_settings();
        let settings = queue.lock().unwrap().clone();
        assert_eq!(settings.source, QueueSource::CurrentSearch);
        assert!(!settings.smart);
        assert!(shared.control().is_enabled());
    }
}
