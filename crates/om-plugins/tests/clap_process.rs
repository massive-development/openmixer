//! A CLAP effect in the engine's stage chain: gain applied, no allocation on the audio path,
//! a faulting plugin bypassed, unloading with a live stage (ADR 0007, test plan "Process", "Unload").

mod clap_fixture;

use assert_no_alloc::{assert_no_alloc, AllocDisabler};
use clap_fixture::{fixture, PLUGIN_ID};
use om_engine::{StageChain, StageInsert};
use om_plugins::clap::{ClapLoader, ClapPlugin, Fault, StageState};
use om_plugins::{
    InsertAt, InsertPoint, InsertRefused, Plugin, PluginError, PluginHost, PluginRegistry,
    ProcessStage, StageFactory, StageKind,
};
use std::path::Path;
use std::sync::Arc;

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

/// The fixture's default gain (see the fixture crate).
const GAIN: f32 = 0.5;

/// A plugin host that only keeps the stage factory.
#[derive(Default)]
struct Capture {
    factory: Option<StageFactory>,
}

impl PluginHost for Capture {
    fn register_stage(
        &mut self,
        _id: &str,
        _kind: StageKind,
        f: StageFactory,
    ) -> Result<(), PluginError> {
        self.factory = Some(f);
        Ok(())
    }
    fn register_midi_map(&mut self, _: &str, _: &str) -> Result<(), PluginError> {
        Ok(())
    }
    fn register_parameter_target(
        &mut self,
        _: &str,
        _: Arc<dyn om_core::ParameterTarget>,
    ) -> Result<(), PluginError> {
        Ok(())
    }
    fn register_theme(&mut self, _: &str) -> Result<(), PluginError> {
        Ok(())
    }
}

/// An insert point over the engine's real stage chain.
struct ChainPoint(StageInsert);

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
        self.0.set(slot, stage).then_some(()).ok_or(InsertRefused)
    }
}

/// A loaded plugin, one live stage in a chain prepared at `rate`/`block`, and the stage's state.
struct Rig {
    _plugin: ClapPlugin,
    chain: StageChain,
    insert: StageInsert,
    state: Arc<StageState>,
}

fn rig(path: &Path, rate: f64, block: usize) -> Rig {
    let mut plugin = ClapLoader::load_typed(path, PLUGIN_ID).expect("load");
    let mut capture = Capture::default();
    plugin.on_load(&mut capture);
    let (mut chain, insert) = StageChain::new();
    chain.prepare(rate, block);
    let stage = (capture.factory.expect("stage registered"))();
    assert!(insert.set(0, Some(stage)));
    let state = plugin.stage_states().remove(0);
    Rig {
        _plugin: plugin,
        chain,
        insert,
        state,
    }
}

fn run(chain: &mut StageChain, frames: usize, level: f32) -> (Vec<f32>, Vec<f32>) {
    let (mut l, mut r) = (vec![level; frames], vec![-level; frames]);
    chain.process(&mut l, &mut r);
    (l, r)
}

#[test]
fn gain_is_applied_across_sample_rates_and_block_sizes() {
    for (rate, block) in [(44_100.0, 64), (48_000.0, 512), (96_000.0, 1024)] {
        let mut rig = rig(&fixture(None), rate, block);
        for frames in [1, 7, block, block + 13, 3 * block] {
            let (l, r) = run(&mut rig.chain, frames, 0.8);
            assert!(
                l.iter().all(|&s| (s - 0.8 * GAIN).abs() < 1e-6),
                "{rate} {block} {frames}"
            );
            assert!(
                r.iter().all(|&s| (s + 0.8 * GAIN).abs() < 1e-6),
                "{rate} {block} {frames}"
            );
        }
        assert_eq!(rig.state.fault(), None);
        assert_eq!(
            rig.state.latency_frames(),
            16,
            "read from the latency extension"
        );
        drop(rig.chain);
        rig.insert.reclaim();
    }
}

#[test]
fn silence_stays_silent_and_a_tone_is_scaled() {
    let mut rig = rig(&fixture(None), 48_000.0, 256);
    let (l, r) = run(&mut rig.chain, 256, 0.0);
    assert!(l.iter().chain(&r).all(|&s| s == 0.0));
    let tone: Vec<f32> = (0..256).map(|n| (n as f32 * 0.1).sin()).collect();
    let (mut l, mut r) = (tone.clone(), tone.clone());
    rig.chain.process(&mut l, &mut r);
    for (out, input) in l.iter().zip(&tone) {
        assert!((out - input * GAIN).abs() < 1e-6);
    }
}

#[test]
fn the_audio_path_does_not_allocate() {
    let mut rig = rig(&fixture(None), 48_000.0, 256);
    let (mut l, mut r) = (vec![0.5_f32; 600], vec![0.5_f32; 600]);
    run(&mut rig.chain, 256, 0.1); // first block starts processing
    for _ in 0..50 {
        assert_no_alloc(|| rig.chain.process(&mut l, &mut r));
    }
    assert_eq!(rig.state.fault(), None);
    assert!(rig.state.max_process_time() > std::time::Duration::ZERO);
}

#[test]
fn a_plugin_that_panics_is_bypassed_and_the_host_survives() {
    let mut rig = rig(&fixture(Some("panic-in-process")), 48_000.0, 128);
    let (l, r) = run(&mut rig.chain, 128, 0.7);
    assert!(
        l.iter().all(|&s| s == 0.7) && r.iter().all(|&s| s == -0.7),
        "dry signal passes"
    );
    // clack's plugin-side wrapper turns the panic into an error status; the host's own
    // `catch_unwind` (Fault::Panicked) is the second line for a panic that reaches it.
    assert!(
        matches!(
            rig.state.fault(),
            Some(Fault::ProcessFailed | Fault::Panicked)
        ),
        "{:?}",
        rig.state.fault()
    );
    let (l, _) = run(&mut rig.chain, 128, 0.3);
    assert!(l.iter().all(|&s| s == 0.3), "later blocks stay bypassed");
    assert!(rig.insert.clear(0));
}

#[test]
fn a_plugin_that_returns_an_error_is_bypassed() {
    let mut rig = rig(&fixture(Some("error-in-process")), 48_000.0, 128);
    let (l, _) = run(&mut rig.chain, 128, 0.7);
    assert!(l.iter().all(|&s| s == 0.7));
    assert_eq!(rig.state.fault(), Some(Fault::ProcessFailed));
}

#[test]
fn unloading_the_plugin_with_a_live_stage_is_safe() {
    let path = fixture(None);
    let mut registry = PluginRegistry::new();
    registry.acknowledge_clap_warning();
    let mut errors = Vec::new();
    assert_eq!(registry.load_clap(&path, &mut errors).expect("loads"), 1);
    let (mut chain, insert) = StageChain::new();
    chain.prepare(48_000.0, 256);
    let point: Arc<dyn InsertPoint> = Arc::new(ChainPoint(insert));
    registry
        .attach_stage(PLUGIN_ID, &point, InsertAt::Master, 0)
        .expect("attach");
    let (l, _) = run(&mut chain, 256, 1.0);
    assert!(l.iter().all(|&s| (s - GAIN).abs() < 1e-6));

    assert!(registry.unload(PLUGIN_ID));
    drop(registry);
    // The slot was cleared by the unload: the dry signal passes, and nothing crashes.
    let (l, _) = run(&mut chain, 256, 1.0);
    assert!(l.iter().all(|&s| s == 1.0));
    drop(chain);
    drop(point);
}
