//! Loads plugins and keeps what they register: stages, MIDI maps, parameter targets and themes.
//! Control thread only.
//!
//! Plugins are in-process ([`PluginRegistry::add_plugin`]) until the dynamic-loading decision is
//! made (see [`crate::loader`]). The rules: ids are unique, a second
//! registration of a stage, map, prefix or theme id is refused, and unloading removes everything
//! the plugin registered, then calls its `on_unload`.

use crate::api::{
    Plugin, PluginError, PluginHost, PluginInfo, StageFactory, StageKind, API_VERSION,
};
use crate::catalog::{InsertAt, InsertPoint, StageCatalog};
use crate::loader::{builtin_plugins, PluginLoader};
use om_core::{ParameterRouter, ParameterTarget};
use om_midi::{MidiMap, MidiMapper};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// A theme a plugin provided, ready for the UI registry. `owner` is the plugin id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvidedTheme {
    pub id: String,
    pub json: String,
    pub owner: String,
}

struct Entry {
    id: String,
    plugin: Box<dyn Plugin>,
}

struct MapEntry {
    json: String,
    owner: String,
}

struct TargetEntry {
    prefix: String,
    target: Arc<dyn ParameterTarget>,
    owner: String,
}

/// Everything plugins register. Kept apart from the loaded plugins so a plugin can be handed a
/// registration context while it is itself borrowed for `on_load`.
#[derive(Default)]
struct Registrations {
    stages: StageCatalog,
    maps: BTreeMap<String, MapEntry>,
    targets: Vec<TargetEntry>,
    themes: Vec<ProvidedTheme>,
    router: Option<Arc<ParameterRouter>>,
}

#[derive(Default)]
pub struct PluginRegistry {
    entries: Vec<Entry>,
    regs: Registrations,
}

/// The host side of one `on_load` call. Exists only for the duration of the call.
struct LoadContext<'a> {
    owner: &'a str,
    regs: &'a mut Registrations,
}

