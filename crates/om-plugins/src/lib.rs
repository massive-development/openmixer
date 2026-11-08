// `unsafe` is denied everywhere but the modules that load libraries: `abi` and `loader` (C ABI) and
// `clap/entry.rs` (CLAP, feature `clap`).
#![deny(unsafe_code)]

//! Plugin API, registry and stage catalog.
//!
//! A plugin implements [`Plugin`] and registers its stages, controller maps, parameter targets and
//! themes through [`PluginHost`] in `on_load`. The [`PluginRegistry`] owns the loaded plugins and
//! installs what they register into the engine (stages through [`InsertPoint`]), the MIDI mapper
//! ([`PluginRegistry::apply_midi_map`]), the parameter router ([`PluginRegistry::bind_router`]) and
//! the theme list ([`PluginRegistry::themes`]).
//!
//! A plugin is in-process, or a shared library exporting the C ABI in [`abi`]: [`PluginLoader`]
//! loads it and adapts it to [`Plugin`]. With the `clap` feature, `.clap` files load through the
//! [`clap`] module (ADR 0007).

#[allow(unsafe_code)]
pub mod abi;
mod api;
mod catalog;
#[cfg(feature = "clap")]
pub mod clap;
#[allow(unsafe_code)]
mod loader;
mod registry;

pub use abi::{PluginVTable, PluginVTableV1, PLUGIN_ABI_MIN, PLUGIN_ABI_VERSION};
pub use api::{
    ParameterInfo, Plugin, PluginError, PluginHost, PluginInfo, StageFactory, StageKind,
    API_VERSION, MAX_STATE_BYTES,
};
pub use catalog::{InsertAt, InsertPoint, InsertRefused, StageCatalog};
pub use loader::{builtin_plugins, PluginLoader, SHARED_LIBRARY_EXTENSIONS};
pub use om_engine::ProcessStage;
pub use registry::{PluginRegistry, ProvidedTheme};
