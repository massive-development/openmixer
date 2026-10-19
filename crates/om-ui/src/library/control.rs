//! The settings page's view of the library: [`LibraryControl`] implemented over the services, so
//! the page's watch-folder edits reach the library and its subscribers. Port of the calls
//! `LibrarySettingsPage.cpp` makes on `LibraryServices`.

use std::path::PathBuf;

use super::services::LibraryServices;
use crate::settings::{LibraryControl, SettingsResult};

impl LibraryControl for LibraryServices {
    fn available(&self) -> bool {
        self.watch_folders().is_some()
    }

    fn folders(&self) -> Vec<PathBuf> {
        self.watch_folders()
            .and_then(|watch| watch.list().ok())
            .unwrap_or_default()
            .into_iter()
            .map(PathBuf::from)
            .collect()
    }

    fn add_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
        let result = match self.watch_folders() {
            Some(watch) => watch.add(&folder).map_err(|e| e.to_string()),
            None => Err("no watch folders available".to_owned()),
        };
        self.notify_library_changed();
        result
    }

    fn remove_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
        let result = match self.watch_folders() {
            Some(watch) => watch.remove(&folder).map_err(|e| e.to_string()),
            None => Err("no watch folders available".to_owned()),
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
            None => Err("no watch folders available".to_owned()),
        }
    }
}
