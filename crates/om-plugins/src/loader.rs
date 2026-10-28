//! Loads shared-library plugins over the C ABI, and the in-process built-in list.
//!
//! A `.so`/`.dll`/`.dylib` exporting `openmixer_plugin_entry` (see [`crate::abi`]) is opened with
//! `libloading`, its table is checked, and the library is adapted to [`Plugin`]: the registry
//! treats it like an in-process plugin. The adapter registers one stage; each stage holds one
//! plugin instance per channel. The decision is in `docs/spikes/DECISIONS.md`, "Plugins: C ABI
//! dynamic loading".
//!
//! Version 2 plugins add `prepare` (forwarded by the stage's `ProcessStage::prepare`) and
//! parameters. The adapter keeps one host-side value per parameter; setting it through the
//! plugin's parameter target bumps a generation counter, and each stage pushes the values to its
//! instances at the start of its next `process` (atomics only, no allocation, no lock).
//!
//! Lifetimes: the adapter and every stage hold the library through a shared handle, and a stage
//! destroys its instances before letting go of it. The library unloads after the plugin is
//! unloaded and the engine has dropped the plugin's last stage.
//!
//! Loading is open by default: any shared library exporting the entry point is accepted, with no
//! allow-list and no signature check. Loading runs the library's initialisers, so a plugin folder
//! is trusted like any other program.

use crate::abi::{self, Instance, LoadedLibrary, PluginEntry, TableError, PLUGIN_ABI_VERSION};
use crate::api::{ParameterInfo, Plugin, PluginError, PluginHost, PluginInfo, StageKind};
use om_core::ParameterTarget;
use om_engine::ProcessStage;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
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
        let mut probe = Instance::create(&library).map_err(&invalid)?;
        let params = ParamSet::read(&library, &mut probe).map_err(&invalid)?;
        drop(probe);
        Ok(Box::new(LibraryPlugin {
            library,
            described,
            params: Arc::new(params),
        }))
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
    param_prefix: String,
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
    let param_prefix = field("param_prefix").unwrap_or(&stage_id).to_owned();
    Ok(Described {
        info,
        stage_id,
        kind,
        param_prefix,
    })
}

/// The parameters a plugin declared and their host-side values (`f64` bits). `generation` counts
/// changes so a stage can tell cheaply whether to push the values to its instances.
struct ParamSet {
    infos: Vec<ParameterInfo>,
    values: Vec<AtomicU64>,
    generation: AtomicU64,
}

impl ParamSet {
    /// Reads the declarations and the plugin's initial values (from `probe`, else the default).
    fn read(library: &Arc<LoadedLibrary>, probe: &mut Instance) -> Result<Self, String> {
        let infos = library
            .param_infos()?
            .iter()
            .map(|text| parse_parameter(text))
            .collect::<Result<Vec<_>, _>>()?;
        for (i, p) in infos.iter().enumerate() {
            if infos[..i].iter().any(|q| q.id == p.id) {
                return Err(format!("duplicate parameter id {:?}", p.id));
            }
        }
        let values = infos
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let index = u32::try_from(i).unwrap_or(u32::MAX);
                let start = probe
                    .get_param(index)
                    .filter(|v| (p.min..=p.max).contains(v))
                    .unwrap_or(p.default);
                AtomicU64::new(start.to_bits())
            })
            .collect();
        Ok(Self {
            infos,
            values,
            generation: AtomicU64::new(0),
        })
    }

    fn value(&self, index: usize) -> f64 {
        f64::from_bits(self.values[index].load(Ordering::Relaxed))
    }

    fn set(&self, index: usize, value: f64) {
        let info = &self.infos[index];
        let value = value.clamp(info.min, info.max);
        self.values[index].store(value.to_bits(), Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::Release);
    }

    fn index_of(&self, id: &str) -> Option<usize> {
        self.infos.iter().position(|p| p.id == id)
    }
}

fn parse_parameter(text: &str) -> Result<ParameterInfo, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("parameter info is not JSON: {e}"))?;
    let string = |name: &str| {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("parameter has no string {name:?}"))
    };
    let number = |name: &str| {
        value
            .get(name)
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| format!("parameter has no number {name:?}"))
    };
    let info = ParameterInfo {
        id: string("id")?,
        name: string("name")?,
        min: number("min")?,
        max: number("max")?,
        default: number("default")?,
    };
    if !(info.min <= info.max && (info.min..=info.max).contains(&info.default)) {
        return Err(format!("parameter {:?} has an invalid range", info.id));
    }
    Ok(info)
}

/// The plugin's parameter target: `set_parameter` stores the clamped value for the stages.
struct LibraryTarget {
    params: Arc<ParamSet>,
}

impl ParameterTarget for LibraryTarget {
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        match self.params.index_of(id) {
            Some(index) if value.is_finite() => {
                self.params.set(index, value);
                true
            }
            _ => false,
        }
    }
}

/// A shared-library plugin as a [`Plugin`]. Holds the library for as long as it lives.
struct LibraryPlugin {
    library: Arc<LoadedLibrary>,
    described: Described,
    params: Arc<ParamSet>,
}

impl Plugin for LibraryPlugin {
    fn info(&self) -> PluginInfo {
        self.described.info.clone()
    }

    fn parameters(&self) -> Vec<ParameterInfo> {
        self.params.infos.clone()
    }

    fn parameter(&self, id: &str) -> Option<f64> {
        self.params.index_of(id).map(|i| self.params.value(i))
    }

    fn on_load(&mut self, host: &mut dyn PluginHost) {
        let (library, params) = (Arc::clone(&self.library), Arc::clone(&self.params));
        // `on_load` cannot fail: an id another plugin took leaves this one without that
        // registration, which the Plugins page shows (no stage listed).
        let _ = host.register_stage(
            &self.described.stage_id,
            self.described.kind,
            Box::new(move || Box::new(LibraryStage::new(&library, &params))),
        );
        if !self.params.infos.is_empty() {
            let target = Arc::new(LibraryTarget {
                params: Arc::clone(&self.params),
            });
            let _ = host.register_parameter_target(&self.described.param_prefix, target);
        }
    }
}

/// A stage backed by two plugin instances, one per channel. A channel whose instance could not be
/// created, or that faulted, passes audio through.
struct LibraryStage {
    left: Option<Instance>,
    right: Option<Instance>,
    params: Arc<ParamSet>,
    /// The generation of `params` last pushed to the instances.
    seen: u64,
}

impl LibraryStage {
    fn new(library: &Arc<LoadedLibrary>, params: &Arc<ParamSet>) -> Self {
        let mut stage = Self {
            left: Instance::create(library).ok(),
            right: Instance::create(library).ok(),
            params: Arc::clone(params),
            seen: u64::MAX,
        };
        stage.push_params();
        stage
    }

    /// Pushes the parameter values when they changed. Atomics and plugin calls only.
    fn push_params(&mut self) {
        let generation = self.params.generation.load(Ordering::Acquire);
        if generation == self.seen {
            return;
        }
        self.seen = generation;
        for index in 0..self.params.values.len() {
            let value = self.params.value(index);
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            for instance in [&mut self.left, &mut self.right].into_iter().flatten() {
                instance.set_param(index, value);
            }
        }
    }
}

impl ProcessStage for LibraryStage {
    fn prepare(&mut self, sample_rate: f64, max_block_size: usize) {
        for instance in [&mut self.left, &mut self.right].into_iter().flatten() {
            instance.prepare(sample_rate, max_block_size);
        }
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.push_params();
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
