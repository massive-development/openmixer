//! Library settings page: watch folders added, removed and rescanned through the library, and the
//! interval and scan-on-start preferences. Drives the existing settings page model over the real
//! [`LibraryServices`] (which implements its `LibraryControl`).

mod library_common;

use library_common::*;
use om_ui::library::{LibraryServices, Options};
use om_ui::preferences::Preferences;
use om_ui::settings::LibrarySettingsPage;

fn file_services(dir: &std::path::Path) -> LibraryServices {
    let mut options = Options::for_path(dir.join("db").join("library.db").to_string_lossy());
    options.analyzers = Some(fake_analyzers());
    options.scan_on_start = false;
    make_services(options)
}

#[test]
fn a_memory_library_refuses_watch_folder_edits() {
    let mut prefs = Preferences::default();
    let mut services = make_services(memory_options());
    let mut page = LibrarySettingsPage::new(&mut prefs, &mut services);
    assert!(page.folders().unwrap().is_empty());
    assert!(page.add_watch_folder("/music".into()).is_err());
    assert!(page.rescan().is_err());
}

#[test]
fn watch_folders_can_be_added_removed_and_rescanned() {
    let dir = tempfile::tempdir().unwrap();
    let music = dir.path().join("music");
    std::fs::create_dir_all(&music).unwrap();
    let mut prefs = Preferences::default();
    let mut services = file_services(dir.path());

    let mut page = LibrarySettingsPage::new(&mut prefs, &mut services);
    page.add_watch_folder(music.clone()).expect("folder added");
    assert_eq!(page.folders().unwrap(), vec![music.clone()]);
    assert!(page.add_watch_folder(dir.path().join("missing")).is_err());
    assert_eq!(
        page.folders().unwrap().len(),
        1,
        "a refused folder is not listed"
    );
    page.rescan().expect("rescan starts");
    page.remove_watch_folder(music).expect("folder removed");
    assert!(page.folders().unwrap().is_empty());
}

#[test]
fn interval_is_clamped_and_scan_on_start_is_stored() {
    let mut prefs = Preferences::default();
    let mut services = make_services(memory_options());
    {
        let mut page = LibrarySettingsPage::new(&mut prefs, &mut services);
        assert!(page.set_scan_interval(0));
        assert!(page.set_scan_on_start(false));
        assert!(
            !page.set_scan_on_start(false),
            "unchanged value reports no change"
        );
    }
    assert_eq!(prefs.library.scan_interval_minutes, 1);
    assert!(!prefs.library.scan_on_start);
    LibrarySettingsPage::new(&mut prefs, &mut services).set_scan_interval(99_999);
    assert_eq!(
        prefs.library.scan_interval_minutes,
        LibrarySettingsPage::MAX_INTERVAL_MINUTES
    );
}
