//! The plugin API a plugin implements and the services the host offers it. Plugins are trait objects: in-process,
//! or a shared library behind the C ABI adapted by the loader (see `docs/spikes/DECISIONS.md`, "Plugins: in-process
//! only" and "Plugins: C ABI dynamic loading").

use om_core::ParameterTarget;
use om_engine::ProcessStage;
use std::sync::Arc;
use thiserror::Error;

/// Plugin API version the host speaks. A plugin reporting another version is refused.
pub const API_VERSION: u32 = 1;

/// Identity a plugin reports about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInfo {
    /// Unique, e.g. `"com.example.gain"`. Loading a second plugin with the same id is refused.
    pub id: String,
    pub name: String,
    pub version: String,
}

/// One plugin parameter: the id the plugin's parameter target accepts, and its range.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterInfo {
    pub id: String,
    pub name: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
}

/// Where a stage is meant to sit. The host does not enforce the kind; it is reported for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageKind {
    Fx,
    PreProcess,
    PostProcess,
}

/// Creates a fresh stage instance. Called on the control thread each time a stage is attached.
pub type StageFactory = Box<dyn Fn() -> Box<dyn ProcessStage> + Send + Sync>;

/// Why the host refused a request or an operation failed.
#[derive(Debug, Error)]
pub enum PluginError {
    /// Registration refused: the name or prefix is taken, or the data is invalid.
    #[error("registration refused: {0}")]
    Rejected(String),
    #[error("unsupported plugin API version {0}")]
    UnsupportedApi(u32),
    #[error("plugin id empty or already loaded: {0}")]
    DuplicateId(String),
    #[error("unknown stage {0}")]
    UnknownStage(String),
    #[error("cannot insert {0} at that channel or slot")]
    CannotInsert(String),
    #[error("unknown MIDI map {0}")]
    UnknownMap(String),
    #[error("MIDI map {name}: {reason}")]
    InvalidMap { name: String, reason: String },
    /// A shared library built for another C ABI version (see [`crate::abi::PLUGIN_ABI_VERSION`]).
    #[error("plugin ABI version {found}, the host speaks {min} to {expected}", min = crate::abi::PLUGIN_ABI_MIN)]
    BadAbi { found: u32, expected: u32 },
    /// A shared library that does not export `openmixer_plugin_entry`. Holds the path.
    #[error("{0}: no openmixer_plugin_entry symbol")]
    MissingEntry(String),
    /// A file that is not a loadable library, or a plugin whose table or calls failed.
    #[error("{path}: {reason}")]
    InvalidLibrary { path: String, reason: String },
    #[error("cannot read {path}: {reason}")]
    Io { path: String, reason: String },
}

/// Services the host offers a plugin. Only valid during [`Plugin::on_load`]; the host hands a
/// plugin no other way to register things, so a call outside a load cannot happen.
pub trait PluginHost {
    /// Registers a stage factory under `stage_id`. Ids are unique across all plugins.
    fn register_stage(
        &mut self,
        stage_id: &str,
        kind: StageKind,
        factory: StageFactory,
    ) -> Result<(), PluginError>;

    /// Registers a controller map as JSON (the `om-midi` map schema). The JSON must parse.
    fn register_midi_map(&mut self, map_name: &str, json: &str) -> Result<(), PluginError>;

    /// Routes `"prefix.rest"` parameter ids to `target`, with `rest` as the id it receives. The
    /// target stays registered until the plugin unloads.
    fn register_parameter_target(
        &mut self,
        prefix: &str,
        target: Arc<dyn ParameterTarget>,
    ) -> Result<(), PluginError>;

    /// A UI theme as JSON (the theme schema of `om-ui`). The host checks the envelope (an object
    /// with a unique string `id`); the UI registry validates the tokens when it adopts the theme.
    fn register_theme(&mut self, theme_json: &str) -> Result<(), PluginError>;
}

/// A plugin: identity, load and unload hooks. Implement this in-process, or in a plugin crate
/// that the application links.
pub trait Plugin: Send {
    fn info(&self) -> PluginInfo;

    /// The API version the plugin was built against. Checked by the host before `on_load`.
    fn api_version(&self) -> u32 {
        API_VERSION
    }

    /// The parameters the plugin's parameter target accepts (ids relative to its prefix). Empty by
    /// default.
    fn parameters(&self) -> Vec<ParameterInfo> {
        Vec::new()
    }

    /// The current value of parameter `id`, or `None` when the plugin has no such parameter.
    fn parameter(&self, _id: &str) -> Option<f64> {
        None
    }

    /// Registers the plugin's stages, targets, maps and themes.
    fn on_load(&mut self, host: &mut dyn PluginHost);

    /// Called after the plugin's registrations are removed and before the plugin is dropped.
    fn on_unload(&mut self) {}
}