impl PluginHost for LoadContext<'_> {
    fn register_stage(
        &mut self,
        stage_id: &str,
        kind: StageKind,
        factory: StageFactory,
    ) -> Result<(), PluginError> {
        self.regs.stages.add(self.owner, stage_id, kind, factory)
    }

    fn register_midi_map(&mut self, map_name: &str, json: &str) -> Result<(), PluginError> {
        if map_name.is_empty() || self.regs.maps.contains_key(map_name) {
            return Err(PluginError::Rejected(format!(
                "MIDI map {map_name:?} is taken or unnamed"
            )));
        }
        MidiMap::parse(json).map_err(|e| PluginError::InvalidMap {
            name: map_name.to_owned(),
            reason: e.to_string(),
        })?;
        self.regs.maps.insert(
            map_name.to_owned(),
            MapEntry {
                json: json.to_owned(),
                owner: self.owner.to_owned(),
            },
        );
        Ok(())
    }

    fn register_parameter_target(
        &mut self,
        prefix: &str,
        target: Arc<dyn ParameterTarget>,
    ) -> Result<(), PluginError> {
        if prefix.is_empty() || self.regs.targets.iter().any(|t| t.prefix == prefix) {
            return Err(PluginError::Rejected(format!(
                "parameter prefix {prefix:?} is taken or empty"
            )));
        }
        if om_core::param_id::is_reserved_prefix(prefix) {
            return Err(PluginError::Rejected(format!(
                "parameter prefix {prefix:?} is reserved for the engine"
            )));
        }
        if let Some(router) = &self.regs.router {
            router
                .register_target(prefix, Arc::clone(&target))
                .map_err(|e| PluginError::Rejected(e.to_string()))?;
        }
        self.regs.targets.push(TargetEntry {
            prefix: prefix.to_owned(),
            target,
            owner: self.owner.to_owned(),
        });
        Ok(())
    }

    fn register_theme(&mut self, theme_json: &str) -> Result<(), PluginError> {
        // Only the envelope is checked here: a JSON object with a unique, non-empty string id.
        let value: serde_json::Value = serde_json::from_str(theme_json)
            .map_err(|e| PluginError::Rejected(format!("theme is not JSON: {e}")))?;
        let id = value
            .as_object()
            .and_then(|o| o.get("id"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if id.is_empty() {
            return Err(PluginError::Rejected("theme has no string id".into()));
        }
        if self.regs.themes.iter().any(|t| t.id == id) {
            return Err(PluginError::Rejected(format!(
                "theme {id} is already provided"
            )));
        }
        self.regs.themes.push(ProvidedTheme {
            id: id.to_owned(),
            json: theme_json.to_owned(),
            owner: self.owner.to_owned(),
        });
        Ok(())
    }
}

impl Registrations {
    /// Removes what `owner` registered. Returns the targets it held: the caller drops them after
    /// `on_unload`, so a target outlives the plugin's unload hook.
    fn remove_owner(&mut self, owner: &str) -> Vec<Arc<dyn ParameterTarget>> {
        self.stages.remove_owner(owner);
        self.maps.retain(|_, m| m.owner != owner);
        self.themes.retain(|t| t.owner != owner);
        let mut held = Vec::new();
        let mut index = 0;
        while index < self.targets.len() {
            if self.targets[index].owner == owner {
                let entry = self.targets.remove(index);
                if let Some(router) = &self.router {
                    router.unregister_target(&entry.prefix);
                }
                held.push(entry.target);
            } else {
                index += 1;
            }
        }
        held
    }
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an in-process plugin and runs its `on_load`. Refused for an unsupported API version or
    /// an id that is empty or already loaded.
    pub fn add_plugin(&mut self, mut plugin: Box<dyn Plugin>) -> Result<(), PluginError> {
        if plugin.api_version() != API_VERSION {
            return Err(PluginError::UnsupportedApi(plugin.api_version()));
        }
        let info: PluginInfo = plugin.info();
        if info.id.is_empty() || self.entries.iter().any(|e| e.id == info.id) {
            return Err(PluginError::DuplicateId(info.id));
        }
        plugin.on_load(&mut LoadContext {
            owner: &info.id,
            regs: &mut self.regs,
        });
        self.entries.push(Entry {
            id: info.id,
            plugin,
        });
        Ok(())
    }

    /// Loads a shared-library plugin. Not supported yet: see [`crate::loader`].
    pub fn load_library(&mut self, path: &Path) -> Result<(), PluginError> {
        let plugin = PluginLoader::load(path)?;
        self.add_plugin(plugin)
    }

    /// Loads every shared library in `dir` and every built-in plugin. Failures are appended to
    /// `errors`. Returns how many plugins loaded.
    pub fn load_directory(&mut self, dir: &Path, errors: &mut Vec<String>) -> usize {
        let mut loaded = 0;
        for file in PluginLoader::candidates(dir) {
            match self.load_library(&file) {
                Ok(()) => loaded += 1,
                Err(error) => errors.push(error.to_string()),
            }
        }
        for plugin in builtin_plugins() {
            let id = plugin.info().id;
            match self.add_plugin(plugin) {
                Ok(()) => loaded += 1,
                Err(error) => errors.push(format!("{id}: {error}")),
            }
        }
        loaded
    }

    /// Unloads the plugin with `id`: removes its registrations, calls `on_unload`, then drops it.
    /// False when no such plugin is loaded.
    pub fn unload(&mut self, id: &str) -> bool {
        let Some(index) = self.entries.iter().position(|e| e.id == id) else {
            return false;
        };
        let held = self.regs.remove_owner(id);
        let mut entry = self.entries.remove(index);
        entry.plugin.on_unload();
        drop(entry);
        drop(held);
        true
    }

    /// Unloads everything, most recently loaded first.
    pub fn unload_all(&mut self) {
        while let Some(last) = self.entries.last() {
            let id = last.id.clone();
            self.unload(&id);
        }
    }

    pub fn plugin_ids(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.id.clone()).collect()
    }

    pub fn stage_ids(&self, kind: Option<StageKind>) -> Vec<String> {
        self.regs.stages.ids(kind)
    }

    pub fn midi_map_names(&self) -> Vec<String> {
        self.regs.maps.keys().cloned().collect()
    }

    /// Routes the plugins' parameter targets through `router`, now and for later registrations.
    /// The previous router (if any) loses them first; `None` detaches them from all routers.
    pub fn bind_router(&mut self, router: Option<Arc<ParameterRouter>>) {
        if let Some(old) = self.regs.router.take() {
            for target in &self.regs.targets {
                old.unregister_target(&target.prefix);
            }
        }
        if let Some(new) = &router {
            for target in &self.regs.targets {
                // Prefixes were checked when the plugin registered them.
                new.register_target(&target.prefix, Arc::clone(&target.target))
                    .expect("plugin prefixes are validated at registration");
            }
        }
        self.regs.router = router;
    }

    /// Creates the stage `stage_id` and inserts it at `at`/`slot`. The stage is removed when its
    /// plugin unloads.
    pub fn attach_stage(
        &mut self,
        stage_id: &str,
        point: &Arc<dyn InsertPoint>,
        at: InsertAt,
        slot: usize,
    ) -> Result<(), PluginError> {
        self.regs.stages.attach(stage_id, point, at, slot)
    }

    /// Clears a slot a stage was attached to. Does nothing if the slot is empty.
    pub fn detach_stage(&mut self, point: &Arc<dyn InsertPoint>, at: InsertAt, slot: usize) {
        self.regs.stages.detach(point, at, slot);
    }

    /// Themes the plugins registered, in registration order.
    pub fn themes(&self) -> &[ProvidedTheme] {
        &self.regs.themes
    }

    /// Replaces the MIDI mappings with the plugin's named map.
    pub fn apply_midi_map(&self, map_name: &str, mapper: &MidiMapper) -> Result<(), PluginError> {
        let entry = self
            .regs
            .maps
            .get(map_name)
            .ok_or_else(|| PluginError::UnknownMap(map_name.to_owned()))?;
        mapper
            .load_from_json(&entry.json)
            .map_err(|e| PluginError::InvalidMap {
                name: map_name.to_owned(),
                reason: e.to_string(),
            })
    }
}

impl Drop for PluginRegistry {
    fn drop(&mut self) {
        self.unload_all();
    }
}
