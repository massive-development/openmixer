//! Library settings page model: the watch-folder list (add, remove, rescan now) and the scan interval
//! and scan-on-start preferences. The folder list lives in the library database, so it is edited
//! through [`LibraryServices`]; the interval and scan-on-start are read when the library starts.
//! Port of `src/ui/LibrarySettingsPage.{h,cpp}` (model only).

use std::path::Path;

use super::services::LibraryServices;
use crate::preferences::Preferences;

/// Scan intervals offered, in minutes.
pub const SCAN_INTERVALS_MINUTES: [i32; 5] = [1, 5, 15, 30, 60];

/// Caption for an interval in the drop-down.
pub fn interval_label(minutes: i32) -> String {
    if minutes == 1 {
        "Every minute".to_owned()
    } else {
        format!("Every {minutes} minutes")
    }
}

pub struct LibrarySettingsPage {
    folders: Vec<String>,
    status: String,
    available: bool,
    selected: Option<usize>,
}

impl LibrarySettingsPage {
    /// Reads the folder list. `services` is `None` when the library failed to open; the page then
    /// explains itself and its folder controls stay disabled.
    pub fn new(services: Option<&LibraryServices>) -> Self {
        let mut page = Self {
            folders: Vec::new(),
            status: String::new(),
            available: false,
            selected: None,
        };
        page.refresh_folders(services);
        page.status = match services {
            None => "The library is not available.".to_owned(),
            Some(s) if s.watch_folders().is_none() => "This library has no watch folders.".to_owned(),
            Some(_) => String::new(),
        };
        page
    }

    /// Re-reads the folder list.
    pub fn reflect(&mut self, services: Option<&LibraryServices>) {
        self.refresh_folders(services);
    }

    fn refresh_folders(&mut self, services: Option<&LibraryServices>) {
        self.folders = services
            .and_then(|s| s.watch_folders())
            .and_then(|w| w.list().ok())
            .unwrap_or_default();
        self.available = services.is_some_and(|s| s.watch_folders().is_some());
        if self.selected.is_some_and(|row| row >= self.folders.len()) {
            self.selected = None;
        }
    }

    /// Whether Add, Remove and Rescan can do anything.
    pub fn controls_enabled(&self) -> bool {
        self.available
    }

    pub fn folders(&self) -> &[String] {
        &self.folders
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn select(&mut self, row: Option<usize>) {
        self.selected = row.filter(|&r| r < self.folders.len());
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Stores the interval (minutes) for the next start.
    pub fn set_interval(prefs: &mut Preferences, minutes: i32) {
        prefs.library.scan_interval_minutes = minutes;
    }

    /// Stores scan-on-start for the next start.
    pub fn set_scan_on_start(prefs: &mut Preferences, on: bool) {
        prefs.library.scan_on_start = on;
    }

    /// Adds a watch folder and asks for a scan.
    pub fn add_folder(&mut self, services: &mut LibraryServices, folder: &Path) -> Result<(), String> {
        let Some(watch) = services.watch_folders() else {
            return Err("no watch folders available".to_owned());
        };
        let result = watch.add(folder).map_err(|e| e.to_string());
        self.status = match &result {
            Ok(()) => format!(
                "Added {}; scanning",
                folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
            ),
            Err(error) => format!("Could not add: {error}"),
        };
        self.refresh_folders(Some(services));
        services.notify_library_changed();
        result
    }

    /// Removes a watch folder by path.
    pub fn remove_folder(&mut self, services: &mut LibraryServices, path: &str) -> Result<(), String> {
        let Some(watch) = services.watch_folders() else {
            return Err("no watch folders available".to_owned());
        };
        let result = watch.remove(Path::new(path)).map_err(|e| e.to_string());
        self.refresh_folders(Some(services));
        services.notify_library_changed();
        result
    }

    /// Removes the selected folder, if any, and reports the outcome.
    pub fn remove_selected(&mut self, services: &mut LibraryServices) {
        let Some(path) = self.selected.and_then(|row| self.folders.get(row).cloned()) else {
            return;
        };
        self.status = match self.remove_folder(services, &path) {
            Ok(()) => "Folder removed".to_owned(),
            Err(_) => "Could not remove the folder".to_owned(),
        };
    }

    /// Asks for a scan of every watch folder now.
    pub fn rescan(&mut self, services: &LibraryServices) {
        if let Some(watch) = services.watch_folders() {
            watch.trigger_scan();
            self.status = "Rescan started".to_owned();
        }
    }
}
