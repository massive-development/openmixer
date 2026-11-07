//! A CLAP plugin as a [`Plugin`]: the registry treats it like an in-process or C-ABI plugin.

use super::loader::ClapPluginInfo;
use super::param_bus::{ClapTarget, ParamSet};
use super::params::ClapParam;
use super::ports::StageDescriptor;
use super::stage::ClapStage;
use super::state::{Fault, StageState};
use super::worker::Worker;
use crate::{ParameterInfo, Plugin, PluginHost, PluginInfo, StageKind};
use clack_host::entry::PluginEntry;
use std::ffi::CString;
use std::sync::{Arc, Mutex, Weak};

pub struct ClapPlugin {
    // Held so the library stays mapped for as long as the plugin is loaded.
    entry: PluginEntry,
    plugin_id: CString,
    info: ClapPluginInfo,
    descriptor: StageDescriptor,
    params: Arc<ParamSet>,
    stages: Arc<Mutex<Vec<Weak<StageState>>>>,
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
            entry,
            plugin_id,
            info,
            descriptor,
            params: Arc::new(ParamSet::new(params)),
            stages: Arc::default(),
        }
    }

    /// The state of every live stage this plugin made (faults, latency, cost). Prunes dead ones.
    pub fn stage_states(&self) -> Vec<Arc<StageState>> {
        let mut stages = self.stages.lock().unwrap_or_else(|e| e.into_inner());
        stages.retain(|w| w.strong_count() > 0);
        stages.iter().filter_map(Weak::upgrade).collect()
    }

    pub fn descriptor(&self) -> &StageDescriptor {
        &self.descriptor
    }

    /// The declared parameters (hidden ones included).
    pub fn clap_parameters(&self) -> &[ClapParam] {
        &self.params.params
    }

    /// The host's latest value of a parameter, by CLAP id (changes the plugin made itself included).
    pub fn param_value(&self, param_id: u32) -> Option<f64> {
        self.params
            .index_of_id(param_id)
            .map(|i| self.params.value(i))
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
        self.params.infos()
    }

    fn parameter(&self, id: &str) -> Option<f64> {
        self.param_value(id.parse().ok()?)
    }

    fn on_load(&mut self, host: &mut dyn PluginHost) {
        let (entry, id) = (self.entry.clone(), self.plugin_id.clone());
        let (stages, label) = (Arc::clone(&self.stages), self.info.id.clone());
        let params = Arc::clone(&self.params);
        let factory = Box::new(move || {
            let state = Arc::new(StageState::default());
            stages
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(Arc::downgrade(&state));
            let worker = Worker::spawn(entry.clone(), id.clone(), &label)
                .inspect_err(
                    |error| tracing::warn!(%error, plugin = %label, "cannot create CLAP instance"),
                )
                .ok();
            if worker.is_none() {
                state.set_fault(Fault::ActivationFailed);
            }
            Box::new(ClapStage::new(worker, state, Arc::clone(&params)))
                as Box<dyn crate::ProcessStage>
        });
        // A stage id another plugin took leaves this one without a stage; the Plugins page shows it.
        let _ = host.register_stage(&self.info.id, StageKind::Fx, factory);
        if !self.params.params.is_empty() {
            let target = Arc::new(ClapTarget {
                set: Arc::clone(&self.params),
            });
            let _ = host.register_parameter_target(&self.info.id, target);
        }
    }
}
