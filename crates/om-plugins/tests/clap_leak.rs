//! Leak check for the CLAP load, activate, process, deactivate and drop path (ADR 0007, decision 3).
//!
//! A counting allocator tracks live heap bytes in this process (the host side: clack, our wrappers,
//! the worker thread). The loop must not grow after a warm-up. Allocations made inside the fixture
//! plugin use the plugin library's own allocator and are covered by the valgrind run documented in
//! the PR (each cycle also saves, restores and re-restores plugin state): `valgrind --leak-check=full --errors-for-leak-kinds=definite,indirect --error-exitcode=9
//! target/debug/deps/clap_leak-<hash> --test-threads=1` with `CLAP_LEAK_ITERATIONS=15`.

#![cfg(feature = "clap")]

mod clap_fixture;

use clap_fixture::{fixture, PLUGIN_ID};
use om_core::{ParameterRouter, ParameterTarget};
use om_engine::StageChain;
use om_plugins::clap::ClapLoader;
use om_plugins::{InsertAt, InsertPoint, InsertRefused, PluginRegistry, ProcessStage};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};

/// Counts live heap bytes (alloc minus dealloc) across all threads.
struct Counting;

static LIVE: AtomicIsize = AtomicIsize::new(0);

// SAFETY: forwards every call to the system allocator unchanged and only adds to a counter.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract, which we pass on.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: `ptr` and `layout` come from a matching `alloc` call, as the caller guarantees.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        LIVE.fetch_add(
            new_size as isize - layout.size() as isize,
            Ordering::Relaxed,
        );
        // SAFETY: forwarded unchanged from the caller, who upholds `GlobalAlloc::realloc`'s contract.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

struct ChainPoint(Mutex<StageChain>, om_engine::StageInsert);

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
        self.1.set(slot, stage).then_some(()).ok_or(InsertRefused)
    }
}

/// One full life of a plugin: load, activate, process, change a parameter, deactivate, drop.
fn one_cycle(path: &std::path::Path, cycle: usize) {
    let mut registry = PluginRegistry::new();
    let router = Arc::new(ParameterRouter::new());
    registry.bind_router(Some(Arc::clone(&router)));
    registry.acknowledge_clap_warning();
    let mut errors = Vec::new();
    assert_eq!(registry.load_clap(path, &mut errors).expect("load"), 1);
    let (mut chain, insert) = StageChain::new();
    chain.prepare(
        [44_100.0, 48_000.0, 96_000.0][cycle % 3],
        [64, 256, 1024][cycle % 3],
    );
    let point = Arc::new(ChainPoint(Mutex::new(chain), insert));
    let dyn_point: Arc<dyn InsertPoint> = point.clone();
    registry
        .attach_stage(PLUGIN_ID, &dyn_point, InsertAt::Master, 0)
        .expect("attach");
    for block in 0..8 {
        assert!(router.set_parameter(&format!("{PLUGIN_ID}.1"), 0.1 * f64::from(block)));
        let (mut l, mut r) = (vec![0.5_f32; 300], vec![0.5_f32; 300]);
        point.0.lock().unwrap().process(&mut l, &mut r);
    }
    // State: save from the live instance, restore it into the live instance, then keep it for a
    // second load (the deferred path restores into the stage the next load creates).
    let states = registry.plugin_states();
    let state = states
        .get(PLUGIN_ID)
        .expect("the fixture has state")
        .clone();
    registry
        .restore_plugin_state(PLUGIN_ID, &state)
        .expect("restore into the live instance");
    assert!(registry
        .restore_plugin_state(PLUGIN_ID, b"not a state")
        .is_err());
    assert!(registry.take_state_errors().is_empty());
    // Replace the live stage (the old one is deactivated and freed on this thread), then unload.
    registry
        .attach_stage(PLUGIN_ID, &dyn_point, InsertAt::Master, 0)
        .expect("replace");
    point.1.reclaim();
    assert!(registry.unload(PLUGIN_ID));
    // Load again with the saved state: the new stage loads it before it activates.
    registry.set_saved_states(states);
    assert_eq!(registry.load_clap(path, &mut errors).expect("reload"), 1);
    registry
        .attach_stage(PLUGIN_ID, &dyn_point, InsertAt::Master, 0)
        .expect("attach after reload");
    let (mut l, mut r) = (vec![0.5_f32; 300], vec![0.5_f32; 300]);
    point.0.lock().unwrap().process(&mut l, &mut r);
    drop(registry);
    point.1.reclaim();
}

/// A probe-only cycle: scan, load (probe instance), drop without ever activating.
fn probe_cycle(path: &std::path::Path) {
    ClapLoader::scan(path).expect("scan");
    drop(ClapLoader::load_typed(path, PLUGIN_ID).expect("load"));
}

fn iterations() -> usize {
    std::env::var("CLAP_LEAK_ITERATIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300)
}

fn live() -> isize {
    LIVE.load(Ordering::Relaxed)
}

#[test]
fn load_activate_process_deactivate_drop_does_not_leak() {
    let path = fixture(None);
    let n = iterations();
    for cycle in 0..5 {
        one_cycle(&path, cycle);
        probe_cycle(&path);
    }
    let baseline = live();
    for cycle in 0..n {
        one_cycle(&path, cycle);
        probe_cycle(&path);
    }
    let grown = live() - baseline;
    eprintln!("live heap growth over {n} cycles: {grown} bytes (baseline {baseline})");
    // Allow a few KiB of one-off lazy state; a per-cycle leak of even 16 bytes would add up to 4800+.
    assert!(
        grown < 4096,
        "live heap grew by {grown} bytes over {n} cycles (baseline {baseline})"
    );
}

#[test]
fn a_faulting_plugin_does_not_leak_either() {
    let path = fixture(Some("error-in-process"));
    let n = iterations() / 3;
    for cycle in 0..3 {
        one_cycle_faulting(&path, cycle);
    }
    let baseline = live();
    for cycle in 0..n {
        one_cycle_faulting(&path, cycle);
    }
    let grown = live() - baseline;
    assert!(
        grown < 4096,
        "live heap grew by {grown} bytes over {n} cycles"
    );
}

fn one_cycle_faulting(path: &std::path::Path, _cycle: usize) {
    let mut registry = PluginRegistry::new();
    registry.acknowledge_clap_warning();
    let mut errors = Vec::new();
    assert_eq!(registry.load_clap(path, &mut errors).expect("load"), 1);
    let (mut chain, insert) = StageChain::new();
    chain.prepare(48_000.0, 128);
    let point = Arc::new(ChainPoint(Mutex::new(chain), insert));
    let dyn_point: Arc<dyn InsertPoint> = point.clone();
    registry
        .attach_stage(PLUGIN_ID, &dyn_point, InsertAt::Master, 0)
        .expect("attach");
    let (mut l, mut r) = (vec![0.5_f32; 128], vec![0.5_f32; 128]);
    point.0.lock().unwrap().process(&mut l, &mut r);
    drop(registry);
}
