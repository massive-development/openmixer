#![forbid(unsafe_code)]

//! Plugin API, registry and stage catalog.
//!
//! A plugin implements [`Plugin`] and registers its stages, controller maps, parameter targets and
//! themes through [`PluginHost`] in `on_load`. The [`PluginRegistry`] owns the loaded plugins and
//! installs what they register into the engine (stages through [`InsertPoint`]), the MIDI mapper
//! ([`PluginRegistry::apply_midi_map`]), the parameter router ([`PluginRegistry::bind_router`]) and
//! the theme list ([`PluginRegistry::themes`]).
//!
//! Plugins are in-process for now. Shared-library loading is a documented stub in [`loader`].

mod api;
mod catalog;
mod loader;
mod registry;

pub use api::{Plugin, PluginError, PluginHost, PluginInfo, StageFactory, StageKind, API_VERSION};
pub use catalog::{InsertAt, InsertPoint, StageCatalog};
pub use loader::{builtin_plugins, PluginLoader, SHARED_LIBRARY_EXTENSIONS};
pub use om_engine::ProcessStage;
pub use registry::{PluginRegistry, ProvidedTheme};
