//! The library page: watch folders, rescan, scan settings and XML import.

use super::{set_if_changed, SettingsError, SettingsResult};
use crate::preferences::Preferences;
use std::path::PathBuf;

/// What the library page needs. Implemented by [`crate::library::LibraryServices`].
pub trait LibraryControl {
    fn available(&self) -> bool;
    /// Watch folder paths, in the library's order; empty when there is no library. A read failure
    /// is an error, so the page can show it rather than an empty list.
    fn folders(&self) -> SettingsResult<Vec<PathBuf>> {
        Ok(Vec::new())
    }
    fn add_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()>;
    fn remove_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()>;
    fn rescan(&mut self) -> SettingsResult<()>;
    /// Imports an iTunes or Rekordbox XML file; returns the one-line outcome for the page.
    fn import_xml(&mut self, _xml: PathBuf) -> SettingsResult<String> {
        Err(SettingsError::ImportUnavailable)
    }
}

/// Library page logic. Interval and scan-on-start are read when the library starts.
pub struct LibrarySettingsPage<'a> {
    prefs: &'a mut Preferences,
    library: &'a mut dyn LibraryControl,
}

impl<'a> LibrarySettingsPage<'a> {
    pub const MAX_INTERVAL_MINUTES: i32 = 24 * 60;

    pub fn new(prefs: &'a mut Preferences, library: &'a mut dyn LibraryControl) -> Self {
        Self { prefs, library }
    }

    /// Watch folders the library currently scans.
    pub fn folders(&self) -> SettingsResult<Vec<PathBuf>> {
        self.library.folders()
    }

    pub fn set_scan_on_start(&mut self, on: bool) -> bool {
        set_if_changed(&mut self.prefs.library.scan_on_start, on)
    }

    /// Clamped to `1..=MAX_INTERVAL_MINUTES`, as the preferences reader does.
    pub fn set_scan_interval(&mut self, minutes: i32) -> bool {
        let clamped = minutes.clamp(1, Self::MAX_INTERVAL_MINUTES);
        set_if_changed(&mut self.prefs.library.scan_interval_minutes, clamped)
    }

