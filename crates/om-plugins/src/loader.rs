//! Where shared-library plugins would load, and the in-process built-in list.
//!
//! Dynamic loading is NOT implemented. Rust has no stable ABI, so a `.so`/`.dll`/`.dylib` built
//! from Rust cannot be loaded safely by the host: a plugin would have to be built with the exact
//! compiler and crate versions as the host. The options are a C ABI (a `#[repr(C)]` function table
//! behind `extern "C"`), a WASM component, or source-level plugins compiled into the app. That choice
//! is open (`docs/spikes/DECISIONS.md`, "Plugins: in-process only"). Until it is made, the loader
//! only reports what it found, and plugins come from [`builtin_plugins`] or [`crate::PluginRegistry::add_plugin`].
//!
//! Loading is open by default: any shared library exporting the entry points is accepted, with no allow-list. The
//! future loader keeps that stance: no allow-list and no signature
//! check are planned.

use crate::api::{Plugin, PluginError};
use std::path::{Path, PathBuf};

/// Extensions treated as plugin libraries. The stub reports them without loading them.
pub const SHARED_LIBRARY_EXTENSIONS: [&str; 3] = ["so", "dll", "dylib"];

pub struct PluginLoader;

impl PluginLoader {
    /// Loads one plugin file. Always refused: see the module docs.
    pub fn load(path: &Path) -> Result<Box<dyn Plugin>, PluginError> {
        if !path.is_file() {
            return Err(PluginError::Io {
                path: path.display().to_string(),
                reason: "no such file".into(),
            });
        }
        Err(PluginError::DynamicLoadingUnsupported(
            path.display().to_string(),
        ))
    }

    /// The shared-library files in `dir` (not recursive, no symlinks). A folder that does not
    /// exist has none.
    pub fn candidates(dir: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut found: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| SHARED_LIBRARY_EXTENSIONS.contains(&e))
            })
            .collect();
        found.sort();
        found
    }
}

/// Plugins compiled into the application, loaded by every folder load. Empty by default: the
/// example plugin is a separate library, and so is `examples/plugins/gain-plugin`.
pub fn builtin_plugins() -> Vec<Box<dyn Plugin>> {
    Vec::new()
}
