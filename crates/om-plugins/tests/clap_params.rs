//! CLAP parameters: a router or UI change reaches the plugin as a parameter event in the next block,
//! a change the plugin makes itself shows in the host snapshot (ADR 0007, test plan "Parameters").

mod clap_fixture;

use assert_no_alloc::{assert_no_alloc, AllocDisabler};
use clap_fixture::{fixture, PLUGIN_ID};
use om_core::{ParameterRouter, ParameterTarget};
use om_engine::{StageChain, StageInsert};
use om_plugins::{InsertAt, InsertPoint, InsertRefused, PluginRegistry, ProcessStage};
use std::sync::{Arc, Mutex};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

/// An insert point over the engine's real stage chain; the chain is shared so tests can run it.
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

struct Setup {
    registry: PluginRegistry,
    router: Arc<ParameterRouter>,
    point: Arc<ChainPoint>,
}

fn setup(feature: Option<&str>) -> Setup {
    let mut registry = PluginRegistry::new();
    let router = Arc::new(ParameterRouter::new());
    registry.bind_router(Some(Arc::clone(&router)));
    registry.acknowledge_clap_warning();
    let mut errors = Vec::new();
    assert_eq!(
        registry
            .load_clap(&fixture(feature), &mut errors)
            .expect("loads"),
        1
    );
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
    Setup {
        registry,
        router,
        point,
    }
}

fn set(router: &ParameterRouter, value: f64) -> bool {
    router.set_parameter(&format!("{PLUGIN_ID}.1"), value)
}

#[test]
fn a_router_change_reaches_the_plugin_in_the_next_block() {
    let s = setup(None);
    assert!(
        (s.point.run(1.0) - 0.5).abs() < 1e-6,
        "starts at the default"
    );
    assert!(set(&s.router, 2.0));
    assert!((s.point.run(1.0) - 2.0).abs() < 1e-6);
    assert!(set(&s.router, 0.25));
    assert!((s.point.run(1.0) - 0.25).abs() < 1e-6);
    // Out of range values are clamped by the host; unknown ids and non-finite values are refused.
    assert!(set(&s.router, 9.0));
    assert!((s.point.run(1.0) - 2.0).abs() < 1e-6);
    assert!(!s.router.set_parameter(&format!("{PLUGIN_ID}.99"), 1.0));
    assert!(!set(&s.router, f64::NAN));
    assert_eq!(s.registry.plugin_parameter(PLUGIN_ID, "1"), Some(2.0));
}

#[test]
fn a_value_set_before_the_first_block_is_not_lost() {
    let s = setup(None);
    for v in [0.1, 0.2, 0.3, 1.25] {
        assert!(set(&s.router, v));
    }
    assert!((s.point.run(1.0) - 1.25).abs() < 1e-6);
}

#[test]
fn the_parameter_path_does_not_allocate() {
    let s = setup(None);
    s.point.run(0.1);
    for n in 0..20 {
        assert!(set(&s.router, f64::from(n) * 0.1));
        let (mut l, mut r) = (vec![0.5_f32; 128], vec![0.5_f32; 128]);
        let mut chain = s.point.chain.lock().unwrap();
        assert_no_alloc(|| chain.process(&mut l, &mut r));
    }
}

#[test]
fn a_change_the_plugin_makes_itself_appears_in_the_host_snapshot() {
    let s = setup(Some("emit-param-change"));
    assert_eq!(s.registry.plugin_parameter(PLUGIN_ID, "1"), Some(0.5));
    s.point.run(1.0);
    // The plugin set its gain to 1.5 and reported it in its output events.
    assert_eq!(s.registry.plugin_parameter(PLUGIN_ID, "1"), Some(1.5));
}

#[test]
fn two_stages_of_one_plugin_both_follow_the_parameter() {
    let mut s = setup(None);
    let (mut chain, insert) = StageChain::new();
    chain.prepare(48_000.0, 128);
    let second = Arc::new(ChainPoint {
        insert,
        chain: Mutex::new(chain),
    });
    let dyn_point: Arc<dyn InsertPoint> = second.clone();
    s.registry
        .attach_stage(PLUGIN_ID, &dyn_point, InsertAt::Master, 0)
        .expect("attach second");
    assert!(set(&s.router, 1.5));
    assert!((s.point.run(1.0) - 1.5).abs() < 1e-6);
    assert!(
        (second.run(1.0) - 1.5).abs() < 1e-6,
        "the second stage synced at its first block"
    );
}