    pub fn add_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
        self.require_available()?;
        self.library.add_watch_folder(folder)
    }

    pub fn remove_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
        self.require_available()?;
        self.library.remove_watch_folder(folder)
    }

    pub fn rescan(&mut self) -> SettingsResult<()> {
        self.require_available()?;
        self.library.rescan()
    }

    /// Imports the iTunes or Rekordbox XML file at `xml` into the library.
    pub fn import_xml(&mut self, xml: PathBuf) -> SettingsResult<String> {
        self.require_available()?;
        self.library.import_xml(xml)
    }

    fn require_available(&self) -> SettingsResult<()> {
        if self.library.available() {
            Ok(())
        } else {
            Err(SettingsError::LibraryUnavailable)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::Preferences;
    use crate::settings::{PluginHost, PluginsSettingsPage};
    use om_library::LibraryError;

    #[derive(Default)]
    struct FakeLibrary {
        available: bool,
        calls: Vec<String>,
    }
    impl LibraryControl for FakeLibrary {
        fn available(&self) -> bool {
            self.available
        }
        fn add_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
            self.calls.push(format!("add:{}", folder.display()));
            Ok(())
        }
        fn remove_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
            self.calls.push(format!("remove:{}", folder.display()));
            Ok(())
        }
        fn rescan(&mut self) -> SettingsResult<()> {
            self.calls.push("rescan".into());
            Ok(())
        }
    }

    #[test]
    fn library_interval_is_clamped_and_scan_on_start_is_stored() {
        let mut prefs = Preferences::default();
        let mut lib = FakeLibrary {
            available: true,
            ..Default::default()
        };
        {
            let mut page = LibrarySettingsPage::new(&mut prefs, &mut lib);
            assert!(page.set_scan_interval(0));
            assert!(!page.set_scan_interval(1), "clamped to 1 is already stored");
        }
        assert_eq!(prefs.library.scan_interval_minutes, 1);
        {
            let mut page = LibrarySettingsPage::new(&mut prefs, &mut lib);
            assert!(page.set_scan_interval(100_000));
            assert!(page.set_scan_on_start(false));
        }
        assert_eq!(prefs.library.scan_interval_minutes, 1440);
        assert!(!prefs.library.scan_on_start);
    }

    #[test]
    fn library_folder_buttons_are_disabled_without_a_library() {
        let mut prefs = Preferences::default();
        let mut lib = FakeLibrary::default();
        let mut page = LibrarySettingsPage::new(&mut prefs, &mut lib);
        assert!(page.rescan().is_err());
        assert!(page.add_watch_folder(PathBuf::from("/music")).is_err());
        assert!(lib.calls.is_empty());
    }

    #[derive(Default)]
    struct Library {
        available: bool,
        folders: Vec<PathBuf>,
        rescans: usize,
    }

    impl LibraryControl for Library {
        fn available(&self) -> bool {
            self.available
        }
        fn add_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
            if !self.available {
                return Err(SettingsError::LibraryUnavailable);
            }
            self.folders.push(folder);
            Ok(())
        }
        fn remove_watch_folder(&mut self, folder: PathBuf) -> SettingsResult<()> {
            self.folders.retain(|f| *f != folder);
            Ok(())
        }
        fn rescan(&mut self) -> SettingsResult<()> {
            self.rescans += 1;
            Ok(())
        }
    }

    #[test]
    fn watch_folders_are_added_and_removed_through_the_library() {
        let mut prefs = Preferences::default();
        let mut library = Library {
            available: true,
            ..Library::default()
        };
        let mut page = LibrarySettingsPage::new(&mut prefs, &mut library);
        page.add_watch_folder(PathBuf::from("/music")).unwrap();
        page.add_watch_folder(PathBuf::from("/more")).unwrap();
        page.remove_watch_folder(PathBuf::from("/music")).unwrap();
        assert_eq!(library.folders, vec![PathBuf::from("/more")]);
    }

    /// A library whose watch-folder table cannot be read.
    struct UnreadableFolders;
    impl LibraryControl for UnreadableFolders {
        fn available(&self) -> bool {
            true
        }
        fn folders(&self) -> SettingsResult<Vec<PathBuf>> {
            Err(SettingsError::Library(LibraryError::Io(
                std::io::Error::other("watch table is locked"),
            )))
        }
        fn add_watch_folder(&mut self, _: PathBuf) -> SettingsResult<()> {
            Ok(())
        }
        fn remove_watch_folder(&mut self, _: PathBuf) -> SettingsResult<()> {
            Ok(())
        }
        fn rescan(&mut self) -> SettingsResult<()> {
            Ok(())
        }
    }

    #[test]
    fn a_watch_folder_read_error_reaches_the_library_page() {
        let mut prefs = Preferences::default();
        let mut library = UnreadableFolders;
        let page = LibrarySettingsPage::new(&mut prefs, &mut library);
        let err = page.folders().unwrap_err();
        assert!(err.to_string().contains("watch table is locked"), "{err}");
    }

    #[test]
    fn rescan_triggers_one_scan_pass_per_press() {
        let mut prefs = Preferences::default();
        let mut library = Library {
            available: true,
            ..Library::default()
        };
        let mut page = LibrarySettingsPage::new(&mut prefs, &mut library);
        page.rescan().unwrap();
        assert_eq!(library.rescans, 1);
    }

    #[test]
    fn a_hand_edited_interval_is_clamped_when_the_page_next_sets_it() {
        let mut prefs = Preferences::default();
        prefs.library.scan_interval_minutes = 99_999;
        let mut library = Library::default();
        let mut page = LibrarySettingsPage::new(&mut prefs, &mut library);
        assert!(page.set_scan_interval(LibrarySettingsPage::MAX_INTERVAL_MINUTES + 1));
        assert_eq!(
            prefs.library.scan_interval_minutes,
            LibrarySettingsPage::MAX_INTERVAL_MINUTES
        );
    }

    struct NoLibrary;

    impl LibraryControl for NoLibrary {
        fn available(&self) -> bool {
            false
        }
        fn add_watch_folder(&mut self, _: PathBuf) -> SettingsResult<()> {
            Ok(())
        }
        fn remove_watch_folder(&mut self, _: PathBuf) -> SettingsResult<()> {
            Ok(())
        }
        fn rescan(&mut self) -> SettingsResult<()> {
            Ok(())
        }
    }

    #[test]
    fn import_without_a_library_is_refused_and_the_default_host_has_no_built_ins() {
        let mut prefs = Preferences::default();
        let mut library = NoLibrary;
        let mut page = LibrarySettingsPage::new(&mut prefs, &mut library);
        assert!(page.import_xml(PathBuf::from("/tmp/x.xml")).is_err());

        struct Bare;
        impl PluginHost for Bare {
            fn loaded_plugins(&self) -> Vec<String> {
                Vec::new()
            }
            fn reload(&mut self) -> SettingsResult<()> {
                Ok(())
            }
        }
        let prefs = Preferences::default();
        let mut host = Bare;
        let mut page = PluginsSettingsPage::new(&prefs, &mut host);
        assert!(page.builtins().is_empty());
        assert!(page.load_builtin("x").is_err());
        assert!(page.unload("x").is_err());
    }
}
