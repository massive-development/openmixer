//! Builds the CLAP fixture plugin for the integration tests. Shared by `clap_*.rs`.

#![allow(dead_code)] // each test file uses a different part

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

pub const PLUGIN_ID: &str = "org.openmixer.test.clap-gain";

/// Builds `examples/plugins/<crate>` (once per run and feature) and returns the library path.
fn build(krate: &str, feature: Option<&str>, as_clap: bool) -> PathBuf {
    static BUILT: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();
    let key = format!("{krate}/{}", feature.unwrap_or("default"));
    let mut built = BUILT
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(path) = built.get(&key) {
        return path.clone();
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../examples/plugins/{krate}/Cargo.toml"));
    let target_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(&key);
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cmd = Command::new(&cargo);
    cmd.args(["build", "--offline", "--quiet", "--manifest-path"])
        .arg(&manifest)
        .arg("--target-dir")
        .arg(&target_dir);
    if let Some(feature) = feature {
        cmd.arg("--features").arg(feature);
    }
    let output = cmd
        .output()
        .expect("run cargo build for the fixture plugin");
    assert!(
        output.status.success(),
        "building {key} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let file = format!(
        "{}{}{}",
        std::env::consts::DLL_PREFIX,
        krate.replace('-', "_"),
        std::env::consts::DLL_SUFFIX
    );
    let mut path = target_dir.join("debug").join(file);
    assert!(
        path.is_file(),
        "built library not found at {}",
        path.display()
    );
    if as_clap {
        // Copied once under the lock: a copy racing a dlopen of the same file would fault.
        let clap = path.with_extension("clap");
        std::fs::copy(&path, &clap).expect("copy the build to a .clap file");
        path = clap;
    }
    built.insert(key, path.clone());
    path
}

/// The fixture as a `.clap` file (a copy next to the build output), with an optional fault feature
/// (`panic-in-process`, `error-in-process`).
pub fn fixture(feature: Option<&str>) -> PathBuf {
    build("clap-gain-fixture", feature, true)
}

/// A valid shared library that is not a CLAP plugin (the C-ABI gain example).
pub fn other_library() -> PathBuf {
    build("gain-plugin-cdylib", None, false)
}
