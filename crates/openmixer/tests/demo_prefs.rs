//! A demo run (any `--demo-*` flag) must never write the user's preferences file: its settings
//! changes go to a scratch file instead.

use om_ui::preferences::Preferences;
use openmixer::composition::{prefs_save_target, AppComposition};

#[test]
fn a_demo_run_saves_settings_away_from_the_user_file() {
    let home = tempfile::tempdir().expect("tempdir");
    let scratch = tempfile::tempdir().expect("tempdir");
    let real = home.path().join("OpenMixer").join("preferences.json");
    Preferences::default()
        .save(&real)
        .expect("seed the user's file");
    let before = std::fs::read(&real).expect("read seeded file");

    // The composition loads the user's file (read only), as the demo window does.
    let composition = AppComposition::new(&real, None);
    let mut changed = composition.prefs.clone();
    changed.theme = "demo-only-change".into();

    // The window's save path for a demo run.
    let target = prefs_save_target(true, real.clone(), scratch.path());
    changed.save(&target).expect("demo save");

    assert_eq!(
        std::fs::read(&real).expect("read user file"),
        before,
        "a demo run must not write the user's preferences file"
    );
    let saved = std::fs::read_to_string(&target).expect("scratch file written");
    assert!(saved.contains("demo-only-change"));
}

#[test]
fn a_normal_run_saves_to_the_user_file() {
    let home = tempfile::tempdir().expect("tempdir");
    let scratch = tempfile::tempdir().expect("tempdir");
    let real = home.path().join("preferences.json");
    assert_eq!(prefs_save_target(false, real.clone(), scratch.path()), real);
}
