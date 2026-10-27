//! Shared-library plugins over the C ABI. Each test builds a variant of
//! `examples/plugins/gain-plugin-cdylib` with `cargo build` into the test temp dir (the crate has no
//! dependencies, so a build takes seconds) and loads it through `PluginLoader`.

use om_plugins::{
    InsertAt, InsertPoint, InsertRefused, PluginError, PluginLoader, PluginRegistry, ProcessStage,
    PLUGIN_ABI_VERSION,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};

const PLUGIN_ID: &str = "org.openmixer.example.cdylib-gain";
const STAGE_ID: &str = "example.cdylib-gain";

/// Builds the example cdylib with `feature` (or none) once per test run and returns the library.
/// Fails the test with the cargo output when the build cannot run.
fn example_library(feature: Option<&str>) -> PathBuf {
    static BUILT: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();
    let key = feature.unwrap_or("default").to_owned();
    let mut built = BUILT
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(path) = built.get(&key) {
        return path.clone();
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/plugins/gain-plugin-cdylib/Cargo.toml");
    let target_dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("gain-plugin-cdylib")
        .join(&key);
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cmd = Command::new(&cargo);
    cmd.arg("build")
        .arg("--offline")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--target-dir")
        .arg(&target_dir);
    if let Some(feature) = feature {
        cmd.arg("--features").arg(feature);
    }
    let output = cmd.output().unwrap_or_else(|e| {
        panic!("cannot run `{cargo} build` for the example plugin (is cargo on PATH?): {e}")
    });
    assert!(
        output.status.success(),
        "building the example plugin ({key}) failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let file = format!(
        "{}gain_plugin_cdylib{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let path = target_dir.join("debug").join(file);
    assert!(
        path.is_file(),
        "built library not found at {}",
        path.display()
    );
    built.insert(key, path.clone());
    path
}

/// Holds inserted stages. A cleared slot keeps the displaced stage in `retired`, as the engine
/// frees it later, so a test can use a stage after its plugin unloaded.
#[derive(Default)]
struct Slots {
    live: Mutex<Option<Box<dyn ProcessStage>>>,
    retired: Mutex<Vec<Box<dyn ProcessStage>>>,
}

impl InsertPoint for Slots {
    fn channel_count(&self) -> usize {
        2
    }

    fn set_insert(
        &self,
        _at: InsertAt,
        _slot: usize,
        stage: Option<Box<dyn ProcessStage>>,
    ) -> Result<(), InsertRefused> {
        let old = std::mem::replace(&mut *self.live.lock().unwrap(), stage);
        self.retired.lock().unwrap().extend(old);
        Ok(())
    }
}

fn process_live(slots: &Slots, left: &mut [f32], right: &mut [f32]) {
    slots
        .live
        .lock()
        .unwrap()
        .as_mut()
        .expect("a stage is attached")
        .process(left, right);
}

#[test]
fn loads_example_plugin_and_processes_gain() {
    let library = example_library(None);
    let plugin = PluginLoader::load(&library).expect("example plugin loads");
    let info = plugin.info();
    assert_eq!(info.id, PLUGIN_ID);
    assert_eq!(info.name, "Example C-ABI Gain");
    assert_eq!(info.version, "1.0");

    let mut reg = PluginRegistry::new();
    reg.add_plugin(plugin).expect("registers");
    assert_eq!(reg.plugin_ids(), vec![PLUGIN_ID.to_owned()]);
    assert_eq!(reg.stage_ids(None), vec![STAGE_ID.to_owned()]);

    let slots = Arc::new(Slots::default());
    let point: Arc<dyn InsertPoint> = slots.clone();
    reg.attach_stage(STAGE_ID, &point, InsertAt::Master, 0)
        .expect("attaches");
    let mut left = vec![0.8_f32; 64];
    let mut right = vec![-0.4_f32; 64];
    process_live(&slots, &mut left, &mut right);
    assert!(left.iter().all(|&s| (s - 0.4).abs() < 1.0e-6), "{left:?}");
    assert!(right.iter().all(|&s| (s + 0.2).abs() < 1.0e-6), "{right:?}");

    // Unloading clears the stage; the engine may still hold it, so the library must stay loaded
    // until the stage is dropped.
    assert!(reg.unload(PLUGIN_ID));
    drop(reg);
    assert!(slots.live.lock().unwrap().is_none());
    let mut retired = slots.retired.lock().unwrap();
    assert_eq!(retired.len(), 1);
    let mut samples = vec![1.0_f32; 8];
    let mut other = vec![1.0_f32; 8];
    retired[0].process(&mut samples, &mut other);
    assert!(samples.iter().all(|&s| (s - 0.5).abs() < 1.0e-6));
    retired.clear(); // destroys the instances, then releases the library
}

#[test]
fn registry_loads_a_plugin_folder() {
    let library = example_library(None);
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::copy(
        &library,
        dir.path().join(library.file_name().expect("file name")),
    )
    .expect("copy");
    std::fs::write(dir.path().join("broken.so"), "not a library").expect("write");
    let mut reg = PluginRegistry::new();
    let mut errors = Vec::new();
    assert_eq!(reg.load_directory(dir.path(), &mut errors), 1);
    assert_eq!(reg.plugin_ids(), vec![PLUGIN_ID.to_owned()]);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("broken.so"), "{errors:?}");
}

#[test]
fn wrong_abi_version_is_bad_abi() {
    let library = example_library(Some("bad-abi"));
    match PluginLoader::load(&library) {
        Err(PluginError::BadAbi { found, expected }) => {
            assert_eq!(expected, PLUGIN_ABI_VERSION);
            assert_eq!(found, PLUGIN_ABI_VERSION + 1);
        }
        other => panic!("expected BadAbi, got {:?}", other.err()),
    }
}

#[test]
fn library_without_entry_is_missing_entry() {
    let library = example_library(Some("no-entry"));
    match PluginLoader::load(&library) {
        Err(PluginError::MissingEntry(path)) => {
            assert!(path.contains("gain_plugin_cdylib"), "{path}");
        }
        other => panic!("expected MissingEntry, got {:?}", other.err()),
    }
}

#[test]
fn panic_in_create_is_contained() {
    let library = example_library(Some("panic-in-create"));
    match PluginLoader::load(&library) {
        Err(PluginError::InvalidLibrary { reason, .. }) => {
            assert!(reason.contains("create"), "{reason}");
        }
        other => panic!("expected InvalidLibrary, got {:?}", other.err()),
    }
    // The host carries on: a good plugin still loads after the failed one.
    let good = PluginLoader::load(&example_library(None)).expect("loads");
    assert_eq!(good.info().id, PLUGIN_ID);
}

#[test]
fn non_library_file_is_a_typed_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    for ext in om_plugins::SHARED_LIBRARY_EXTENSIONS {
        let path = dir.path().join(format!("fake.{ext}"));
        std::fs::write(&path, "this is not a shared library").expect("write");
        match PluginLoader::load(&path) {
            Err(PluginError::InvalidLibrary { path: p, .. }) => assert!(p.contains("fake")),
            other => panic!("expected InvalidLibrary for .{ext}, got {:?}", other.err()),
        }
    }
    assert!(matches!(
        PluginLoader::load(&dir.path().join("missing.so")),
        Err(PluginError::Io { .. })
    ));
}
