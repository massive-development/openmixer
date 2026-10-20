//! The Automix page's link to the app. [`apply_automix_prefs`] pushes the stored settings into the
//! running automix (the controller config, the style and enable flags, and the queue settings the
//! host reads before each pick); [`AppAutomixSettings`] is the page's control. The window also
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

/// Pushes the stored automix settings to the running service.
pub fn apply_automix_prefs(shared: &AutomixShared, a: &AutomixPrefs) {
    shared.update_config(automix_config(a));
    let control = shared.control();
    control.set_style(AutomixStyle::from_u8(a.style as u8));
    control.set_enabled(a.enabled);
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
}

impl AppAutomixSettings {
    /// `library` is `None` when the library did not open; the page then says automix is idle.
    pub fn new(shared: Arc<AutomixShared>, library: Option<Arc<Library>>) -> Self {
        Self { shared, library }
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

    fn apply(&mut self, prefs: &AutomixPrefs) -> SettingsResult<()> {
        apply_automix_prefs(&self.shared, prefs);
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
        let mut a = AutomixPrefs::default();
        a.length_beats = 32;
        a.min_bpm_gap_percent = 10;
        a.autostart_at_end = false;
        let cfg = automix_config(&a);
        assert_eq!(cfg.length_beats, Some(32.0));
        assert!((cfg.max_bpm_ratio - 0.10).abs() < 1e-9);
        assert!(!cfg.autostart);
    }

    #[test]
    fn applying_prefs_updates_the_queue_settings_the_host_reads() {
        let shared = AutomixShared::new();
        let mut a = AutomixPrefs::default();
        a.enabled = true;
        a.style = 2;
        a.smart = false;
        a.queue_search = true;
        apply_automix_prefs(&shared, &a);
        let queue = shared.queue_settings();
        let settings = queue.lock().unwrap().clone();
        assert_eq!(settings.source, QueueSource::CurrentSearch);
        assert!(!settings.smart);
        assert!(shared.control().is_enabled());
    }
}
