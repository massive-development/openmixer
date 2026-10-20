//! The Library tab: watch folders, the scan timer, rescan and import.

use super::chips::*;
use super::stems::stems_section;
use super::*;

/// Minutes the Library page offers for the rescan timer.
const RESCAN_CHOICES: [i32; 6] = [1, 5, 15, 30, 60, 240];

/// Stands in for the library when it did not open: the scan settings are still stored, and the
/// library actions report that there is no library.
pub(super) struct NoLibrary;

impl LibraryControl for NoLibrary {
    fn available(&self) -> bool {
        false
    }

    fn add_watch_folder(&mut self, _: PathBuf) -> SettingsResult<()> {
        Err(SettingsError::LibraryUnavailable)
    }

    fn remove_watch_folder(&mut self, _: PathBuf) -> SettingsResult<()> {
        Err(SettingsError::LibraryUnavailable)
    }

    fn rescan(&mut self) -> SettingsResult<()> {
        Err(SettingsError::LibraryUnavailable)
    }
}

pub(super) fn library_page(
    t: &ThemeTokens,
    settings: &SettingsState,
    cx: &mut Context<Shell>,
) -> Div {
    let mut folders = div().flex().flex_col();
    if !settings.library_available() {
        folders = folders.child(note(
            t,
            "The library is not available (see the terminal for the reason).",
        ));
    } else if settings.lists.folders.is_empty() {
        folders = folders.child(note(
            t,
            "No watch folder yet. Add one to scan it into the library.",
        ));
    }
    for folder in &settings.lists.folders {
        let press = folder.clone();
        folders = folders.child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .mb(px(4.))
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .text_color(paint(t.text_primary))
                        .child(folder.display().to_string()),
                )
                .child(chip(t, "Remove", false, cx, move |this| {
                    let folder = press.clone();
                    this.settings_mut(|s| s.library_remove_folder(folder))
                })),
        );
    }

    let mut intervals = Vec::new();
    for minutes in RESCAN_CHOICES {
        let lit = settings.prefs.library.scan_interval_minutes == minutes;
        intervals.push(chip(t, &format!("{minutes} min"), lit, cx, move |this| {
            this.settings_mut(|s| s.library_set_interval(minutes))
        }));
    }
    let scan_on = settings.prefs.library.scan_on_start;
    let on_start = chip(
        t,
        if scan_on { "On" } else { "Off" },
        scan_on,
        cx,
        move |this| this.settings_mut(|s| s.library_set_scan_on_start(!scan_on)),
    );

    let mut column = div()
        .flex()
        .flex_col()
        .child(choice_row(t, "Watch folders", folders))
        .child(choice_row(
            t,
            "Folders",
            row_of(vec![
                chip(t, "Add watch folder…", false, cx, |this| {
                    this.settings_mut(|s| s.begin_entry(EntryKind::WatchFolder))
                }),
                chip(t, "Rescan now", false, cx, |this| {
                    this.settings_mut(|s| s.library_rescan())
                }),
            ]),
        ));
    if let Some(field) = entry_field(t, settings, EntryKind::WatchFolder, "folder path", cx) {
        column = column.child(field);
    }
    column = column
        .child(choice_row(t, "Rescan every", row_of(intervals)))
        .child(choice_row(
            t,
            "Scan the folders on start",
            row_of(vec![on_start]),
        ));
    column = column.child(stems_section(t, settings, cx));
    column = column.child(choice_row(
        t,
        "Import a library",
        row_of(vec![chip(
            t,
            "Import iTunes or Rekordbox XML…",
            false,
            cx,
            |this| this.settings_mut(|s| s.begin_entry(EntryKind::ImportXml)),
        )]),
    ));
    if let Some(field) = entry_field(t, settings, EntryKind::ImportXml, "XML file path", cx) {
        column = column.child(field);
    }
    column
}

impl SettingsState {
    pub fn library_available(&self) -> bool {
        self.library.is_some()
    }

