//! The composition attaches the library on demand: a file library opens into the hook, and a
//! library that cannot open leaves the hook empty and reports why.

use openmixer::composition::AppComposition;

#[test]
fn attaching_a_file_library_fills_the_library_hook() {
    let dir = tempfile::tempdir().unwrap();
    let mut composition = AppComposition::new(&dir.path().join("preferences.json"), None);
    assert!(composition.hooks.library.is_none());
    composition
        .attach_library(Some(dir.path().join("db").join("library.db")))
        .expect("library opens");
    assert!(composition.hooks.library.is_some());
    assert!(composition
        .hooks
        .library
        .as_ref()
        .unwrap()
        .watch_folders()
        .is_some());
}

#[test]
fn an_unopenable_library_leaves_the_hook_empty_and_reports_why() {
    let dir = tempfile::tempdir().unwrap();
    let mut composition = AppComposition::new(&dir.path().join("preferences.json"), None);
    let error = composition.attach_library(Some("/proc/does/not/exist/library.db".into()));
    assert!(error.is_err());
    assert!(composition.hooks.library.is_none());
}
