//! A CLAP plugin as a [`Plugin`]: the registry treats it like an in-process or C-ABI plugin.

use super::loader::ClapPluginInfo;
use super::params::ClapParam;
use super::ports::StageDescriptor;
use crate::{ParameterInfo, Plugin, PluginHost, PluginInfo};
use clack_host::entry::PluginEntry;
use std::ffi::CString;

pub struct ClapPlugin {
    // Held so the library stays mapped for as long as the plugin is loaded.
    _entry: PluginEntry,
    _plugin_id: CString,
    info: ClapPluginInfo,
    descriptor: StageDescriptor,
    params: Vec<(ClapParam, f64)>,
}

impl ClapPlugin {
    pub(super) fn new(
        entry: PluginEntry,
        plugin_id: CString,
        info: ClapPluginInfo,
        descriptor: StageDescriptor,
        params: Vec<(ClapParam, f64)>,
    ) -> Self {
        Self {
            _entry: entry,
            _plugin_id: plugin_id,
            info,
            descriptor,
            params,
        }
    }

    pub fn descriptor(&self) -> &StageDescriptor {
        &self.descriptor
    }

    /// The declared parameters with their values at load time.
    pub fn clap_parameters(&self) -> &[(ClapParam, f64)] {
        &self.params
    }
}

impl Plugin for ClapPlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: self.info.id.clone(),
            name: self.info.name.clone(),
            version: self.info.version.clone(),
        }
    }

    fn parameters(&self) -> Vec<ParameterInfo> {
        self.params
            .iter()
            .filter(|(p, _)| !p.hidden)
            .map(|(p, _)| ParameterInfo {
                id: p.id.to_string(),
                name: p.name.clone(),
                min: p.min,
                max: p.max,
                default: p.default,
            })
            .collect()
    }

    fn parameter(&self, id: &str) -> Option<f64> {
        let id: u32 = id.parse().ok()?;
        self.params
            .iter()
            .find(|(p, _)| p.id == id)
            .map(|(_, v)| *v)
    }

    fn on_load(&mut self, _host: &mut dyn PluginHost) {}
}