    /// Runs `f` on the Library page over the library, or over the stand-in when there is none, so
    /// the scan settings are stored either way.
    pub(super) fn with_library_page<T>(
        &mut self,
        f: impl FnOnce(&mut LibrarySettingsPage<'_>) -> SettingsResult<T>,
    ) -> SettingsResult<T> {
        let mut none = NoLibrary;
        let library: &mut dyn LibraryControl = match self.library.as_deref_mut() {
            Some(library) => library,
            None => &mut none,
        };
        let mut page = LibrarySettingsPage::new(&mut self.prefs, library);
        f(&mut page)
    }

    pub fn library_set_scan_on_start(&mut self, on: bool) {
        if matches!(
            self.with_library_page(|p| Ok(p.set_scan_on_start(on))),
            Ok(true)
        ) {
            self.changed();
        }
    }

    pub fn library_set_interval(&mut self, minutes: i32) {
        if matches!(
            self.with_library_page(|p| Ok(p.set_scan_interval(minutes))),
            Ok(true)
        ) {
            self.changed();
        }
    }

    pub fn library_add_folder(&mut self, folder: PathBuf) {
        let shown = folder.display().to_string();
        match self.with_library_page(|p| p.add_watch_folder(folder)) {
            Ok(()) => {
                self.refresh_lists();
                self.set_notice(format!("Watching {shown}"), false);
            }
            Err(error) => self.set_notice(error.to_string(), true),
        }
    }

    pub fn library_remove_folder(&mut self, folder: PathBuf) {
        let shown = folder.display().to_string();
        match self.with_library_page(|p| p.remove_watch_folder(folder)) {
            Ok(()) => {
                self.refresh_lists();
                self.set_notice(format!("Stopped watching {shown}"), false);
            }
            Err(error) => self.set_notice(error.to_string(), true),
        }
    }

    pub fn library_rescan(&mut self) {
        match self.with_library_page(|p| p.rescan()) {
            Ok(()) => self.set_notice("Rescan started", false),
            Err(error) => self.set_notice(error.to_string(), true),
        }
    }

    pub fn library_import(&mut self, xml: PathBuf) {
        match self.with_library_page(|p| p.import_xml(xml)) {
            Ok(summary) => self.set_notice(summary, false),
            Err(error) => self.set_notice(format!("Import failed: {error}"), true),
        }
    }

    // Plugins page.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_view::fixtures::*;
    use om_ui::settings::SettingsResult;
    use std::cell::Cell;

    use std::rc::Rc;

    /// A library that keeps its folders in memory.
    #[derive(Default)]
    struct ListLibrary {
        folders: Vec<PathBuf>,
    }

    impl LibraryControl for ListLibrary {
        fn available(&self) -> bool {
            true
        }
        fn folders(&self) -> SettingsResult<Vec<PathBuf>> {
            Ok(self.folders.clone())
        }
        fn add_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
            self.folders.push(folder);
            Ok(())
        }
        fn remove_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
            self.folders.retain(|f| *f != folder);
            Ok(())
        }
        fn rescan(&mut self) -> SettingsResult<()> {
            Ok(())
        }
        fn import_xml(&mut self, xml: PathBuf) -> SettingsResult<String> {
            Ok(format!("imported {}", xml.display()))
        }
    }

    #[test]
    fn watch_folders_follow_the_library_and_scan_settings_are_stored_without_one() {
        let changes = Rc::new(Cell::new(0));
        let mut s = SettingsState::new(options(
            None,
            Some(Box::new(ListLibrary::default())),
            None,
            Rc::new(Cell::new(0)),
            Rc::clone(&changes),
        ));
        s.library_add_folder(PathBuf::from("/music"));
        assert_eq!(s.lists.folders, vec![PathBuf::from("/music")]);
        s.library_remove_folder(PathBuf::from("/music"));
        assert!(s.lists.folders.is_empty());
        s.library_import(PathBuf::from("/lib.xml"));
        assert_eq!(s.notice.text, "imported /lib.xml");

        let mut bare = SettingsState::new(options(
            None,
            None,
            None,
            Rc::new(Cell::new(0)),
            Rc::clone(&changes),
        ));
        bare.library_set_interval(15);
        assert_eq!(bare.prefs.library.scan_interval_minutes, 15);
        bare.library_add_folder(PathBuf::from("/music"));
        assert!(bare.notice.error);
        assert!(bare.lists.folders.is_empty());
    }
}
