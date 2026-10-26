//! Loads shared-library plugins over the C ABI, and the in-process built-in list.
//!
//! A `.so`/`.dll`/`.dylib` exporting `openmixer_plugin_entry` (see [`crate::abi`]) is opened with
//! `libloading`, its table is checked, and the library is adapted to [`Plugin`]: the registry
//! treats it like an in-process plugin. The adapter registers one stage; each stage holds one
//! plugin instance per channel. The decision is in `docs/spikes/DECISIONS.md`, "Plugins: C ABI
//! dynamic loading".
//!
//! Lifetimes: the adapter and every stage hold the library through a shared handle, and a stage
//! destroys its instances before letting go of it. The library unloads after the plugin is
//! unloaded and the engine has dropped the plugin's last stage.
//!
//! Loading is open by default: any shared library exporting the entry point is accepted, with no
//! allow-list and no signature check. Loading runs the library's initialisers, so a plugin folder
//! is trusted like any other program.

use crate::abi::{self, Instance, LoadedLibrary, PluginEntry, TableError, PLUGIN_ABI_VERSION};
use crate::api::{Plugin, PluginError, PluginHost, PluginInfo, StageKind};
use om_engine::ProcessStage;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Extensions treated as plugin libraries.
pub const SHARED_LIBRARY_EXTENSIONS: [&str; 3] = ["so", "dll", "dylib"];

pub struct PluginLoader;

impl PluginLoader {
    /// Loads one shared-library plugin and adapts it to [`Plugin`]. `create` is called (and the
    /// instance destroyed) once here, so a plugin that cannot create an instance fails now.
    pub fn load(path: &Path) -> Result<Box<dyn Plugin>, PluginError> {
        let shown = path.display().to_string();
        if !path.is_file() {
            return Err(PluginError::Io {
                path: shown,
                reason: "no such file".into(),
            });
        }
        let invalid = |reason: String| PluginError::InvalidLibrary {
            path: shown.clone(),
            reason,
        };
        // SAFETY: opening a library runs its initialisers. Loading is open by design (no
        // allow-list, CONTRIBUTING "Extensibility"); the user trusts the plugin folder.
        let library =
            unsafe { libloading::Library::new(path) }.map_err(|e| invalid(e.to_string()))?;
        // SAFETY: the symbol is declared by the C ABI as `PluginEntry`. The function pointer is
        // copied out and called only below, while `library` is alive.
        let entry: PluginEntry =
            match unsafe { library.get::<PluginEntry>(abi::ENTRY_SYMBOL.as_bytes()) } {
                Ok(symbol) => *symbol,
                Err(_) => return Err(PluginError::MissingEntry(shown)),
            };
        // `extern "C"` function pointers do not implement `FnOnce`, so the closure is needed.
        #[allow(clippy::redundant_closure)]
        let table = abi::guarded("openmixer_plugin_entry", || entry()).map_err(&invalid)?;
        // SAFETY: the ABI says the entry returns null or a table that lives as long as the library;
        // `LoadedLibrary` keeps the library with the table.
        let table = unsafe { abi::read_table(table) }.map_err(|e| match e {
            TableError::BadAbi(found) => PluginError::BadAbi {
                found,
                expected: PLUGIN_ABI_VERSION,
            },
            TableError::Null => invalid("openmixer_plugin_entry returned null".into()),
            TableError::NullFunction(name) => invalid(format!("table has a null {name}")),
        })?;
        // SAFETY: `table` was read from `library` just above.
        let library = Arc::new(unsafe { LoadedLibrary::new(table, library) });

        let text = library.info().map_err(&invalid)?;
        let described = parse_info(&text).map_err(&invalid)?;
        drop(Instance::create(&library).map_err(&invalid)?);
        Ok(Box::new(LibraryPlugin { library, described }))
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

/// What a library's `info` reported.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Described {
    info: PluginInfo,
    stage_id: String,
    kind: StageKind,
}

fn parse_info(text: &str) -> Result<Described, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("info is not JSON: {e}"))?;
    let field = |name: &str| value.get(name).and_then(serde_json::Value::as_str);
    let required = |name: &str| {
        field(name)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("info has no string {name:?}"))
    };
    let info = PluginInfo {
        id: required("id")?,
        name: required("name")?,
        version: required("version")?,
    };
    let stage_id = field("stage_id").unwrap_or(&info.id).to_owned();
    let kind = match field("kind").unwrap_or("fx") {
        "fx" => StageKind::Fx,
        "pre" => StageKind::PreProcess,
        "post" => StageKind::PostProcess,
        other => return Err(format!("info has unknown kind {other:?}")),
    };
    Ok(Described {
        info,
        stage_id,
        kind,
    })
}

/// A shared-library plugin as a [`Plugin`]. Holds the library for as long as it lives.
struct LibraryPlugin {
    library: Arc<LoadedLibrary>,
    described: Described,
}

impl Plugin for LibraryPlugin {
    fn info(&self) -> PluginInfo {
        self.described.info.clone()
    }

    fn on_load(&mut self, host: &mut dyn PluginHost) {
        let library = Arc::clone(&self.library);
        // `on_load` cannot fail: a stage id another plugin took leaves this one without a stage,
        // which the Plugins page shows (no stage listed).
        let _ = host.register_stage(
            &self.described.stage_id,
            self.described.kind,
            Box::new(move || Box::new(LibraryStage::new(&library))),
        );
    }
}

/// A stage backed by two plugin instances, one per channel. A channel whose instance could not be
/// created passes audio through.
struct LibraryStage {
    left: Option<Instance>,
    right: Option<Instance>,
}

impl LibraryStage {
    fn new(library: &Arc<LoadedLibrary>) -> Self {
        Self {
            left: Instance::create(library).ok(),
            right: Instance::create(library).ok(),
        }
    }
}

impl ProcessStage for LibraryStage {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        if let Some(instance) = &mut self.left {
            instance.process(left);
        }
        if let Some(instance) = &mut self.right {
            instance.process(right);
        }
    }
}

/// Plugins compiled into the application, loaded by every folder load. Empty by default: the
/// example plugins (`examples/plugins/gain-plugin`, `examples/plugins/gain-plugin-cdylib`) are
/// separate crates.
pub fn builtin_plugins() -> Vec<Box<dyn Plugin>> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_defaults_and_refusals() {
        let d = parse_info(r#"{"id":"a","name":"A","version":"1"}"#).expect("parses");
        assert_eq!(d.stage_id, "a");
        assert_eq!(d.kind, StageKind::Fx);
        let d = parse_info(r#"{"id":"a","name":"A","version":"1","stage_id":"s","kind":"pre"}"#)
            .expect("parses");
        assert_eq!((d.stage_id.as_str(), d.kind), ("s", StageKind::PreProcess));
        assert!(parse_info("not json").is_err());
        assert!(parse_info(r#"{"id":"a","name":"A"}"#).is_err());
        assert!(parse_info(r#"{"id":"","name":"A","version":"1"}"#).is_err());
        assert!(parse_info(r#"{"id":"a","name":"A","version":"1","kind":"x"}"#).is_err());
    }
}
