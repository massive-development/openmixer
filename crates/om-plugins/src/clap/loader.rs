//! Opens `.clap` files, lists the plugins in them and adapts each to [`Plugin`]. Loading is open by
//! default (no allow-list, no signature check); the first-load warning is the registry's gate.

use super::entry;
use super::plugin::ClapPlugin;
use super::ports::StageDescriptor;
use super::probe;
use super::worker::Worker;
use crate::{Plugin, PluginError};
use clack_host::entry::PluginEntry;
use clack_host::factory::plugin::PluginFactory;
use std::ffi::CString;
use std::path::Path;

/// What a plugin descriptor says about the plugin, read without creating an instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClapPluginInfo {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub description: String,
    pub features: Vec<String>,
}

/// One result per plugin descriptor in a file.
pub type LoadedPlugins = Vec<Result<Box<dyn Plugin>, PluginError>>;

pub struct ClapLoader;

fn open(path: &Path) -> Result<PluginEntry, PluginError> {
    let shown = path.display().to_string();
    if !path.is_file() {
        return Err(PluginError::Io {
            path: shown,
            reason: "no such file".into(),
        });
    }
    entry::open(path).map_err(|e| PluginError::InvalidLibrary {
        path: shown,
        reason: e.to_string(),
    })
}

fn describe(entry: &PluginEntry, path: &Path) -> Result<Vec<ClapPluginInfo>, PluginError> {
    let no_factory = || PluginError::InvalidLibrary {
        path: path.display().to_string(),
        reason: "no CLAP plugin factory".into(),
    };
    let factory = entry
        .get_factory::<PluginFactory>()
        .ok_or_else(no_factory)?;
    let text = |s: Option<&std::ffi::CStr>| s.map_or(String::new(), |c| c.to_string_lossy().into());
    Ok(factory
        .plugin_descriptors()
        .filter_map(|d| {
            Some(ClapPluginInfo {
                id: d.id()?.to_string_lossy().into_owned(),
                name: text(d.name()),
                vendor: text(d.vendor()),
                version: text(d.version()),
                description: text(d.description()),
                features: d
                    .features()
                    .map(|f| f.to_string_lossy().into_owned())
                    .collect(),
            })
        })
        .collect())
}

impl ClapLoader {
    /// Lists the plugins in a `.clap` file. Opening it runs the library's initialisers.
    pub fn scan(path: &Path) -> Result<Vec<ClapPluginInfo>, PluginError> {
        describe(&open(path)?, path)
    }

    /// Loads the plugin `plugin_id` from `path`: creates a probe instance to read its ports and
    /// parameters, refuses layouts the host cannot use, then destroys the probe.
    pub fn load(path: &Path, plugin_id: &str) -> Result<Box<dyn Plugin>, PluginError> {
        Ok(Box::new(Self::load_typed(path, plugin_id)?))
    }

    /// [`Self::load`] keeping the concrete type, for callers that want its descriptor or GUI.
    pub fn load_typed(path: &Path, plugin_id: &str) -> Result<ClapPlugin, PluginError> {
        let entry = open(path)?;
        let info = describe(&entry, path)?
            .into_iter()
            .find(|i| i.id == plugin_id)
            .ok_or_else(|| PluginError::UnknownStage(plugin_id.to_owned()))?;
        Self::adapt(entry, info, path)
    }

    /// Loads every plugin in `path`; one result per descriptor, so a plugin the host cannot use does
    /// not hide the others.
    pub fn load_all(path: &Path) -> Result<LoadedPlugins, PluginError> {
        let entry = open(path)?;
        let infos = describe(&entry, path)?;
        Ok(infos
            .into_iter()
            .map(|info| Self::adapt(entry.clone(), info, path).map(|p| Box::new(p) as _))
            .collect())
    }

    fn adapt(
        entry: PluginEntry,
        info: ClapPluginInfo,
        path: &Path,
    ) -> Result<ClapPlugin, PluginError> {
        let id = CString::new(info.id.as_str()).map_err(|_| PluginError::InvalidLibrary {
            path: path.display().to_string(),
            reason: "plugin id contains a NUL byte".into(),
        })?;
        let probe = Worker::spawn(entry.clone(), id.clone(), &info.id).map_err(|e| match e {
            PluginError::InvalidLibrary { reason, .. } => PluginError::InvalidLibrary {
                path: path.display().to_string(),
                reason,
            },
            other => other,
        })?;
        let read = |what: &str| {
            PluginError::Rejected(format!(
                "{}: plugin thread ended while reading {what}",
                info.id
            ))
        };
        let layout = probe
            .call(probe::read_layout)
            .ok_or_else(|| read("ports"))?;
        let params = probe
            .call(probe::read_params)
            .ok_or_else(|| read("parameters"))?;
        drop(probe);
        let descriptor = StageDescriptor::effect_from(&layout, &info.id)?;
        Ok(ClapPlugin::new(entry, id, info, descriptor, params))
    }
}
