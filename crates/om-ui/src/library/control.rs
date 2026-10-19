//! The settings page's view of the library: [`LibraryControl`] implemented over the services, so
//! the page's watch-folder edits reach the library and its subscribers.

use std::path::PathBuf;

use super::services::LibraryServices;
use crate::settings::{LibraryControl, SettingsError, SettingsResult};
use om_library::ImportReport;

/// The one-line outcome of an import, as the library page and the browser show it.
pub(super) fn import_summary(report: &ImportReport) -> String {
    let mut text = format!(
        "Imported {} new, {} merged, {} playlists",
        report.tracks_added, report.tracks_merged, report.playlists_created
    );
    if report.errors > 0 {
        text.push_str(&format!(", {} problems", report.errors));
    }
    text
}

impl LibraryControl for LibraryServices {
    fn available(&self) -> bool {
        self.watch_folders().is_some()
    }

    fn folders(&self) -> SettingsResult<Vec<PathBuf>> {
        let Some(watch) = self.watch_folders() else {
            return Ok(Vec::new());
        };
        Ok(watch.list()?.into_iter().map(PathBuf::from).collect())
    }

    fn add_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
        let result = match self.watch_folders() {
            Some(watch) => watch.add(&folder).map_err(SettingsError::from),
            None => Err(SettingsError::NoWatchFolders),
        };
        self.notify_library_changed();
        result
    }

    fn remove_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
        let result = match self.watch_folders() {
            Some(watch) => watch.remove(&folder).map_err(SettingsError::from),
            None => Err(SettingsError::NoWatchFolders),
        };
        self.notify_library_changed();
        result
    }

    fn rescan(&mut self) -> SettingsResult<()> {
        match self.watch_folders() {
            Some(watch) => {
                watch.trigger_scan();
                Ok(())
            }
            None => Err(SettingsError::NoWatchFolders),
        }
    }

    fn import_xml(&mut self, xml: PathBuf) -> SettingsResult<String> {
        let report = self.import_file(&xml)?;
        Ok(import_summary(&report))
    }
}
