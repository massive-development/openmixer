//! Shared-library plugins over the C ABI. Each test builds a variant of
//! `examples/plugins/gain-plugin-cdylib` (ABI version 1) or `examples/plugins/param-plugin-cdylib`
//! (version 2: prepare and parameters; it sees only the C header) with `cargo build` into the test
//! temp dir (the crates have no dependencies, so a build takes seconds) and loads it through
//! `PluginLoader`.

use assert_no_alloc::{assert_no_alloc, AllocDisabler};
use om_core::{ParameterRouter, ParameterTarget};
use om_plugins::{
    InsertAt, InsertPoint, InsertRefused, PluginError, PluginLoader, PluginRegistry, ProcessStage,
    PLUGIN_ABI_VERSION,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};

// Panics on a heap allocation made inside `assert_no_alloc` on the current thread.
#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const PLUGIN_ID: &str = "org.openmixer.example.cdylib-gain";
const STAGE_ID: &str = "example.cdylib-gain";

/// Builds the example cdylib with `feature` (or none) once per test run and returns the library.
/// Fails the test with the cargo output when the build cannot run.
fn example_library(feature: Option<&str>) -> PathBuf {
    build_cdylib("gain-plugin-cdylib", feature)
}

/// Builds `examples/plugins/<name>` (once per test run) and returns the library.
fn build_cdylib(name: &str, feature: Option<&str>) -> PathBuf {
    static BUILT: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();
    let key = format!("{name}/{}", feature.unwrap_or("default"));
    let mut built = BUILT
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(path) = built.get(&key) {
        return path.clone();
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../examples/plugins/{name}/Cargo.toml"));
    let target_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(&key);
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
        "{}{}{}",
        std::env::consts::DLL_PREFIX,
        name.replace('-', "_"),
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
            assert_eq!(
                found,
                PLUGIN_ABI_VERSION + 1,
                "the bad-abi variant reports 3"
            );
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

// ---- ABI version 2: prepare, parameters, contained panics ----

const PARAMS_ID: &str = "org.openmixer.example.cdylib-params";
const PARAMS_STAGE: &str = "example.cdylib-params";
const BLOCK: usize = 256;

fn params_library() -> PathBuf {
    build_cdylib("param-plugin-cdylib", None)
}

/// Loads the params plugin into a registry and attaches its stage to fresh slots.
fn params_registry() -> (PluginRegistry, Arc<Slots>, Arc<ParameterRouter>) {
    let mut reg = PluginRegistry::new();
    let router = Arc::new(ParameterRouter::new());
    reg.bind_router(Some(Arc::clone(&router)));
    reg.load_library(&params_library()).expect("loads");
    let slots = attach_new_stage(&mut reg);
    (reg, slots, router)
}

fn attach_new_stage(reg: &mut PluginRegistry) -> Arc<Slots> {
    let slots = Arc::new(Slots::default());
    let point: Arc<dyn InsertPoint> = slots.clone();
    reg.attach_stage(PARAMS_STAGE, &point, InsertAt::Master, 0)
        .expect("attaches");
    slots
}

fn prepare_live(slots: &Slots, sample_rate: f64) {
    let mut live = slots.live.lock().unwrap();
    live.as_mut().expect("attached").prepare(sample_rate, BLOCK);
}

fn render(slots: &Slots, input: f32) -> (Vec<f32>, Vec<f32>) {
    let (mut left, mut right) = (vec![input; BLOCK], vec![input; BLOCK]);
    process_live(slots, &mut left, &mut right);
    (left, right)
}

fn set(router: &ParameterRouter, id: &str, value: f64) -> bool {
    router.set_parameter(&format!("{PARAMS_STAGE}.{id}"), value)
}

#[test]
fn old_abi_version_one_plugin_is_still_accepted() {
    // The gain example is a version 1 table: no prepare, no parameters.
    let plugin = PluginLoader::load(&example_library(None)).expect("v1 loads");
    assert!(plugin.parameters().is_empty());
    assert_eq!(plugin.parameter("gain"), None);
}

#[test]
fn parameters_are_enumerated_and_set_through_the_router() {
    let (reg, slots, router) = params_registry();
    let params = reg.plugin_parameters(PARAMS_ID);
    let ids: Vec<&str> = params.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["gain", "tone", "crash"]);
    assert_eq!(
        (params[0].min, params[0].max, params[0].default),
        (0.0, 2.0, 1.0)
    );
    assert_eq!(reg.plugin_parameter(PARAMS_ID, "gain"), Some(1.0));

    assert_eq!(render(&slots, 0.5).0[0], 0.5, "default gain is 1");
    assert!(set(&router, "gain", 0.5));
    assert_eq!(reg.plugin_parameter(PARAMS_ID, "gain"), Some(0.5));
    let (left, right) = render(&slots, 0.5);
    assert!(left.iter().chain(&right).all(|&s| s == 0.25), "{left:?}");

    // Values clamp to the declared range; unknown ids and non-finite values are refused.
    assert!(set(&router, "gain", 9.0));
    assert_eq!(reg.plugin_parameter(PARAMS_ID, "gain"), Some(2.0));
    assert!(!set(&router, "nope", 1.0));
    assert!(!set(&router, "gain", f64::NAN));
    assert_eq!(reg.plugin_parameter(PARAMS_ID, "gain"), Some(2.0));
}

#[test]
fn a_new_stage_starts_from_the_current_parameter_values() {
    let (mut reg, _slots, router) = params_registry();
    assert!(set(&router, "gain", 0.0));
    let second = attach_new_stage(&mut reg);
    assert!(render(&second, 1.0).0.iter().all(|&s| s == 0.0));
}

#[test]
fn prepare_delivers_the_sample_rate() {
    let (_reg, slots, router) = params_registry();
    assert!(set(&router, "tone", 1.0));
    for rate in [48_000.0_f64, 96_000.0] {
        prepare_live(&slots, rate);
        let (left, right) = render(&slots, 0.0);
        let step = std::f64::consts::TAU * 1000.0 / rate;
        for (n, (&l, &r)) in left.iter().zip(&right).enumerate() {
            let want = (step * n as f64).sin() as f32;
            assert!(
                (l - want).abs() < 1.0e-6,
                "{rate} Hz sample {n}: {l} vs {want}"
            );
            assert!(
                (r - want).abs() < 1.0e-6,
                "{rate} Hz sample {n}: {r} vs {want}"
            );
        }
    }
}

#[test]
fn a_panicking_process_is_contained_and_the_stage_passes_audio_through() {
    let (mut reg, slots, router) = params_registry();
    assert!(set(&router, "gain", 0.5));
    assert_eq!(render(&slots, 1.0).0[0], 0.5);

    // `crash` makes the plugin's process panic; the plugin catches it and reports a fault.
    assert!(set(&router, "crash", 1.0));
    let (left, _) = render(&slots, 1.0);
    assert!(
        left.iter().all(|&s| s == 1.0),
        "the faulted instance is skipped"
    );
    // Later buffers keep passing through, and the host is still alive.
    assert!(set(&router, "crash", 0.0));
    assert!(render(&slots, 1.0).0.iter().all(|&s| s == 1.0));
    assert_eq!(reg.plugin_ids(), vec![PARAMS_ID.to_owned()]);

    // A stage made afterwards is healthy.
    let fresh = attach_new_stage(&mut reg);
    assert_eq!(render(&fresh, 1.0).0[0], 0.5);
}

#[test]
fn the_audio_path_does_not_allocate() {
    let (_reg, slots, router) = params_registry();
    prepare_live(&slots, 48_000.0);
    let (mut left, mut right) = (vec![0.5_f32; BLOCK], vec![0.5_f32; BLOCK]);
    for gain in [1.0, 0.25, 1.5] {
        assert!(set(&router, "gain", gain));
        // Includes the parameter push at the start of the buffer.
        assert_no_alloc(|| process_live(&slots, &mut left, &mut right));
    }
}

#[test]
fn unloading_the_params_plugin_removes_its_target() {
    let (mut reg, slots, router) = params_registry();
    assert!(reg.unload(PARAMS_ID));
    assert!(!set(&router, "gain", 1.0));
    slots.retired.lock().unwrap().clear();
}
