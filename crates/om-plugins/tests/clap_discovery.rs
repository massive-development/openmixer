//! Finding `.clap` files in the plugin folder and the standard folders, and the two gates in front of
//! loading them: the first-run answer (standard folders) and the first-load warning (ADR 0007,
//! decisions 1 and 6).

#![cfg(feature = "clap")]

mod clap_fixture;

use clap_fixture::{fixture, PLUGIN_ID};
use om_plugins::PluginRegistry;
use std::path::{Path, PathBuf};

/// A plugin folder holding a copy of the fixture, and a "home" with a `.clap` folder holding another
/// file that is not a library (so any attempt to open it shows up as an error).
struct Folders {
    _root: tempfile::TempDir,
    plugins: PathBuf,
    home: PathBuf,
}

fn folders() -> Folders {
    let root = tempfile::tempdir().expect("tempdir");
    let plugins = root.path().join("plugins");
    let home = root.path().join("home");
    std::fs::create_dir_all(&plugins).expect("mkdir");
    std::fs::create_dir_all(home.join(".clap")).expect("mkdir");
    std::fs::copy(fixture(None), plugins.join("gain.clap")).expect("copy");
    std::fs::write(plugins.join("notes.txt"), b"x").expect("write");
    std::fs::write(home.join(".clap").join("standard.clap"), b"not a library").expect("write");
    Folders {
        _root: root,
        plugins,
        home,
    }
}

fn names(files: &[PathBuf]) -> Vec<String> {
    files
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_owned))
        .collect()
}

fn load(registry: &mut PluginRegistry, f: &Folders, errors: &mut Vec<String>) -> usize {
    registry.load_clap_folders(&f.plugins, Some(Path::new(&f.home)), errors)
}

#[test]
fn files_are_listed_but_nothing_is_opened_before_the_warning_is_accepted() {
    let f = folders();
    let mut registry = PluginRegistry::new();
    registry.set_clap_policy(false, false);
    let mut errors = Vec::new();
    assert_eq!(load(&mut registry, &f, &mut errors), 0);
    assert_eq!(names(registry.clap_files_found()), ["gain.clap"]);
    assert_eq!(names(registry.clap_files_waiting()), ["gain.clap"]);
    assert!(registry.plugin_ids().is_empty(), "no plugin code ran");
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn the_standard_folders_are_scanned_only_after_the_first_run_answer() {
    let f = folders();
    let mut registry = PluginRegistry::new();
    let mut errors = Vec::new();

    // Warning accepted, standard folders declined: the user folder loads, the home folder is not
    // even looked into.
    registry.set_clap_policy(false, true);
    assert_eq!(load(&mut registry, &f, &mut errors), 1, "{errors:?}");
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(names(registry.clap_files_found()), ["gain.clap"]);
    assert!(registry.clap_files_waiting().is_empty());
    assert_eq!(registry.plugin_ids(), [PLUGIN_ID]);
    registry.unload_all();

    // Allowed: the home folder is scanned too, and its broken file is reported when opened.
    registry.set_clap_policy(true, true);
    // `/usr/lib/clap` is scanned too, so a machine with plugins there loads more than the fixture.
    let loaded = load(&mut registry, &f, &mut errors);
    assert!(loaded >= 1, "{errors:?}");
    let found = names(registry.clap_files_found());
    assert!(found.contains(&"gain.clap".to_owned()), "{found:?}");
    assert!(found.contains(&"standard.clap".to_owned()), "{found:?}");
    assert!(
        errors.iter().any(|e| e.contains("standard.clap")),
        "{errors:?}"
    );
}

#[test]
fn accepting_the_warning_loads_what_was_waiting() {
    let f = folders();
    let mut registry = PluginRegistry::new();
    let mut errors = Vec::new();
    registry.set_clap_policy(false, false);
    assert_eq!(load(&mut registry, &f, &mut errors), 0);
    assert_eq!(registry.clap_files_waiting().len(), 1);

    registry.set_clap_policy(false, true);
    registry.unload_all();
    assert_eq!(load(&mut registry, &f, &mut errors), 1, "{errors:?}");
    assert!(registry.clap_files_waiting().is_empty());
    assert_eq!(registry.plugin_ids(), [PLUGIN_ID]);
}
