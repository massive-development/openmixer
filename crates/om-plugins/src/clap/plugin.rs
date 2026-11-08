//! A CLAP plugin as a [`Plugin`]: the registry treats it like an in-process or C-ABI plugin.

use super::gui::PluginGui;
use super::loader::ClapPluginInfo;
use super::param_bus::{ClapTarget, ParamSet};
use super::params::ClapParam;
use super::persist;
use super::ports::StageDescriptor;
use super::stage::ClapStage;
use super::state::{Fault, StageState};
use super::worker::Worker;
use crate::{ParameterInfo, Plugin, PluginError, PluginHost, PluginInfo, StageKind};
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
    /// The latest state seen: restored from the app's store or saved from a live instance. New
    /// stages load it before they activate, so a plugin with no stage keeps its state too.
    state: Arc<Mutex<Option<Vec<u8>>>>,
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
            state: Arc::default(),
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

/// Loads the latest saved state into a new instance, before it activates. A refusal is logged and
/// the instance keeps its defaults.
fn restore_into(
    worker: &Worker,
    label: &str,
    saved: &Mutex<Option<Vec<u8>>>,
    params: &Arc<ParamSet>,
) {
    let Some(bytes) = saved.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
        return;
    };
    let (plugin, params) = (label.to_owned(), Arc::clone(params));
    match worker.call(move |ctx| persist::load(ctx, &plugin, &bytes, &params)) {
        Some(Ok(())) => {}
        Some(Err(error)) => tracing::warn!(%error, plugin = %label, "cannot restore CLAP state"),
        None => tracing::warn!(plugin = %label, "plugin thread ended while restoring state"),
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

    /// The state of the first live instance (and remembered), else the last state restored. `None`
    /// when the plugin has no `state` extension and nothing was restored.
    fn save_state(&self) -> Result<Option<Vec<u8>>, PluginError> {
        let live = self.stage_states().iter().find_map(|s| s.worker());
        if let Some(worker) = live {
            let plugin = self.info.id.clone();
            let saved = worker
                .call(move |ctx| persist::save(ctx, &plugin))
                .ok_or_else(|| PluginError::State {
                    plugin: self.info.id.clone(),
                    reason: "the plugin's control thread has ended".into(),
                })??;
            if let Some(bytes) = saved {
                *self.state.lock().unwrap_or_else(|e| e.into_inner()) = Some(bytes);
            }
        }
        Ok(self.state.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }

    /// Remembers `state` for stages made later and loads it into every live instance.
    fn restore_state(&self, state: &[u8]) -> Result<(), PluginError> {
        if state.len() > crate::MAX_STATE_BYTES {
            return Err(PluginError::State {
                plugin: self.info.id.clone(),
                reason: "plugin state is too large".into(),
            });
        }
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = Some(state.to_vec());
        let mut failure = None;
        for worker in self.stage_states().iter().filter_map(|s| s.worker()) {
            let (plugin, bytes) = (self.info.id.clone(), state.to_vec());
            let params = Arc::clone(&self.params);
            let result = worker.call(move |ctx| persist::load(ctx, &plugin, &bytes, &params));
            if let Some(Err(error)) = result {
                failure.get_or_insert(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }

    fn clap_gui(&self) -> Option<PluginGui> {
        self.stage_states().iter().find_map(|s| s.gui())
    }

    fn on_load(&mut self, host: &mut dyn PluginHost) {
        let (entry, id) = (self.entry.clone(), self.plugin_id.clone());
        let (stages, label) = (Arc::clone(&self.stages), self.info.id.clone());
        let params = Arc::clone(&self.params);
        let saved = Arc::clone(&self.state);
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
            match &worker {
                Some(w) => {
                    state.set_gui(PluginGui::new(w.handle()));
                    state.set_worker(w.handle());
                    restore_into(w, &label, &saved, &params);
                }
                None => state.set_fault(Fault::ActivationFailed),
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
