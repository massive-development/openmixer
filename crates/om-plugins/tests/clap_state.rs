//! CLAP plugin state: saved from a live instance, restored into a later one, kept across unload and
//! refused cleanly when the bytes are bad (ADR 0007, step 6).

#![cfg(feature = "clap")]

mod clap_fixture;

use clap_fixture::{fixture, PLUGIN_ID};
use om_core::{ParameterRouter, ParameterTarget};
use om_engine::{StageChain, StageInsert};
use om_plugins::{
    InsertAt, InsertPoint, InsertRefused, Plugin, PluginError, PluginRegistry, ProcessStage,
    MAX_STATE_BYTES,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

struct ChainPoint {
    insert: StageInsert,
    chain: Mutex<StageChain>,
}

impl InsertPoint for ChainPoint {
    fn channel_count(&self) -> usize {
        1
    }
    fn set_insert(
        &self,
        _at: InsertAt,
        slot: usize,
        stage: Option<Box<dyn ProcessStage>>,
    ) -> Result<(), InsertRefused> {
        self.insert
            .set(slot, stage)
            .then_some(())
            .ok_or(InsertRefused)
    }
}

impl ChainPoint {
    fn run(&self, level: f32) -> f32 {
        let (mut l, mut r) = (vec![level; 128], vec![level; 128]);
        self.chain.lock().unwrap().process(&mut l, &mut r);
        l[127]
    }
}

fn registry(feature: Option<&str>) -> (PluginRegistry, Arc<ParameterRouter>) {
    let mut registry = PluginRegistry::new();
    let router = Arc::new(ParameterRouter::new());
    registry.bind_router(Some(Arc::clone(&router)));
    registry.acknowledge_clap_warning();
    load(&mut registry, feature);
    (registry, router)
}

fn load(registry: &mut PluginRegistry, feature: Option<&str>) {
    let mut errors = Vec::new();
    let path = fixture(feature);
    assert_eq!(registry.load_clap(&path, &mut errors).expect("loads"), 1);
}

fn attach(registry: &mut PluginRegistry) -> Arc<ChainPoint> {
    let (mut chain, insert) = StageChain::new();
    chain.prepare(48_000.0, 128);
    let point = Arc::new(ChainPoint {
        insert,
        chain: Mutex::new(chain),
    });
    let dyn_point: Arc<dyn InsertPoint> = point.clone();
    registry
        .attach_stage(PLUGIN_ID, &dyn_point, InsertAt::Master, 0)
        .expect("attach");
    point
}

fn set(router: &ParameterRouter, value: f64) {
    assert!(router.set_parameter(&format!("{PLUGIN_ID}.1"), value));
}

#[test]
fn state_saved_from_a_live_instance_restores_into_a_new_session() {
    let (mut first, router) = registry(None);
    let point = attach(&mut first);
    set(&router, 1.25);
    assert!((point.run(1.0) - 1.25).abs() < 1e-6);
    let states = first.plugin_states();
    let bytes = states.get(PLUGIN_ID).expect("the plugin has state").clone();
    assert!(
        bytes.starts_with(b"GAIN"),
        "the bytes are the plugin's own: {bytes:?}"
    );
    drop(first);

    // A new session: the saved state goes in before the plugin loads.
    let mut second = PluginRegistry::new();
    second.set_saved_states(states);
    second.acknowledge_clap_warning();
    load(&mut second, None);
    assert!(second.take_state_errors().is_empty());
    let point = attach(&mut second);
    assert!(
        (point.run(1.0) - 1.25).abs() < 1e-6,
        "the new instance starts from the saved state, not the default 0.5"
    );
    assert_eq!(second.plugin_parameter(PLUGIN_ID, "1"), Some(1.25));
}

#[test]
fn restore_reaches_a_live_instance_and_the_parameter_snapshot() {
    let (mut registry, _router) = registry(None);
    let point = attach(&mut registry);
    assert!((point.run(1.0) - 0.5).abs() < 1e-6);
    let mut state = b"GAIN".to_vec();
    state.push(1);
    state.extend_from_slice(&1.75_f32.to_le_bytes());
    registry
        .restore_plugin_state(PLUGIN_ID, &state)
        .expect("restores");
    assert_eq!(registry.plugin_parameter(PLUGIN_ID, "1"), Some(1.75));
    assert!((point.run(1.0) - 1.75).abs() < 1e-6);
    // An unattached plugin keeps the state it was given, too.
    let plugin = om_plugins::clap::ClapLoader::load_typed(&fixture(None), PLUGIN_ID).expect("load");
    plugin.restore_state(&state).expect("restores");
    assert_eq!(
        plugin.save_state().expect("saves").as_deref(),
        Some(state.as_slice())
    );
}

#[test]
fn unloading_keeps_the_state_for_the_next_load() {
    let (mut registry, router) = registry(None);
    let point = attach(&mut registry);
    set(&router, 0.75);
    assert!((point.run(1.0) - 0.75).abs() < 1e-6);
    // The parameter change reached the plugin in `process`; its state now holds 0.75.
    assert!(registry.unload(PLUGIN_ID));
    load(&mut registry, None);
    assert!(registry.take_state_errors().is_empty());
    let point = attach(&mut registry);
    assert!(
        (point.run(1.0) - 0.75).abs() < 1e-6,
        "reload restores what the plugin held when it unloaded"
    );
}

#[test]
fn bad_bytes_are_refused_live_and_ignored_with_defaults_when_deferred() {
    // Deferred: the plugin has no instance yet, so the bytes are tried when the stage is made. The
    // plugin refuses them (logged) and the stage runs with its defaults.
    let mut registry = PluginRegistry::new();
    registry.set_saved_states(BTreeMap::from([(
        PLUGIN_ID.to_owned(),
        b"nonsense".to_vec(),
    )]));
    registry.acknowledge_clap_warning();
    load(&mut registry, None);
    let point = attach(&mut registry);
    assert!((point.run(1.0) - 0.5).abs() < 1e-6, "defaults");
    // Live: the refusal is returned.
    let error = registry
        .restore_plugin_state(PLUGIN_ID, b"nonsense")
        .expect_err("refused");
    assert!(matches!(error, PluginError::State { .. }), "{error}");
    assert!((point.run(1.0) - 0.5).abs() < 1e-6, "still defaults");
}

#[test]
fn a_plugin_that_rejects_state_reports_it_and_still_runs() {
    let (mut registry, _router) = registry(Some("reject-state"));
    let point = attach(&mut registry);
    let live = registry.plugin_states();
    assert!(live.contains_key(PLUGIN_ID), "saving still works");
    assert!(registry
        .restore_plugin_state(PLUGIN_ID, &live[PLUGIN_ID])
        .is_err());
    assert!((point.run(1.0) - 0.5).abs() < 1e-6);
}

#[test]
fn an_oversized_state_is_refused_before_it_reaches_the_plugin() {
    let plugin = om_plugins::clap::ClapLoader::load_typed(&fixture(None), PLUGIN_ID).expect("load");
    let huge = vec![0_u8; MAX_STATE_BYTES + 1];
    assert!(matches!(
        plugin.restore_state(&huge),
        Err(PluginError::State { .. })
    ));
}
