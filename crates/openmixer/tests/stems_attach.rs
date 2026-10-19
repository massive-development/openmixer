//! Stem service attachment: needs the library first, the mock separator attaches without a model,
//! and without a manifest (or a model) stem separation is simply off.

use openmixer::composition::AppComposition;
use std::path::PathBuf;

fn composition(dir: &std::path::Path) -> AppComposition {
    AppComposition::new(&dir.join("preferences.json"), None)
}

#[test]
fn stems_need_the_library_first() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = composition(dir.path());
    assert!(c.attach_stems(true).is_err());
    assert!(c.services.stems.is_none());
}

#[test]
fn the_mock_separator_attaches_beside_the_library() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = composition(dir.path());
    let db: PathBuf = dir.path().join("db").join("library.db");
    c.attach_library(Some(db.clone())).expect("library");
    c.attach_stems(true).expect("mock stems");
    let service = c.services.stems.as_ref().expect("stem service");
    assert_eq!(service.cache().root(), dir.path().join("db").join("stems"));
    assert_eq!(service.queued_len(), 0, "nothing starts by itself");
}

#[test]
fn without_a_model_manifest_stems_stay_off_without_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = composition(dir.path());
    c.attach_library(Some(dir.path().join("db").join("library.db")))
        .expect("library");
    c.attach_stems(false).expect("no error when no model");
    assert!(c.services.stems.is_none());
}
